from __future__ import annotations

from dataclasses import dataclass

from flatcitybuf.errors import ErrorCode, FcbError

# const_vars.rs:5 -- MAGIC_BYTES = {'f','c','b',0x01,'f','c','b',0x00}
MAGIC_SIZE = 8

# const_vars.rs:2 -- VERSION = 1, stored at magic byte index 3.
_VERSION = 1

# header.fbs:65-70 / packed_rtree/mod.rs:23-33,56-77 -- NodeItem is
# { f64 min_x, min_y, max_x, max_y; u64 offset }, 40 bytes, no padding.
NODE_ITEM_SIZE = 40

# packed_rtree/mod.rs:325 -- default index_node_size.
DEFAULT_NODE_SIZE = 16

# layout.hpp kMaxFeatureSize -- hard ceiling enforced before allocating a
# feature buffer, so a crafted 4-byte prefix cannot request a huge alloc.
MAX_FEATURE_SIZE = 256 * 1024 * 1024

# const_vars.rs:8, reader/mod.rs:97-102 -- header_size guard.
_HEADER_MIN_BUFFER_SIZE = 8
_HEADER_MAX_BUFFER_SIZE = 1024 * 1024 * 512  # 512 MB

# lib.rs:56-58, http_reader/mod.rs:136 -- constant byte-count of the
# FlatBuffers size-prefix field preceding the Header buffer.
_HEADER_SIZE_FIELD_SIZE = 4


def check_magic_bytes(b: bytes) -> bool:
    """Mirrors fcb_core::check_magic_bytes (lib.rs:56-58).

    Compares only bytes [0,3) and [4,7); byte 7 is never validated.
    Byte 3 (the version) is a forward-compat rejection, not an equality
    check: a future version byte fails.
    """
    if len(b) < MAGIC_SIZE:
        return False
    if b[0:3] != b"fcb":
        return False
    if b[4:7] != b"fcb":
        return False
    return b[3] <= _VERSION


def rtree_index_size(num_items: int, node_size: int) -> int:
    """Mirrors PackedRTree::index_size (packed_rtree/mod.rs:879-898).

    Returns the byte size of the packed R-tree index. Raises FcbError on
    node_size < 2 (Rust asserts node_size >= 2; a smaller value means the
    file is corrupt, so we reject rather than clamp) or num_items == 0
    (the accumulation loop would never terminate).
    """
    if node_size < 2:
        raise FcbError(
            ErrorCode.ILLEGAL_HEADER_SIZE,
            f"invalid index_node_size: {node_size}",
        )
    if num_items == 0:
        raise FcbError(
            ErrorCode.ILLEGAL_HEADER_SIZE,
            "rtree_index_size requires num_items > 0",
        )
    n = num_items
    num_nodes = n
    while True:
        n = -(-n // node_size)  # ceil_div
        num_nodes += n
        if n == 1:
            break
    return num_nodes * NODE_ITEM_SIZE


@dataclass
class FileLayout:
    """Byte offsets of each section. Nothing in the file records these --
    they must be computed, and an off-by-one silently corrupts everything
    after."""

    header_len: int
    rtree_begin: int
    rtree_size: int
    attr_index_begin: int
    attr_index_size: int
    feature_begin: int


def compute_layout(
    header_size: int,
    features_count: int,
    index_node_size: int,
    attr_index_size: int,
) -> FileLayout:
    """Mirrors fcb::compute_layout (layout.cpp).

    Raises FcbError{ILLEGAL_HEADER_SIZE} when header_size is out of range.
    """
    if not (
        _HEADER_MIN_BUFFER_SIZE
        <= header_size
        <= _HEADER_MAX_BUFFER_SIZE
    ):
        raise FcbError(
            ErrorCode.ILLEGAL_HEADER_SIZE,
            f"illegal header size: {header_size}",
        )

    header_len = MAGIC_SIZE + _HEADER_SIZE_FIELD_SIZE + header_size
    rtree_begin = header_len
    # index_node_size == 0 means "no spatial index" and is legal; any
    # other value below 2 is corrupt and rtree_index_size rejects it.
    if index_node_size == 0 or features_count == 0:
        rtree_size = 0
    else:
        rtree_size = rtree_index_size(features_count, index_node_size)
    attr_index_begin = rtree_begin + rtree_size
    feature_begin = attr_index_begin + attr_index_size
    return FileLayout(
        header_len=header_len,
        rtree_begin=rtree_begin,
        rtree_size=rtree_size,
        attr_index_begin=attr_index_begin,
        attr_index_size=attr_index_size,
        feature_begin=feature_begin,
    )


def validate_layout_against_size(
    layout: FileLayout, total_size: int
) -> None:
    """Throws unless the computed sections fit inside the resource. Call
    this immediately after compute_layout, before issuing any index
    read."""
    if layout.feature_begin > total_size:
        raise FcbError(
            ErrorCode.ILLEGAL_HEADER_SIZE,
            "sections extend past end of file: "
            f"feature_begin={layout.feature_begin} "
            f"total_size={total_size}",
        )
