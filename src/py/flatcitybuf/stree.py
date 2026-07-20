from __future__ import annotations

from collections import deque
from dataclasses import dataclass
from enum import Enum
from typing import Sequence

from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.header import AttrIndexInfo, HeaderView
from flatcitybuf.keys import (
    KeyKind,
    KeyValue,
    column_type_to_key_kind,
    compare_keys,
    decode_key,
    key_max,
    key_min,
    key_serialized_size,
)
from flatcitybuf.packed_rtree import SearchResultItem
from flatcitybuf.range_reader import BufferedRangeReader, RangeReader

# stree.rs:15-17 -- the MSB of a leaf offset marks a PAYLOAD REFERENCE
# rather than a direct feature offset. Written as `1 << 63` because
# Python ints are arbitrary precision: a u64 decoded with "<q" instead
# of "<Q" goes negative at exactly this value and indexes backwards,
# which is gotcha 3 from the task brief.
PAYLOAD_TAG = 1 << 63
PAYLOAD_MASK = PAYLOAD_TAG - 1

# Entry<K> = key then a u64 LE offset (entry.rs:25-52).
_OFFSET_SIZE = 8

# http_reader/mod.rs:363 -- the attribute phase's combine threshold,
# reused as the per-query buffering window, matching
# FcbReader::select_attr (reader.cpp:331).
_INDEX_FETCH_SIZE = 1_048_576

# Column types the writer indexes as FixedStringKey<100> over a JSON or
# binary blob. See search_stree's docstring, divergence 2.
_JSON_COLUMN_TYPE = 12
_BINARY_COLUMN_TYPE = 14


class Operator(Enum):
    """Comparison operators the attribute index supports. Mirrors
    fcb::Operator (stree.hpp:17)."""

    EQ = "eq"
    NE = "ne"
    GT = "gt"
    GE = "ge"
    LT = "lt"
    LE = "le"


@dataclass(frozen=True)
class AttrCondition:
    """One condition of an attribute query. Mirrors fcb::AttrCondition
    (stree.hpp:20-24), whose `field` is spelled `column` here to match
    the interface this task's plan entry names.

    `column` is the column NAME, resolved against Header.columns.
    """

    column: str
    operator: Operator
    value: KeyValue


def is_payload_ref(offset: int) -> bool:
    """True if a leaf offset points at the payload section rather than
    directly at a feature. Mirrors fcb::is_payload_ref (stree.hpp:41)."""
    return offset & PAYLOAD_TAG != 0


def payload_offset(offset: int) -> int:
    """The low 63 bits of a tagged offset -- relative to the PAYLOAD
    SECTION start, not the file or the index blob (stree.rs:652-659).
    Mirrors fcb::payload_offset (stree.hpp:42)."""
    return offset & PAYLOAD_MASK


