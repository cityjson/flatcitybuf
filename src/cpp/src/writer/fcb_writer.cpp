#include <fcb/writer/btree_builder.hpp>
#include <fcb/writer/fcb_writer.hpp>
#include <fcb/writer/feature_serializer.hpp>
#include <fcb/writer/rtree_builder.hpp>

#include <algorithm>
#include <limits>

namespace fcb {

namespace {

// "fcb" + VERSION + "fcb" + 0 (const_vars.rs:5, layout.hpp's kVersion == 1).
constexpr std::uint8_t kMagicBytes[8] = {'f', 'c', 'b', kVersion, 'f', 'c', 'b', 0};

}  // namespace

std::vector<std::uint8_t> write_fcb(const nlohmann::ordered_json& cj,
                                    const std::vector<nlohmann::ordered_json>& features,
                                    const FcbWriterOptions& options,
                                    const AttributeSchema& attr_schema,
                                    const AttributeSchema* semantic_attr_schema) {
    const auto& transform = cj.at("transform");
    const double scale_x = transform.at("scale").at(0).get<double>();
    const double scale_y = transform.at("scale").at(1).get<double>();
    const double translate_x = transform.at("translate").at(0).get<double>();
    const double translate_y = transform.at("translate").at(1).get<double>();

    // Only the REQUESTED columns' names are passed to
    // `cityfeature_to_index_entries` -- mirrors `attr_indices: Option<Vec
    // <String>>` in feature_writer.rs, derived from
    // `header_options.attribute_indices`, not from the whole schema.
    std::vector<std::string> indexing_attr;
    for (const auto& [name, unused] : options.attribute_indices)
        indexing_attr.push_back(name);

    // Per-feature accumulation pass (mirrors FcbWriter::write_feature,
    // called once per `add_feature`, writer/mod.rs:100-131): each
    // feature's encoded bytes, its transform-scaled bbox (`actual_bbox`,
    // writer/mod.rs:133-144) tagged with its ORIGINAL (pre-sort) index,
    // and its attribute-index entries (tagged the same way, since the
    // final sorted byte offset isn't known until the next pass).
    std::vector<std::vector<std::uint8_t>> feature_bytes(features.size());
    std::vector<NodeItem> feat_nodes;
    feat_nodes.reserve(features.size());
    std::vector<std::vector<AttributeIndexEntry>> index_entries_by_feature(features.size());

    for (std::size_t i = 0; i < features.size(); ++i) {
        flatbuffers::FlatBufferBuilder fbb;
        auto [off, raw_bbox] = to_fcb_city_feature(fbb, features[i].at("id").get<std::string>(),
                                                   features[i], attr_schema, semantic_attr_schema);
        fbb.FinishSizePrefixed(off);
        feature_bytes[i].assign(fbb.GetBufferPointer(), fbb.GetBufferPointer() + fbb.GetSize());

        feat_nodes.push_back(NodeItem{
            raw_bbox.min_x * scale_x + translate_x, raw_bbox.min_y * scale_y + translate_y,
            raw_bbox.max_x * scale_x + translate_x, raw_bbox.max_y * scale_y + translate_y, i});

        if (!indexing_attr.empty())
            index_entries_by_feature[i] =
                cityfeature_to_index_entries(features[i], attr_schema, indexing_attr);
    }

    // `write_index: false` forces index_node_size to 0 in the header,
    // exactly like `HeaderWriter::new_with_options` (header_writer.rs:
    // 87-94) -- computed once here so every downstream decision (whether
    // to hilbert_sort, whether to build an R-tree, what the header
    // records) uses the SAME effective value.
    const std::uint16_t effective_node_size = options.write_index ? options.index_node_size : 0;

    // Sort-and-reassign pass (writer/mod.rs:204-247): if there's no R-tree
    // to build, `feat_nodes` is left in ORIGINAL order (each `.offset`
    // still its temp id from the loop above) -- Rust's own `if
    // index_node_size > 0 && !feat_nodes.is_empty()` guards `hilbert_sort`
    // itself, not just the R-tree build, so skipping it here too (rather
    // than always sorting) is required for byte-exactness when no spatial
    // index is requested.
    NodeItem extent = NodeItem::empty(0);
    const bool build_rtree = effective_node_size > 0 && !feat_nodes.empty();
    if (build_rtree) {
        extent = calc_extent(feat_nodes);
        hilbert_sort(feat_nodes, extent);
    }

    // Concatenate every feature's bytes in `feat_nodes`' CURRENT order
    // (sorted or original, per above) into the final features section,
    // recording each feature's FINAL byte offset both on its `NodeItem`
    // (consumed by the R-tree build right after) and, indexed by ORIGINAL
    // id, for attribute-index entries below -- the one pass where both
    // trees' offsets come from the same "final feature position"
    // computation, so it must run before either is built.
    std::vector<std::uint8_t> sorted_feature_bytes;
    std::vector<std::uint64_t> final_offset_by_temp_id(features.size());
    for (auto& node : feat_nodes) {
        const std::uint64_t temp_id = node.offset;
        const std::uint64_t feature_offset = sorted_feature_bytes.size();
        final_offset_by_temp_id[temp_id] = feature_offset;
        node.offset = feature_offset;
        const auto& bytes = feature_bytes[temp_id];
        sorted_feature_bytes.insert(sorted_feature_bytes.end(), bytes.begin(), bytes.end());
    }

    std::vector<std::uint8_t> rtree_bytes;
    if (build_rtree) {
        std::vector<NodeItem> tree = build_packed_rtree(feat_nodes, extent, effective_node_size);
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
        options.attribute_indices;
    std::stable_sort(
        sorted_indices.begin(), sorted_indices.end(), [&attr_schema](const auto& a, const auto& b) {
            const auto ia = attr_schema.find(a.first);
            const auto ib = attr_schema.find(b.first);
            const std::uint16_t idx_a = ia != attr_schema.end()
                                            ? ia->second.first
                                            : std::numeric_limits<std::uint16_t>::max();
            const std::uint16_t idx_b = ib != attr_schema.end()
                                            ? ib->second.first
                                            : std::numeric_limits<std::uint16_t>::max();
            return idx_a < idx_b;
        });

    std::vector<std::uint8_t> attr_index_bytes;
    std::vector<AttributeIndexInfo> attr_index_info;
    if (!sorted_indices.empty()) {
        std::vector<std::vector<BtreeEntry>> entries_by_column(attr_schema.size());
        for (std::size_t temp_id = 0; temp_id < features.size(); ++temp_id) {
            const std::uint64_t feature_offset = final_offset_by_temp_id[temp_id];
            for (const auto& e : index_entries_by_feature[temp_id])
                entries_by_column.at(e.index).push_back(BtreeEntry{e.value, feature_offset});
        }

        for (const auto& [name, bf_opt] : sorted_indices) {
            const auto it = attr_schema.find(name);
            if (it == attr_schema.end())
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
    header_options.feature_count = features.size();
    header_options.index_node_size = effective_node_size;
    header_options.geographical_extent = options.geographical_extent;

    flatbuffers::FlatBufferBuilder header_fbb;
    auto header_off =
        to_fcb_header(header_fbb, cj, header_options, attr_schema, semantic_attr_schema,
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
