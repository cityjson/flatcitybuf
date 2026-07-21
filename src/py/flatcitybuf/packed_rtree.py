from __future__ import annotations

import struct
from collections import deque
from dataclasses import dataclass

from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.layout import NODE_ITEM_SIZE
from flatcitybuf.range_reader import RangeReader

# packed_rtree.hpp / packed_rtree/mod.rs:23-33,56-77 -- 4 doubles then a
# u64, all little-endian, 40 bytes, no padding. Decoded directly with
# struct -- never through FlatBuffers, this section is raw hand-rolled
# bytes.
_NODE_FMT = "<4dQ"


@dataclass(frozen=True)
class NodeItem:
    """One R-tree node entry. Mirrors fcb::NodeItem
    (packed_rtree.hpp:29-46).

    `offset` means different things by level: for an INTERNAL node it is
    the child node INDEX; for a LEAF it is a byte offset relative to the
    start of the features section. search_rtree is the only code that
    knows which is which at any given moment (via the level it is
    currently visiting) -- NodeItem itself does not know.
    """

    min_x: float
    min_y: float
    max_x: float
    max_y: float
    offset: int

    @staticmethod
    def decode(buf: bytes, pos: int = 0) -> NodeItem:
        """Decode one 40-byte node entry at `pos` within `buf`.

        Uses "<4dQ" -- an unsigned "Q", never signed "q": `offset` is a
        u64, and Python ints are arbitrary precision, so decoding it
        signed would silently turn any value >= 2**63 negative and send
        traversal indexing backwards. This is gotcha 1 from the task
        brief.
        """
        if len(buf) < pos + NODE_ITEM_SIZE:
            raise FcbError(
                ErrorCode.INDEX_OUT_OF_BOUNDS,
                "short rtree node item",
            )
        min_x, min_y, max_x, max_y, offset = struct.unpack_from(
            _NODE_FMT, buf, pos
        )
        return NodeItem(min_x, min_y, max_x, max_y, offset)

    def intersects(self, bbox: tuple[float, float, float, float]) -> bool:
        """Mirrors NodeItem::intersects (packed_rtree.cpp:87-94, origin
        packed_rtree/mod.rs:122-134): strict `<`/`>` only, so touching
        edges DO intersect."""
        min_x, min_y, max_x, max_y = bbox
        if max_x < self.min_x:
            return False
        if max_y < self.min_y:
            return False
        if min_x > self.max_x:
            return False
        if min_y > self.max_y:
            return False
        return True


@dataclass(frozen=True)
class SearchResultItem:
    """One hit from an R-tree traversal. Mirrors fcb::SearchResultItem
    (reader.hpp:18-21)."""

    offset: int  # relative to the features section
    index: int  # feature ordinal (position within the leaf level)


