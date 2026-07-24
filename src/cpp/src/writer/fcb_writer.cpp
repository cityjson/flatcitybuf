#include <fcb/error.hpp>
#include <fcb/writer/btree_builder.hpp>
#include <fcb/writer/fcb_writer.hpp>
#include <fcb/writer/feature_serializer.hpp>

#include <algorithm>
#include <atomic>
#include <chrono>
#include <limits>

namespace fcb {

namespace {

// "fcb" + VERSION + "fcb" + 0 (const_vars.rs:5, layout.hpp's kVersion == 1).
constexpr std::uint8_t kMagicBytes[8] = {'f', 'c', 'b', kVersion, 'f', 'c', 'b', 0};

/// A private, uniquely-named path under the system temp directory. Not
/// cryptographically unique -- just unlikely to collide in practice, which
/// is all a same-process spool file needs (mirrors Rust's `tempfile` crate,
/// which similarly relies on OS-assisted uniqueness rather than a
/// cryptographic guarantee).
std::filesystem::path make_temp_path() {
    static std::atomic<std::uint64_t> counter{0};
    const auto n = counter.fetch_add(1, std::memory_order_relaxed);
    const auto now = std::chrono::steady_clock::now().time_since_epoch().count();
    return std::filesystem::temp_directory_path() /
           ("fcb_writer_" + std::to_string(now) + "_" + std::to_string(n) + ".tmp");
}

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

    tmp_path_ = make_temp_path();
    tmp_.open(tmp_path_, std::ios::binary | std::ios::in | std::ios::out | std::ios::trunc);
    if (!tmp_.is_open()) {
        throw Error(ErrorCode::IoError,
                    "FcbWriter: failed to create a temporary file for feature spooling at " +
                        tmp_path_.string());
    }
}

FcbWriter::~FcbWriter() {
    tmp_.close();
    std::error_code ec;  // best-effort cleanup; a destructor must not throw
    std::filesystem::remove(tmp_path_, ec);
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
        tmp_.seekp(static_cast<std::streamoff>(tmp_write_pos_));
        tmp_.write(reinterpret_cast<const char*>(fbb.GetBufferPointer()),
                   static_cast<std::streamsize>(size));
        if (!tmp_) {
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

std::vector<std::uint8_t> FcbWriter::write() {
    if (written_) {
        throw Error(ErrorCode::IoError, "FcbWriter::write called more than once");
    }
    written_ = true;
    tmp_.flush();

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

    // Read every feature back from the spool file, in `feat_nodes_`'s
    // CURRENT order (sorted or original), concatenating into the final
    // features section. Records each feature's FINAL byte offset both on
    // its `NodeItem` (consumed by the R-tree build right after) and,
    // indexed by ORIGINAL id, for attribute-index entries below -- the one
    // pass where both trees' offsets come from the same "final feature
    // position" computation, so it must run before either is built.
    std::vector<std::uint8_t> sorted_feature_bytes;
    std::vector<std::uint64_t> final_offset_by_temp_id(feat_offsets_.size());
    for (auto& node : feat_nodes_) {
        const std::uint64_t temp_id = node.offset;
        const FeatureSlot& slot = feat_offsets_[temp_id];
        const std::uint64_t feature_offset = sorted_feature_bytes.size();
        final_offset_by_temp_id[temp_id] = feature_offset;
        node.offset = feature_offset;

        if (slot.size > 0) {
            tmp_.seekg(static_cast<std::streamoff>(slot.offset));
            const std::size_t cur = sorted_feature_bytes.size();
            sorted_feature_bytes.resize(cur + slot.size);
            tmp_.read(reinterpret_cast<char*>(sorted_feature_bytes.data() + cur),
                      static_cast<std::streamsize>(slot.size));
            if (!tmp_) {
                throw Error(ErrorCode::IoError, "FcbWriter: failed reading a feature back from "
                                                "the temp file");
            }
        }
    }

    std::vector<std::uint8_t> rtree_bytes;
    if (build_rtree) {
        std::vector<NodeItem> tree = build_packed_rtree(feat_nodes_, extent, effective_node_size);
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

    std::vector<std::uint8_t> out;
    out.reserve(sizeof(kMagicBytes) + header_fbb.GetSize() + rtree_bytes.size() +
                attr_index_bytes.size() + sorted_feature_bytes.size());
    out.insert(out.end(), kMagicBytes, kMagicBytes + sizeof(kMagicBytes));
    out.insert(out.end(), header_fbb.GetBufferPointer(),
               header_fbb.GetBufferPointer() + header_fbb.GetSize());
    out.insert(out.end(), rtree_bytes.begin(), rtree_bytes.end());
    out.insert(out.end(), attr_index_bytes.begin(), attr_index_bytes.end());
    out.insert(out.end(), sorted_feature_bytes.begin(), sorted_feature_bytes.end());
    return out;
}

}  // namespace fcb
