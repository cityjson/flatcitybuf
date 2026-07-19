#pragma once

#include <cstdint>

#include <fcb/error.hpp>
#include <fcb/span.hpp>

namespace fcb {

constexpr std::size_t kMagicBytesSize = 8;
constexpr std::size_t kHeaderSizeSize = 4;
constexpr std::size_t kHeaderMinBufferSize = 8;
constexpr std::size_t kHeaderMaxBufferSize = 1024ULL * 1024ULL * 512ULL;  // 512 MB
constexpr std::uint8_t kVersion = 1;
constexpr std::size_t kNodeItemSize = 40;
constexpr std::uint16_t kDefaultNodeSize = 16;

/// Hard ceiling on a single feature's byte length, enforced before
/// allocating. A crafted 4-byte prefix would otherwise request up to 4 GiB.
constexpr std::uint64_t kMaxFeatureSize = 256ULL * 1024ULL * 1024ULL;

/// Mirrors fcb_core::check_magic_bytes (src/rust/fcb_core/src/lib.rs:56-58).
/// Compares only bytes [0,3) and [4,7); byte 7 is never validated.
bool check_magic_bytes(bytes_view b);

/// Mirrors PackedRTree::index_size (packed_rtree/mod.rs:879-898). Returns
/// bytes. Throws on node_size < 2, num_items == 0, or arithmetic overflow.
std::uint64_t rtree_index_size(std::uint64_t num_items, std::uint16_t node_size);

/// Byte offsets of each section. Nothing in the file records these -- they
/// must be computed, and an off-by-one silently corrupts everything after.
struct FileLayout {
    std::uint64_t header_len;
    std::uint64_t rtree_begin;
    std::uint64_t rtree_size;
    std::uint64_t attr_index_begin;
    std::uint64_t attr_index_size;
    std::uint64_t feature_begin;
};

/// Throws fcb::Error{IllegalHeaderSize} when header_size is out of range or
/// any size arithmetic overflows.
FileLayout compute_layout(std::uint32_t header_size,
                          std::uint64_t features_count,
                          std::uint16_t index_node_size,
                          std::uint64_t attr_index_size);

/// Throws unless the computed sections fit inside the resource. Call this
/// immediately after compute_layout, before issuing any index read.
void validate_layout_against_size(const FileLayout& l, std::uint64_t total_size);

}  // namespace fcb