def _level_bounds(num_items: int, node_size: int) -> list[tuple[int, int]]:
    """Mirrors generate_level_bounds (packed_rtree.cpp:38-64, origin
    packed_rtree/mod.rs:342-375).

    Returns one (start, end) half-open node-index range per level.
    Index 0 is the LEAF level and is LAST in on-disk storage order (it
    occupies the highest node indices); the last entry in the returned
    list is the root, always (0, 1). Getting this order backwards
    searches the wrong nodes entirely while still returning
    plausible-looking results -- Format Reference, "Packed R-tree".
    """
    if node_size < 2:
        raise FcbError(
            ErrorCode.ILLEGAL_HEADER_SIZE,
            f"invalid index_node_size: {node_size}",
        )
    if num_items == 0:
        raise FcbError(
            ErrorCode.ILLEGAL_HEADER_SIZE,
            "_level_bounds requires num_items > 0",
        )

    level_num_nodes = []
    n = num_items
    num_nodes = n
    level_num_nodes.append(n)
    while True:
        n = -(-n // node_size)  # ceil_div; Python ints never overflow.
        num_nodes += n
        level_num_nodes.append(n)
        if n == 1:
            break

    level_offsets = []
    acc = num_nodes
    for size in level_num_nodes:
        acc -= size
        level_offsets.append(acc)

    return [
        (level_offsets[i], level_offsets[i] + size)
        for i, size in enumerate(level_num_nodes)
    ]


def search_rtree(
    reader: RangeReader,
    rtree_begin: int,
    num_items: int,
    node_size: int,
    bbox: tuple[float, float, float, float],
) -> list[SearchResultItem]:
    """Breadth-first bbox search over the packed R-tree. Mirrors
    fcb::rtree_search_bbox (packed_rtree.cpp:113-190, origin
    packed_rtree/mod.rs).

    Results are sorted by feature offset, so a caller reads forward
    through the features section. `rtree_begin` is the absolute byte
    offset of the R-tree section (HeaderView.layout.rtree_begin);
    `num_items`/`node_size` are normally header.info.features_count /
    header.info.index_node_size.

    Every read is bounded by the RangeReader contract: `reader.read`
    either returns the requested byte range in full or raises
    FcbError(INDEX_OUT_OF_BOUNDS)/short-reads it, so a corrupt
    num_items/node_size cannot provoke an unbounded allocation here --
    node blocks are fetched `node_size` entries at a time (realistically
    capped at 65535 since index_node_size is a wire `ushort`, per
    layout.rtree_index_size's identical reasoning), and the level-bounds
    computation above never allocates anything proportional to
    num_items itself (only O(log_node_size(num_items)) levels).
    """
    if num_items == 0:
        return []

    level_bounds = _level_bounds(num_items, node_size)
    leaf_start, _leaf_end = level_bounds[0]

    results: list[SearchResultItem] = []
    queue: deque[tuple[int, int]] = deque()
    queue.append((0, len(level_bounds) - 1))

    while queue:
        node_index, level = queue.popleft()

        if level >= len(level_bounds):
            raise FcbError(
                ErrorCode.INDEX_OUT_OF_BOUNDS,
                "rtree level out of range",
            )
        level_start, level_end = level_bounds[level]
        # Child indices come from the file and are hostile. Prove the
        # node lies within the level we believe we are on BEFORE using
        # it, and derive leaf-ness from the trusted level rather than
        # from the index itself.
        if node_index < level_start or node_index >= level_end:
            raise FcbError(
                ErrorCode.INDEX_OUT_OF_BOUNDS,
                "rtree node index outside its level",
            )
        is_leaf = level == 0

        end = min(node_index + node_size, level_end)
        if end <= node_index:
            continue

        length = end - node_index
        byte_offset = rtree_begin + node_index * NODE_ITEM_SIZE
        byte_len = length * NODE_ITEM_SIZE

        block = reader.read(byte_offset, byte_len)
        if len(block) < byte_len:
            raise FcbError(
                ErrorCode.INDEX_OUT_OF_BOUNDS,
                "truncated rtree node block",
            )

        for pos in range(node_index, end):
            slot = pos - node_index
            item = NodeItem.decode(block, slot * NODE_ITEM_SIZE)
            if not item.intersects(bbox):
                continue

            if is_leaf:
                results.append(
                    SearchResultItem(
                        offset=item.offset, index=pos - leaf_start
                    )
                )
            else:
                child_level = level - 1
                child_start, child_end = level_bounds[child_level]
                if item.offset < child_start or item.offset >= child_end:
                    raise FcbError(
                        ErrorCode.INDEX_OUT_OF_BOUNDS,
                        "rtree child index outside the child level",
                    )
                # Being inside the child level is not enough: a valid
                # child offset is always the FIRST index of a node_size
                # group (every group the writer emits starts at
                # child_start + k*node_size). A hostile offset that
                # lands mid-group would still pass the range check
                # above, and `end = min(node_index + node_size,
                # level_end)` below would then read `node_size` items
                # starting mid-group -- silently reading the wrong
                # entries (dropping the true group's first item and
                # spilling into the next group's) rather than raising.
                # Codex review (Task 12): reproduced with num_items=2,
                # node_size=2, a root child offset of 2 instead of 1 --
                # in range but not group-aligned, and the leaf at
                # offset 1 (never visited) went missing from the result
                # with no error at all.
                if (item.offset - child_start) % node_size != 0:
                    raise FcbError(
                        ErrorCode.INDEX_OUT_OF_BOUNDS,
                        "rtree child index is not aligned to a node group",
                    )
                queue.append((item.offset, child_level))

    results.sort(key=lambda r: r.offset)
    return results
