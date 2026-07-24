#include <fcb/error.hpp>
#include <fcb/writer/btree_builder.hpp>
#include <fcb/writer/fcb_writer.hpp>
#include <fcb/writer/feature_serializer.hpp>

#include <algorithm>
#include <limits>
#include <sstream>

#if defined(_WIN32)
#    define FCB_FSEEK _fseeki64
using fcb_off_t = long long;
#else
#    include <sys/types.h>
#    define FCB_FSEEK fseeko
using fcb_off_t = off_t;
#endif

namespace fcb {

namespace {

// "fcb" + VERSION + "fcb" + 0 (const_vars.rs:5, layout.hpp's kVersion == 1).
constexpr std::uint8_t kMagicBytes[8] = {'f', 'c', 'b', kVersion, 'f', 'c', 'b', 0};

// Feature bytes are streamed from the spool to the output in chunks this
// size, rather than ever materializing a whole feature (let alone the
// whole feature section) as one buffer.
constexpr std::size_t kCopyChunkSize = 1 << 16;  // 64 KiB

}  // namespace

FcbWriter::FcbWriter(nlohmann::ordered_json cj, FcbWriterOptions options,
                     AttributeSchema attr_schema,
                     std::optional<AttributeSchema> semantic_attr_schema)
    : cj_(std::move(cj)), options_(std::move(options)), attr_schema_(std::move(attr_schema)),
      semantic_attr_schema_(std::move(semantic_attr_schema)) {
    for (const auto& [name, unused] : options_.attribute_indices)
        indexing_attr_.push_back(name);

    const auto& transform = cj_.at("transform");
    scale_x_ = transform.at("scale").at(0).get<double>();
    scale_y_ = transform.at("scale").at(1).get<double>();
    translate_x_ = transform.at("translate").at(0).get<double>();
    translate_y_ = transform.at("translate").at(1).get<double>();

    tmp_ = std::tmpfile();
    if (tmp_ == nullptr) {
        throw Error(ErrorCode::IoError, "FcbWriter: failed to create a temporary file for feature "
                                        "spooling");
    }
}

FcbWriter::~FcbWriter() {
    if (tmp_ != nullptr) {
        std::fclose(tmp_);  // removes the file too: that is std::tmpfile()'s whole contract
    }
}

void FcbWriter::add_feature(const nlohmann::ordered_json& feature) {
    if (written_) {
        throw Error(ErrorCode::IoError, "FcbWriter::add_feature called after write()");
    }

    flatbuffers::FlatBufferBuilder fbb;
    auto [off, raw_bbox] =
        to_fcb_city_feature(fbb, feature.at("id").get<std::string>(), feature, attr_schema_,
                            semantic_attr_schema_ ? &*semantic_attr_schema_ : nullptr);
    fbb.FinishSizePrefixed(off);
    const std::uint64_t size = fbb.GetSize();

    if (size > 0) {
        if (FCB_FSEEK(tmp_, static_cast<fcb_off_t>(tmp_write_pos_), SEEK_SET) != 0 ||
            std::fwrite(fbb.GetBufferPointer(), 1, size, tmp_) != size) {
            throw Error(ErrorCode::IoError, "FcbWriter: failed writing a feature to the temp file");
        }
    }

    const std::uint64_t temp_id = feat_offsets_.size();
    feat_offsets_.push_back(FeatureSlot{tmp_write_pos_, size});
    tmp_write_pos_ += size;

    feat_nodes_.push_back(NodeItem{raw_bbox.min_x * scale_x_ + translate_x_,
                                   raw_bbox.min_y * scale_y_ + translate_y_,
                                   raw_bbox.max_x * scale_x_ + translate_x_,
                                   raw_bbox.max_y * scale_y_ + translate_y_, temp_id});

    if (!indexing_attr_.empty()) {
        index_entries_by_feature_.push_back(
            cityfeature_to_index_entries(feature, attr_schema_, indexing_attr_));
    } else {
        index_entries_by_feature_.emplace_back();
    }
}

void FcbWriter::write(std::ostream& out) {
    if (written_) {
        throw Error(ErrorCode::IoError, "FcbWriter::write called more than once");
    }
    written_ = true;
    std::fflush(tmp_);

    // `write_index: false` forces index_node_size to 0 in the header,
    // exactly like `HeaderWriter::new_with_options` (header_writer.rs:
    // 87-94) -- computed once here so every downstream decision (whether
    // to hilbert_sort, whether to build an R-tree, what the header
    // records) uses the SAME effective value.
    const std::uint16_t effective_node_size = options_.write_index ? options_.index_node_size : 0;

    // Rust's own `if index_node_size > 0 && !feat_nodes.is_empty()` guards
    // `hilbert_sort` itself, not just the R-tree build (writer/mod.rs:
    // 208-225) -- when it's false, features stay in ORIGINAL order (each
    // `.offset` still its temp id from `add_feature`).
    NodeItem extent = NodeItem::empty(0);
    const bool build_rtree = effective_node_size > 0 && !feat_nodes_.empty();
    if (build_rtree) {
        extent = calc_extent(feat_nodes_);
        hilbert_sort(feat_nodes_, extent);
    }

    // Bookkeeping-only pass: compute each feature's FINAL byte offset in
    // the (sorted or original) output order, WITHOUT reading any feature
    // bytes yet -- `feat_nodes_` itself is left untouched (`.offset` still
    // each entry's original temp id) so the later streaming pass can still
    // look up where in the spool file to read each one from.
    std::vector<std::uint64_t> final_offset_by_temp_id(feat_offsets_.size());
    {
        std::uint64_t running = 0;
        for (const auto& node : feat_nodes_) {
            final_offset_by_temp_id[static_cast<std::size_t>(node.offset)] = running;
            running += feat_offsets_[static_cast<std::size_t>(node.offset)].size;
        }
    }

    std::vector<std::uint8_t> rtree_bytes;
    if (build_rtree) {
        // A throwaway copy with `.offset` remapped to final byte offsets --
        // `feat_nodes_` itself keeps carrying temp ids for the streaming
        // pass below.
        std::vector<NodeItem> rtree_input = feat_nodes_;
        for (auto& node : rtree_input)
            node.offset = final_offset_by_temp_id[static_cast<std::size_t>(node.offset)];
        std::vector<NodeItem> tree = build_packed_rtree(rtree_input, extent, effective_node_size);
        rtree_bytes = encode_packed_rtree(tree);
    }

    // Per-column attribute index dispatch (writer/mod.rs:192-202,252-265),
    // sorted by SCHEMA COLUMN INDEX (not request order). A requested name
    // absent from the schema, or with zero indexable entries, is silently
    // skipped -- mirrors `if let Ok(...) = build_attribute_index_for_attr(
    // ...)` (writer/mod.rs:255-264), which discards an `Err` (from either
    // `Error::AttributeIndexNotFound` or `Stree::init`'s empty-tree check)
    // rather than propagating it.
    std::vector<std::pair<std::string, std::optional<std::uint16_t>>> sorted_indices =
        options_.attribute_indices;
    std::stable_sort(
        sorted_indices.begin(), sorted_indices.end(), [this](const auto& a, const auto& b) {
            const auto ia = attr_schema_.find(a.first);
            const auto ib = attr_schema_.find(b.first);
            const std::uint16_t idx_a = ia != attr_schema_.end()
                                            ? ia->second.first
                                            : std::numeric_limits<std::uint16_t>::max();
            const std::uint16_t idx_b = ib != attr_schema_.end()
                                            ? ib->second.first
                                            : std::numeric_limits<std::uint16_t>::max();
            return idx_a < idx_b;
        });

    std::vector<std::uint8_t> attr_index_bytes;
    std::vector<AttributeIndexInfo> attr_index_info;
    if (!sorted_indices.empty()) {
        std::vector<std::vector<BtreeEntry>> entries_by_column(attr_schema_.size());
        for (std::size_t temp_id = 0; temp_id < feat_offsets_.size(); ++temp_id) {
            const std::uint64_t feature_offset = final_offset_by_temp_id[temp_id];
            for (const auto& e : index_entries_by_feature_[temp_id])
                entries_by_column.at(e.index).push_back(BtreeEntry{e.value, feature_offset});
        }

        for (const auto& [name, bf_opt] : sorted_indices) {
            const auto it = attr_schema_.find(name);
            if (it == attr_schema_.end())
                continue;
            const std::uint16_t schema_index = it->second.first;
            const auto& col_entries = entries_by_column.at(schema_index);
            if (col_entries.empty())
                continue;

            const KeyKind kind = key_kind_for_column(static_cast<std::uint8_t>(it->second.second));
            const std::uint16_t branching_factor = bf_opt.value_or(kDefaultBranchingFactor);
            BuiltBtreeIndex built = build_static_btree(col_entries, kind, branching_factor);

            attr_index_info.push_back(
                AttributeIndexInfo{schema_index, static_cast<std::uint32_t>(built.bytes.size()),
                                   built.branching_factor, built.num_unique_items});
            attr_index_bytes.insert(attr_index_bytes.end(), built.bytes.begin(), built.bytes.end());
        }
    }

    HeaderWriterOptions header_options;
    header_options.feature_count = feat_offsets_.size();
    header_options.index_node_size = effective_node_size;
    header_options.geographical_extent = options_.geographical_extent;

    flatbuffers::FlatBufferBuilder header_fbb;
    auto header_off = to_fcb_header(header_fbb, cj_, header_options, attr_schema_,
                                    semantic_attr_schema_ ? &*semantic_attr_schema_ : nullptr,
                                    attr_index_info.empty() ? nullptr : &attr_index_info);
    header_fbb.FinishSizePrefixed(header_off);

    out.write(reinterpret_cast<const char*>(kMagicBytes), sizeof(kMagicBytes));
    out.write(reinterpret_cast<const char*>(header_fbb.GetBufferPointer()),
              static_cast<std::streamsize>(header_fbb.GetSize()));
    if (!rtree_bytes.empty())
        out.write(reinterpret_cast<const char*>(rtree_bytes.data()),
                  static_cast<std::streamsize>(rtree_bytes.size()));
    if (!attr_index_bytes.empty())
        out.write(reinterpret_cast<const char*>(attr_index_bytes.data()),
                  static_cast<std::streamsize>(attr_index_bytes.size()));
    if (!out) {
        throw Error(ErrorCode::IoError, "FcbWriter: failed writing the header/index sections");
    }

    // Stream every feature's bytes straight from the spool to `out`, in
    // `feat_nodes_`'s CURRENT (sorted or original) order, through a fixed-
    // size buffer -- never holding more than one chunk of feature data (let
    // alone the whole feature section) in memory at once.
    std::vector<char> chunk(kCopyChunkSize);
    for (const auto& node : feat_nodes_) {
        const FeatureSlot& slot = feat_offsets_[static_cast<std::size_t>(node.offset)];
        if (slot.size == 0)
            continue;
        if (FCB_FSEEK(tmp_, static_cast<fcb_off_t>(slot.offset), SEEK_SET) != 0) {
            throw Error(ErrorCode::IoError, "FcbWriter: failed to seek the temp file");
        }
        std::uint64_t remaining = slot.size;
        while (remaining > 0) {
            const std::size_t want =
                static_cast<std::size_t>(std::min<std::uint64_t>(remaining, chunk.size()));
            if (std::fread(chunk.data(), 1, want, tmp_) != want) {
                throw Error(ErrorCode::IoError,
                            "FcbWriter: failed reading a feature back from the temp file");
            }
            out.write(chunk.data(), static_cast<std::streamsize>(want));
            if (!out) {
                throw Error(ErrorCode::IoError,
                            "FcbWriter: failed writing a feature to the output");
            }
            remaining -= want;
        }
    }
}

std::vector<std::uint8_t> FcbWriter::write() {
    std::ostringstream oss(std::ios::binary);
    write(oss);
    const std::string& s = oss.str();
    return std::vector<std::uint8_t>(s.begin(), s.end());
}

}  // namespace fcb