def stree_num_nodes(num_items: int, branching_factor: int) -> int:
    """Total node count across every level. Mirrors fcb::stree_num_nodes
    (stree.cpp:315-329, origin stree.rs:1480-1501).

    NOTE the loop breaks at `n < branching_factor`, NOT at `n == 1` as
    the packed R-tree does (packed_rtree.py:_level_bounds). The
    asymmetry is deliberate in the reference -- a level stops splitting
    as soon as it fits in one node's worth of separators -- and getting
    it wrong shifts every level's storage range while still producing
    plausible-looking results.
    """
    if branching_factor < 2:
        raise FcbError(
            ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND,
            f"invalid branching factor: {branching_factor}",
        )
    if num_items == 0:
        return 0

    n = num_items
    num_nodes = n
    while True:
        n = -(-n // branching_factor)  # ceil_div; Python ints never wrap
        num_nodes += n
        if n < branching_factor:
            break
    return num_nodes


def _level_bounds(
    num_items: int, branching_factor: int
) -> list[tuple[int, int]]:
    """One (start, end) half-open node-index range per level. Mirrors
    generate_level_bounds (stree.cpp:28-57, origin stree.rs:462-497).

    Index 0 is the LEAF level and is LAST in storage order; the final
    entry is the root.
    """
    if branching_factor < 2:
        raise FcbError(
            ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND,
            f"invalid branching factor: {branching_factor}",
        )
    if num_items == 0:
        raise FcbError(
            ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND, "empty attribute index"
        )

    level_num_nodes = [num_items]
    n = num_items
    num_nodes = n
    while True:
        n = -(-n // branching_factor)
        num_nodes += n
        level_num_nodes.append(n)
        if n < branching_factor:
            break

    bounds: list[tuple[int, int]] = []
    acc = num_nodes
    for size in level_num_nodes:
        acc -= size
        bounds.append((acc, acc + size))
    return bounds


def decode_payload_entry(b: bytes) -> list[int]:
    """Decode a payload entry: u32 count then count x u64, all
    little-endian (payload.rs:36-61). Mirrors
    fcb::decode_payload_entry (stree.cpp:331-347)."""
    if len(b) < 4:
        raise FcbError(
            ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND, "short payload entry"
        )
    count = int.from_bytes(b[0:4], "little")
    want = 4 + count * _OFFSET_SIZE
    if len(b) < want:
        raise FcbError(
            ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND, "truncated payload entry"
        )
    return [
        int.from_bytes(b[4 + i * 8 : 12 + i * 8], "little")
        for i in range(count)
    ]


@dataclass(frozen=True)
class _Entry:
    key: KeyValue
    offset: int


class _Tree:
    """Shared state for one query against one column's index blob.
    Mirrors fcb::Tree (stree.cpp:164-181)."""

    def __init__(
        self,
        reader: RangeReader,
        index_begin: int,
        payload_begin: int,
        payload_size: int,
        kind: KeyKind,
        node_size: int,
        levels: list[tuple[int, int]],
    ) -> None:
        self.reader = reader
        self.index_begin = index_begin
        self.payload_begin = payload_begin
        self.payload_size = payload_size
        self.kind = kind
        self.key_size = key_serialized_size(kind)
        self.entry_size = self.key_size + _OFFSET_SIZE
        # THE search node size: branching_factor - 1 entries, not
        # branching_factor (stree.rs:743, :826, :1087). Each entry is a
        # separator key, so a node of fan-out B holds B-1 of them.
        self.node_size = node_size
        self.levels = levels
        # The LEAF level is last in storage order (levels[0]), so its
        # end is the total node count; levels[-1] is the root, at 0..1.
        self.node_count = levels[0][1]

    @property
    def leaf_start(self) -> int:
        return self.levels[0][0]

    @property
    def leaf_end(self) -> int:
        return self.levels[0][1]

    def read_entries(self, first: int, last: int) -> list[_Entry]:
        """Entries [first, last) of the flat node array. Mirrors
        read_entries (stree.cpp:76-102)."""
        if last <= first:
            return []
        # Bound against the node region, not just against the file: a
        # corrupt child index must not make us read (and decode) the
        # payload section as if it were entries.
        if first < 0 or last > self.node_count:
            raise FcbError(
                ErrorCode.INDEX_OUT_OF_BOUNDS,
                f"attribute index node range {first}..{last} outside "
                f"the {self.node_count}-node region",
            )

        at = self.index_begin + first * self.entry_size
        length = (last - first) * self.entry_size
        block = self.reader.read(at, length)
        if len(block) < length:
            raise FcbError(
                ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND,
                "truncated attribute index node",
            )

        out: list[_Entry] = []
        for i in range(last - first):
            base = i * self.entry_size
            key = decode_key(self.kind, block[base : base + self.key_size])
            offset = int.from_bytes(
                block[base + self.key_size : base + self.entry_size],
                "little",
            )
            out.append(_Entry(key=key, offset=offset))
        return out

    def node_at(self, node_index: int, level: int) -> list[_Entry]:
        end = min(node_index + self.node_size, self.levels[level][1])
        return self.read_entries(node_index, end)

    def emit(
        self, offset: int, index: int, out: list[SearchResultItem]
    ) -> None:
        """Resolve one leaf offset into feature offsets, following a
        payload reference when the MSB is set. Mirrors emit_offset
        (stree.cpp:128-161)."""
        if not is_payload_ref(offset):
            out.append(SearchResultItem(offset=offset, index=index))
            return

        rel = payload_offset(offset)
        if rel + 4 > self.payload_size:
            raise FcbError(
                ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND,
                "payload reference out of range",
            )
        head = self.reader.read(self.payload_begin + rel, 4)
        if len(head) < 4:
            raise FcbError(
                ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND,
                "truncated payload entry",
            )
        count = int.from_bytes(head, "little")
        want = 4 + count * _OFFSET_SIZE
        # Bound the allocation BEFORE making it: a crafted count of
        # 0xFFFFFFFF would otherwise ask for ~32 GiB.
        if rel + want > self.payload_size:
            raise FcbError(
                ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND,
                "payload entry overruns its section",
            )
        body = self.reader.read(self.payload_begin + rel, want)
        if len(body) < want:
            raise FcbError(
                ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND,
                "truncated payload entry body",
            )
        for feature_offset in decode_payload_entry(body):
            out.append(SearchResultItem(offset=feature_offset, index=index))


def _binary_search(items: list[_Entry], key: KeyValue) -> tuple[bool, int]:
    """Rust's binary_search_by result: `found` plus the index of the
    match, or of the insertion point. Mirrors binary_search
    (stree.cpp:111-124).

    Hand-rolled rather than `bisect`: the ordering is compare_keys, not
    `<`, and bisect's key= form (3.10+) cannot express a three-way
    comparator anyway. See keys._cmp_ordered_float.
    """
    lo, hi = 0, len(items)
    while lo < hi:
        mid = lo + (hi - lo) // 2
        c = compare_keys(items[mid].key, key)
        if c == 0:
            return True, mid
        if c < 0:
            lo = mid + 1
        else:
            hi = mid
    return False, lo


def _find_exact(tree: _Tree, key: KeyValue) -> list[SearchResultItem]:
    """Mirrors fcb::find_exact (stree.cpp:184-234, origin
    stree.rs:733-816)."""
    out: list[SearchResultItem] = []
    queue: deque[tuple[int, int]] = deque()
    queue.append((0, len(tree.levels) - 1))

    while queue:
        node_index, level = queue.popleft()
        items = tree.node_at(node_index, level)
        if not items:
            continue

        found, at = _binary_search(items, key)

        if level != 0:
            # Internal descent. On an exact hit the search key belongs
            # to the RIGHT of that separator, hence the + node_size;
            # _find_partition deliberately omits it.
            if found:
                child = items[at].offset + tree.node_size
            elif at == 0:
                child = items[0].offset
            elif at >= len(items):
                child = items[-1].offset + tree.node_size
            else:
                child = items[at].offset

            # Separator entries with no right sibling carry
            # K::max_value() as a sentinel, whose offset ALREADY points
            # at the last child group. Adding node_size would walk off
            # the end of the level for any query whose key equals the
            # type maximum -- Eq(True) on a bool column is enough.
            # Clamping back to `offset` is a no-op for ordinary keys.
            # The same fix has been applied upstream (stree.cpp:212-222).
            child_level = level - 1
            child_start, child_end = tree.levels[child_level]
            if child >= child_end:
                child = items[at if at < len(items) else len(items) - 1].offset
            if child < child_start or child >= child_end:
                raise FcbError(
                    ErrorCode.INDEX_OUT_OF_BOUNDS,
                    "attribute index child outside the child level",
                )
            queue.append((child, child_level))
            continue

        if found:
            tree.emit(items[at].offset, node_index + at - tree.leaf_start, out)
    return out


def _find_partition(tree: _Tree, key: KeyValue) -> int:
    """The leftmost leaf index where `key` could sit. Mirrors
    fcb::find_partition (stree.cpp:240-258, origin stree.rs:1086-1128).

    Identical descent to _find_exact EXCEPT that an exact hit descends
    to `offset` with no + node_size -- that difference is what makes
    this land at the leftmost position rather than skipping past equal
    keys.
    """
    node_index = 0
    for level in range(len(tree.levels) - 1, 0, -1):
        items = tree.node_at(node_index, level)
        if not items:
            continue
        found, at = _binary_search(items, key)
        if found:
            node_index = items[at].offset
        elif at == 0:
            node_index = items[0].offset
        elif at >= len(items):
            node_index = items[-1].offset + tree.node_size
        else:
            node_index = items[at].offset
    return node_index


def _scan_range(
    tree: _Tree,
    lower: KeyValue,
    lower_strict: bool,
    upper: KeyValue,
    upper_strict: bool,
) -> list[SearchResultItem]:
    """Leaf scan with independently strict-or-inclusive bounds. Mirrors
    fcb::scan_range (stree.cpp:270-311).

    This REPLACES the reference's "range minus exact" lowering for
    Gt/Lt/Ne (query/stream.rs:161-191), which is wrong under existential
    semantics: the subtraction removes FEATURE OFFSETS, but one feature
    can appear under several keys when its CityObjects carry different
    values of the indexed attribute. A feature holding both k and
    k' > k is returned by the range scan (via k') and also by
    find_exact(k) (via k), so subtracting deletes a genuine match.
    Filtering at the leaf by bound strictness cannot make that mistake,
    and costs one traversal instead of two.

    There are NO leaf sibling pointers in the format -- the doc comment
    at entry.rs:15 claiming otherwise is stale. The scan therefore walks
    the contiguous leaf array by INDEX (stree.rs:626-679).
    """
    lu = compare_keys(lower, upper)
    if lu > 0:
        return []
    if lu == 0 and (lower_strict or upper_strict):
        return []

    lower_idx = _find_partition(tree, lower)
    upper_idx = _find_partition(tree, upper)

    start = max(lower_idx, tree.leaf_start)

    # Widened by one extra node versus the reference's
    # `upper_idx + node_size`. _find_partition descends LEFT on an exact
    # hit, so when `upper` is itself a separator key its matching leaf
    # entry sits at exactly upper_idx + node_size -- one past the
    # un-widened scan end, and was silently dropped. Widening is safe
    # because the filter below rejects out-of-range keys; it costs at
    # most one extra node read. Applied upstream too (stree.cpp:282-291).
    end = min(upper_idx + 2 * tree.node_size, tree.leaf_end)

    out: list[SearchResultItem] = []
    cur = start
    while cur < end:
        node_end = min(cur + tree.node_size, end)
        items = tree.read_entries(cur, node_end)
        for i, entry in enumerate(items):
            cl = compare_keys(entry.key, lower)
            cu = compare_keys(entry.key, upper)
            if (cl > 0) if lower_strict else (cl >= 0):
                if (cu < 0) if upper_strict else (cu <= 0):
                    tree.emit(entry.offset, cur + i - tree.leaf_start, out)
        cur = node_end
    return out


def _build_tree(
    reader: RangeReader, index: AttrIndexInfo, kind: KeyKind
) -> _Tree:
    """Validate one index blob's declared shape and locate its payload
    section. Mirrors the preamble of fcb::stree_query
    (stree.cpp:349-369), plus bounds checks C++ gets from its
    RangeReader wrapper.

    Every count here comes off the wire and is hostile. The checks below
    are what keeps a corrupt num_unique_items / branching_factor /
    length from provoking an unbounded read or allocation.
    """
    if index.length <= 0:
        raise FcbError(
            ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND,
            f"attribute index for column {index.column_index} is empty",
        )
    total_size = reader.total_size()
    if index.begin < 0 or index.begin + index.length > total_size:
        raise FcbError(
            ErrorCode.INDEX_OUT_OF_BOUNDS,
            f"attribute index for column {index.column_index} lies "
            f"outside the file ({index.begin}+{index.length} > "
            f"{total_size})",
        )

    num_nodes = stree_num_nodes(index.num_unique_items, index.branching_factor)
    entry_size = key_serialized_size(kind) + _OFFSET_SIZE
    tree_bytes = num_nodes * entry_size
    if tree_bytes > index.length:
        raise FcbError(
            ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND,
            "attribute index node region exceeds its declared length "
            f"({tree_bytes} > {index.length})",
        )

    return _Tree(
        reader=reader,
        index_begin=index.begin,
        payload_begin=index.begin + tree_bytes,
        payload_size=index.length - tree_bytes,
        kind=kind,
        node_size=index.branching_factor - 1,
        levels=_level_bounds(index.num_unique_items, index.branching_factor),
    )


def stree_query(
    reader: RangeReader,
    index: AttrIndexInfo,
    kind: KeyKind,
    operator: Operator,
    value: KeyValue,
) -> list[SearchResultItem]:
    """Run ONE condition against ONE column's index blob, returning
    candidate feature offsets (relative to the features section).
    Mirrors fcb::stree_query (stree.cpp:349-410).

    Operator lowering (query/stream.rs:161-191, with the Gt/Lt/Ne
    correction described in _scan_range):

    ==========  ==================================================
    Operator    Lowering
    ==========  ==================================================
    Eq          find_exact(value)
    Ge          scan_range[value, key_max]
    Le          scan_range[key_min, value]
    Gt          scan_range(value, key_max]   (inclusive for strings)
    Lt          scan_range[key_min, value)   (inclusive for strings)
    Ne          scan_range[key_min, value) + scan_range(value, key_max]
    ==========  ==================================================

    Fixed-width string keys are truncated, so ordering AFTER the
    truncation point is invisible to the index: two values sharing a
    50-byte prefix compare equal here but may order either way in full.
    Every string comparison is therefore WIDENED to include the
    equal-prefix band, and a caller's post-filter must apply the real
    operator to the untruncated value. Ne in particular must be a FULL
    scan -- excluding the prefix matches would drop features whose value
    merely shares a prefix.
    """
    tree = _build_tree(reader, index, kind)
    is_string = kind in (
        KeyKind.STRING20,
        KeyKind.STRING50,
        KeyKind.STRING100,
    )

    if operator is Operator.EQ:
        # Equal-prefix collisions are candidates, not answers.
        return _find_exact(tree, value)
    if operator is Operator.GE:
        return _scan_range(tree, value, False, key_max(kind), False)
    if operator is Operator.LE:
        return _scan_range(tree, key_min(kind), False, value, False)
    if operator is Operator.GT:
        # Strict, except for strings where equal-prefix keys must
        # survive to be judged on their full value.
        return _scan_range(tree, value, not is_string, key_max(kind), False)
    if operator is Operator.LT:
        return _scan_range(tree, key_min(kind), False, value, not is_string)
    if operator is Operator.NE:
        if is_string:
            return _scan_range(
                tree, key_min(kind), False, key_max(kind), False
            )
        # Two half-open scans rather than a full scan minus the equal
        # set: subtraction on feature offsets is wrong when one feature
        # carries several values of the attribute.
        lo = _scan_range(tree, key_min(kind), False, value, True)
        hi = _scan_range(tree, value, True, key_max(kind), False)
        return lo + hi
    raise FcbError(  # pragma: no cover - Operator is a closed enum
        ErrorCode.UNSUPPORTED_COLUMN_TYPE, f"unknown operator: {operator}"
    )


def _resolve(
    header: HeaderView, condition: AttrCondition
) -> tuple[AttrIndexInfo, KeyKind]:
    column = None
    for c in header.info.columns:
        if c.name == condition.column:
            column = c
            break
    if column is None:
        raise FcbError(
            ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND,
            f"no such column: {condition.column}",
        )

    # DIVERGENCE 2, checked BEFORE the "is it indexed" lookup so the
    # rejection does not depend on whether this particular writer
    # emitted an index for the column.
    if column.type in (_JSON_COLUMN_TYPE, _BINARY_COLUMN_TYPE):
        raise FcbError(
            ErrorCode.UNSUPPORTED_COLUMN_TYPE,
            f"column {column.name} is Json/Binary: its index is a "
            "fixed-width key over a blob, so hits are meaningless "
            "without post-verification",
        )

    index = None
    for a in header.attr_indices:
        if a.column_index == column.index:
            index = a
            break
    if index is None:
        raise FcbError(
            ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND,
            f"column is not indexed: {condition.column}",
        )

    kind = column_type_to_key_kind(column.type)
    if condition.value.kind is not kind:
        # Caught at the boundary rather than from deep inside the
        # traversal, where compare_keys would raise the same code with
        # no indication of which condition was at fault.
        raise FcbError(
            ErrorCode.UNSUPPORTED_COLUMN_TYPE,
            f"condition on column {column.name} carries a "
            f"{condition.value.kind.value} key but the column indexes "
            f"{kind.value}",
        )
    return index, kind


def search_stree(
    reader: RangeReader,
    info: HeaderView,
    conditions: Sequence[AttrCondition],
) -> list[SearchResultItem]:
    """Run an attribute query over the static B+tree indices, returning
    candidate features as feature-section-relative offsets, sorted and
    de-duplicated. Mirrors the index half of FcbReader::select_attr
    (reader.cpp:326-390).

    Multiple conditions are AND-intersected on feature offset, in order,
    with early exit once the accumulator is empty (stream.rs:402-423).

    Results are CANDIDATES, not answers, for fixed-width string columns
    (String, and the rejected Json/Binary): the on-disk key keeps only
    the first 50 (or 100) BYTES of the value, so distinct values sharing
    a prefix are indistinguishable here. A caller wanting exact answers
    must re-check each hit against the decoded, untruncated attribute --
    resolving the schema PER CityObject, since CityObject.columns
    overrides Header.columns.

    FOUR DELIBERATE DIVERGENCES from Rust's reader are reproduced here,
    so that the Rust, C++ and Python readers agree. Each is a decision,
    not an oversight; do not "fix" one without reading the "Known
    divergences from the Rust reader" section of
    docs/superpowers/plans/2026-07-19-native-cpp-core.md.

    1. Byte columns decode as u8, not i8. The writer stores Byte as u8
       (writer/attribute.rs:209) and indexes it as MemoryIndex<u8>
       (writer/attr_index.rs:240); only Rust's READER decodes i8
       (reader/attr_query.rs:118), returning negative numbers for
       stored values above 127. Matching the writer decodes files
       correctly, at the cost of disagreeing with Rust's reader there.
       See keys.column_type_to_key_kind.
    2. Json and Binary columns are REJECTED with
       ErrorCode.UNSUPPORTED_COLUMN_TYPE (reader/attr_query.rs:273),
       even though the writer may index them. Their keys are the first
       100 bytes of a JSON or binary blob, so index hits are near
       meaningless without post-verification, and rejecting is honest.
    3. The float maximum sentinel is +inf (key.rs:139), but NaN sorts
       ABOVE +inf in the ordered_float total order -- so NaN-keyed
       features are INVISIBLE to every range-lowered operator (Ge, Gt,
       Le, Lt, Ne). See keys.key_max.
    4. The DateTime minimum sentinel is epoch 0 (key.rs:242), not the
       true i64 minimum, even though the wire format stores a signed
       i64. Pre-1970 timestamps are therefore INVISIBLE to Le, Lt and
       Ne. See keys.key_min.

    Raises FcbError(ATTRIBUTE_INDEX_NOT_FOUND) for an empty query, an
    unknown column, an unindexed column or a structurally impossible
    index blob; FcbError(UNSUPPORTED_COLUMN_TYPE) for a Json/Binary
    column or a condition whose key kind does not match its column's.
    """
    if not conditions:
        raise FcbError(
            ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND, "empty attribute query"
        )

    # Per-query buffering, as C++ does (reader.cpp:331): one 1 MiB
    # window over the index section rather than a read per node.
    buffered = BufferedRangeReader(reader, _INDEX_FETCH_SIZE)

    accumulator: list[SearchResultItem] | None = None
    for condition in conditions:
        index, kind = _resolve(info, condition)
        hits = stree_query(
            buffered, index, kind, condition.operator, condition.value
        )

        # De-duplicate on offset: a feature reached through several keys
        # (its CityObjects may carry different values) must appear once.
        by_offset: dict[int, SearchResultItem] = {}
        for hit in hits:
            by_offset.setdefault(hit.offset, hit)

        if accumulator is None:
            accumulator = [by_offset[o] for o in sorted(by_offset)]
        else:
            accumulator = [h for h in accumulator if h.offset in by_offset]
        if not accumulator:
            break

    return accumulator or []


__all__ = [
    "PAYLOAD_TAG",
    "PAYLOAD_MASK",
    "AttrCondition",
    "Operator",
    "SearchResultItem",
    "decode_payload_entry",
    "is_payload_ref",
    "payload_offset",
    "search_stree",
    "stree_num_nodes",
    "stree_query",
]
