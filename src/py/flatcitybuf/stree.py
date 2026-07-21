from __future__ import annotations

from collections import deque
from dataclasses import dataclass
from enum import Enum
from typing import Sequence

from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.generated.header_generated import ColumnType
from flatcitybuf.header import AttrIndexInfo, HeaderView
from flatcitybuf.keys import (
    KeyKind,
    KeyValue,
    column_type_to_key_kind,
    compare_keys,
    decode_key,
    is_string_kind,
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

# Codex review (Task 12): _Tree.emit's existing "bound the allocation
# BEFORE making it" check bounds a payload entry's declared count only
# against `payload_size` -- itself bounded only by the attribute
# index's declared `length`, which is in turn bounded only by the
# RangeReader's reported total_size(). A SPARSE file can report a huge
# total_size while occupying almost no disk space, so a single crafted
# 4-byte count (up to u32::MAX) can still force an allocation of nearly
# `total_size` bytes -- up to ~4 GiB -- from a file that is tiny on
# disk. This ceiling is deliberately tighter than what the format alone
# allows, mirroring the "bound before allocating" philosophy of
# layout.MAX_FEATURE_SIZE (which itself mirrors C++'s kMaxFeatureSize).
# There is no reference constant to mirror here: neither the Rust nor
# the C++ reader caps this at all (both share the same exposure).
_MAX_PAYLOAD_ENTRY_SIZE = 256 * 1024 * 1024

# http_reader/mod.rs:363 -- the attribute phase's combine threshold,
# reused as the per-query buffering window, matching
# FcbReader::select_attr (reader.cpp:331).
_INDEX_FETCH_SIZE = 1_048_576

# Column types the writer indexes as FixedStringKey<100> over a JSON or
# binary blob. See search_stree's docstring, divergence 2. Taken from the
# GENERATED enum rather than re-spelled as literals, so this file and
# keys._COLUMN_TYPE_TO_KIND cannot drift apart if header.fbs changes.
_BLOB_COLUMN_TYPES = (ColumnType.Json, ColumnType.Binary)


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
        (stree.cpp:128-161).

        `index` is the LEAF-RELATIVE ordinal of the entry that produced
        this hit -- see SearchResultItem's meaning in search_stree's
        docstring. It is passed through unchanged to every feature
        behind a payload entry, so all features sharing one key share
        one index.
        """
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
        # 0xFFFFFFFF would otherwise ask for ~32 GiB. A sane ceiling
        # independent of the file's own (possibly sparse-inflated)
        # declared size comes first -- see _MAX_PAYLOAD_ENTRY_SIZE.
        if want > _MAX_PAYLOAD_ENTRY_SIZE:
            raise FcbError(
                ErrorCode.ATTRIBUTE_INDEX_NOT_FOUND,
                f"payload entry claims {count} offsets, exceeding the "
                "sanity ceiling",
            )
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


def _validated_child(
    tree: _Tree, child: int, at: int, items: list[_Entry], child_level: int
) -> int:
    """Turn a computed child pointer into a validated child node index
    within `child_level`, raising on any corruption. Shared by
    _find_exact and _find_partition, whose descent step computes
    `child`/`node_index` identically and must reject the same hostile
    shapes:

    1. A sentinel-induced overflow past the child level's end.
       Separator entries with no right sibling carry K::max_value() as
       a sentinel, whose offset ALREADY points at the last child
       group; adding node_size would walk off the end of the level for
       any query whose key equals the type maximum -- Eq(True) on a
       bool column is enough. Clamping back to `offset` is a no-op for
       ordinary keys. The same fix has been applied upstream
       (stree.cpp:212-222).
    2. Any offset outside [child_start, child_end) -- a corrupt/hostile
       file must not be followed off the end of the tree.
    3. (Codex review, Task 12) An offset that IS inside the child level
       but is not the FIRST index of a node_size group. Every group a
       real writer emits starts at child_start + k*node_size; a
       hostile offset landing mid-group would pass check 2 above and
       then get read as if it were a group's start -- silently
       returning the WRONG entries (skipping the true group's first
       item, spilling into the next group's) rather than raising.
       Reproduced with UINT64/branching_factor=3/num_unique_items=2: a
       root offset of 2 instead of the only valid child index, 1 --
       `find_exact(0)` returned no match at all for a key that is
       genuinely present, with no error.
    """
    child_start, child_end = tree.levels[child_level]
    if child >= child_end:
        child = items[at if at < len(items) else len(items) - 1].offset
    if child < child_start or child >= child_end:
        raise FcbError(
            ErrorCode.INDEX_OUT_OF_BOUNDS,
            "attribute index child outside the child level",
        )
    if (child - child_start) % tree.node_size != 0:
        raise FcbError(
            ErrorCode.INDEX_OUT_OF_BOUNDS,
            "attribute index child is not aligned to a node group",
        )
    return child


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

            child_level = level - 1
            child = _validated_child(tree, child, at, items, child_level)
            queue.append((child, child_level))
            continue

        if found:
            # `- leaf_start` makes the index LEAF-RELATIVE: node_index is
            # an index into the flat node array, whose leaf level starts
            # at levels[0][0], not at 0.
            tree.emit(items[at].offset, node_index + at - tree.leaf_start, out)
    return out


def _find_partition(tree: _Tree, key: KeyValue) -> int:
    """The leftmost leaf index where `key` could sit. Mirrors
    fcb::find_partition (stree.cpp:240-258, origin stree.rs:1086-1128).

    Identical descent to _find_exact EXCEPT that an exact hit descends
    to `offset` with no + node_size -- that difference is what makes
    this land at the leftmost position rather than skipping past equal
    keys. Shares _validated_child's corruption checks with _find_exact
    (Codex review, Task 12): this function used to apply NEITHER the
    sentinel clamp nor the child-level bounds check at all, relying
    solely on read_entries' much weaker whole-array bound one level
    down -- a corrupt offset that stayed under the total node count but
    did not belong to `child_level` would silently drive every
    Ge/Le/Gt/Lt/Ne query's leaf-scan window to the wrong place instead
    of raising.
    """
    node_index = 0
    for level in range(len(tree.levels) - 1, 0, -1):
        items = tree.node_at(node_index, level)
        if not items:
            continue
        found, at = _binary_search(items, key)
        if found:
            child = items[at].offset
        elif at == 0:
            child = items[0].offset
        elif at >= len(items):
            child = items[-1].offset + tree.node_size
        else:
            child = items[at].offset
        node_index = _validated_child(tree, child, at, items, level - 1)
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
                    # Leaf-relative, exactly as _find_exact emits it:
                    # `cur` indexes the flat node array, whose leaf level
                    # begins at leaf_start.
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
    # keys.is_string_kind, not a re-spelled tuple: one predicate, so the
    # widening below cannot disagree with what needs_post_filter
    # considers a candidate-only column.
    is_string = is_string_kind(kind)

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
    if column.type in _BLOB_COLUMN_TYPES:
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
    overrides Header.columns. That is FcbReader.select_attr's job
    (reader.py); this function is deliberately the RAW candidate layer,
    the equivalent of C++'s AttrQueryOptions.exact_index_only
    (reader.cpp:388).

    SearchResultItem.index is the LEAF-RELATIVE ORDINAL of the B+tree
    entry that produced the hit -- i.e. the rank of its (unique) KEY in
    sorted key order, counting from the first leaf entry. This is NOT
    the same meaning the packed R-tree gives the field, where it
    identifies the feature itself: several features can hide behind one
    payload entry, and they all carry that entry's single index. Use
    `offset` to identify a feature; `index` only says which key it was
    found under.

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
        # C++ raises ErrorCode::QueryExecutionError here
        # (reader.cpp:327-329); errors.py has no such member, so this
        # reuses ATTRIBUTE_INDEX_NOT_FOUND rather than inventing one.
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


# --------------------------------------------------- post-filtering ---
#
# Everything below supports FcbReader.select_attr (reader.py), the public
# query entry point. It lives here, next to Operator and the lowering it
# has to undo, rather than in reader.py: reader.py is the top of the
# layering and imports this module, never the other way round.


def condition_key_kind(info: HeaderView, condition: AttrCondition) -> KeyKind:
    """The key kind `condition`'s column is indexed as, resolved exactly
    the way search_stree resolves it -- same column lookup, same
    Json/Binary rejection, same kind-mismatch check. Mirrors the
    `key_kind_for_column(col->type)` step of FcbReader::select_attr
    (reader.cpp:359)."""
    _index, kind = _resolve(info, condition)
    return kind


def needs_post_filter(kind: KeyKind) -> bool:
    """True if stree_query's answers for `kind` are candidates rather
    than answers, so select_attr must re-check them against the
    untruncated value. Mirrors fcb::needs_post_filter
    (reader.cpp:319-322)."""
    return is_string_kind(kind)


_INT_FACTORIES = {
    KeyKind.INT8: KeyValue.from_i8,
    KeyKind.UINT8: KeyValue.from_u8,
    KeyKind.INT16: KeyValue.from_i16,
    KeyKind.UINT16: KeyValue.from_u16,
    KeyKind.INT32: KeyValue.from_i32,
    KeyKind.UINT32: KeyValue.from_u32,
    KeyKind.INT64: KeyValue.from_i64,
    KeyKind.UINT64: KeyValue.from_u64,
}


def _key_from_attr_value(value: object, kind: KeyKind) -> KeyValue | None:
    """Lift one decoded attribute value (attribute.decode_attributes'
    output: bool / int / float / str) into a KeyValue of `kind`, or None
    if it cannot be one. Mirrors the coercion switch in C++'s
    value_satisfies (reader.cpp:259-305).

    None means "this value cannot satisfy a condition of that kind", not
    an error: a post-filter that raised on a type mismatch would turn a
    heterogeneous file into an exception instead of an empty result.
    DATETIME is the one exception -- see below.
    """
    if is_string_kind(kind):
        # DateTime attributes also arrive as text, but their KEY kind is
        # DATETIME, so they never reach this branch.
        if not isinstance(value, str):
            return None
        return KeyValue.from_string(kind, value)
    if kind is KeyKind.DATETIME:
        # A DateTime ATTRIBUTE is length-prefixed UTF-8 text in the blob
        # (attribute.py's _STRING_LIKE_TYPES) while a DateTime KEY is 12
        # packed bytes of (i64 seconds, u32 nanos) -- lifting one to the
        # other needs an RFC-3339 parser with exactly the writer's
        # semantics, which this reader does not have and cannot verify
        # against any committed fixture.
        #
        # Falling through to `return None` would make value_satisfies
        # answer a confident, silent False for EVERY DateTime condition,
        # which is a wrong answer dressed as an empty result. Nothing in
        # this package can reach here today (needs_post_filter is true
        # only for string kinds, so select_attr never post-filters a
        # DateTime column), but value_satisfies is public and exported,
        # so a direct caller gets a diagnosable error instead.
        raise FcbError(
            ErrorCode.UNSUPPORTED_COLUMN_TYPE,
            "post-filtering a DateTime column is not supported: a "
            "DateTime attribute is stored as text and cannot be "
            "compared against a packed DateTime key",
        )
    if kind is KeyKind.BOOL:
        return KeyValue.from_bool(value) if isinstance(value, bool) else None
    # bool IS an int in Python: exclude it explicitly before the numeric
    # branches, or True would compare equal to a 1 in a ULong column.
    if isinstance(value, bool):
        return None
    if kind is KeyKind.FLOAT32 and isinstance(value, (int, float)):
        return KeyValue.from_f32(float(value))
    if kind is KeyKind.FLOAT64 and isinstance(value, (int, float)):
        return KeyValue.from_f64(float(value))
    factory = _INT_FACTORIES.get(kind)
    if factory is None or not isinstance(value, int):
        return None
    try:
        return factory(value)
    except FcbError:
        # Out of range for the column's width -- a value no key of this
        # kind could hold cannot equal, or order against, one that can.
        return None


def _compare_result_satisfies(operator: Operator, c: int) -> bool:
    if operator is Operator.EQ:
        return c == 0
    if operator is Operator.NE:
        return c != 0
    if operator is Operator.GT:
        return c > 0
    if operator is Operator.GE:
        return c >= 0
    if operator is Operator.LT:
        return c < 0
    if operator is Operator.LE:
        return c <= 0
    raise FcbError(  # pragma: no cover - Operator is a closed enum
        ErrorCode.UNSUPPORTED_COLUMN_TYPE, f"unknown operator: {operator}"
    )


def value_satisfies(value: object, operator: Operator, want: KeyValue) -> bool:
    """True if a decoded attribute `value` really satisfies `operator`
    against `want`. Mirrors fcb::value_satisfies (reader.cpp:259-317).

    STRINGS are compared as the FULL, untruncated UTF-8 BYTES -- never as
    `str` (whose ordering is by code point, not by the byte order the
    index and every other implementation use) and never through
    compare_keys, which deliberately compares the TRUNCATED, zero-padded
    key forms. Undoing that truncation is the entire point of the
    post-filter.

    Every other kind goes through compare_keys, so float columns keep the
    ordered_float total order (NaN == NaN, NaN above +inf) rather than
    Python's `<`.

    Raises FcbError{UNSUPPORTED_COLUMN_TYPE} for a DATETIME `want`,
    rather than answering a silent False it cannot justify -- see
    _key_from_attr_value. A value of the wrong TYPE for the kind is
    still a plain False.
    """
    actual = _key_from_attr_value(value, want.kind)
    if actual is None:
        return False
    if is_string_kind(want.kind):
        a, b = actual.raw, want.raw
        c = -1 if a < b else (1 if a > b else 0)
    else:
        c = compare_keys(actual, want)
    return _compare_result_satisfies(operator, c)


__all__ = [
    "PAYLOAD_TAG",
    "PAYLOAD_MASK",
    "AttrCondition",
    "Operator",
    "SearchResultItem",
    "condition_key_kind",
    "decode_payload_entry",
    "is_payload_ref",
    "needs_post_filter",
    "payload_offset",
    "search_stree",
    "stree_num_nodes",
    "stree_query",
    "value_satisfies",
]
