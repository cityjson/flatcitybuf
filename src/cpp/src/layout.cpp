#include <fcb/layout.hpp>

#include "detail/checked.hpp"

#include <cstring>
#include <string>

namespace fcb {

using detail::ceil_div;
using detail::checked_add;
using detail::checked_mul;

bool check_magic_bytes(bytes_view b) {
    if (b.size() < kMagicBytesSize) return false;
    static const std::uint8_t kFcb[3] = {'f', 'c', 'b'};
    if (std::memcmp(b.data() + 0, kFcb, 3) != 0) return false;
    if (std::memcmp(b.data() + 4, kFcb, 3) != 0) return false;
    // Forward-compat rejection, not equality: a future version byte fails.
    return b[3] <= kVersion;
}

std::uint64_t rtree_index_size(std::uint64_t num_items, std::uint16_t node_size) {
    // Rust asserts node_size >= 2 (packed_rtree/mod.rs:879). 0 or 1 means the
    // file is corrupt: reject rather than clamp, so we never invent a layout.
    if (node_size < 2) {
        throw Error(ErrorCode::IllegalHeaderSize,
                    "invalid index_node_size: " + std::to_string(node_size));
    }
    if (num_items == 0) {
        // The loop below would never terminate.
        throw Error(ErrorCode::IllegalHeaderSize,
                    "rtree_index_size requires num_items > 0");
    }
    const std::uint64_t ns = node_size;
    std::uint64_t n = num_items;
    std::uint64_t num_nodes = n;
    for (;;) {
        n = ceil_div(n, ns);
        num_nodes = checked_add(num_nodes, n, "rtree num_nodes");
        if (n == 1) break;
    }
    return checked_mul(num_nodes, kNodeItemSize, "rtree index size");
}

FileLayout compute_layout(std::uint32_t header_size,
                          std::uint64_t features_count,
                          std::uint16_t index_node_size,
                          std::uint64_t attr_index_size) {
    if (header_size < kHeaderMinBufferSize || header_size > kHeaderMaxBufferSize) {
        throw Error(ErrorCode::IllegalHeaderSize,
                    "illegal header size: " + std::to_string(header_size));
    }

    FileLayout l{};
    l.header_len = kMagicBytesSize + kHeaderSizeSize + header_size;
    l.rtree_begin = l.header_len;
    // index_node_size == 0 means "no spatial index" and is legal; any other
    // value below 2 is corrupt and rtree_index_size rejects it.
    l.rtree_size = (index_node_size == 0 || features_count == 0)
                       ? 0
                       : rtree_index_size(features_count, index_node_size);
    l.attr_index_begin = checked_add(l.rtree_begin, l.rtree_size, "attr_index_begin");
    l.attr_index_size = attr_index_size;
    l.feature_begin =
        checked_add(l.attr_index_begin, l.attr_index_size, "feature_begin");
    return l;
}

void validate_layout_against_size(const FileLayout& l, std::uint64_t total_size) {
    if (l.feature_begin > total_size) {
        throw Error(ErrorCode::IllegalHeaderSize,
                    "sections extend past end of file: feature_begin=" +
                        std::to_string(l.feature_begin) +
                        " total_size=" + std::to_string(total_size));
    }
}

}  // namespace fcb
