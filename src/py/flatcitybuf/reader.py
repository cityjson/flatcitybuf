from __future__ import annotations

import struct
from pathlib import Path
from typing import Iterator, Sequence

from flatcitybuf.attribute import decode_attributes
from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.feature import Feature
from flatcitybuf.generated.feature_generated import CityFeature as _CityFeature
from flatcitybuf.header import HeaderView, read_header
from flatcitybuf.layout import MAX_FEATURE_SIZE
from flatcitybuf.packed_rtree import SearchResultItem
from flatcitybuf.range_reader import (
    BufferedRangeReader,
    FileRangeReader,
    RangeReader,
)
from flatcitybuf.stree import (
    AttrCondition,
    condition_key_kind,
    needs_post_filter,
    search_stree,
    value_satisfies,
)

# reader/mod.rs:539-545, :569-572 -- the size prefix in front of every
# feature's FlatBuffer body: a bare 4-byte little-endian u32, excluding
# itself (written by finish_size_prefixed, writer/feature_writer.rs:83).
_LENGTH_PREFIX_SIZE = 4

# http_reader/mod.rs:42 -- DEFAULT_HTTP_FETCH_SIZE, reused here as the
# per-query buffering window for a sequential scan, matching
# FcbReader::select_all's own BufferedRangeReader (reader.cpp:439-443).
_FEATURE_FETCH_SIZE = 1_048_576


def _parse_feature(raw_buf: bytes, byte_offset: int) -> Feature:
    try:
        cf = _CityFeature.GetRootAs(raw_buf, 4)  # type: ignore[no-untyped-call]
        return Feature(cf, byte_offset)
    except FcbError:
        raise
    except Exception as exc:
        # No FlatBuffers Verifier in this Python runtime (Task 3's
        # finding) -- a malformed feature body surfaces as whatever
        # exception the flatbuffers runtime or our own decoding raises
        # (IndexError, struct.error, ...). Nothing may escape the
        # public surface un-wrapped.
        raise FcbError(
            ErrorCode.INVALID_FLATBUFFER,
            f"failed to parse feature FlatBuffer at offset {byte_offset}",
        ) from exc


def _read_feature_at(
    reader: RangeReader, feature_begin: int, offset: int
) -> Feature:
    """One feature by RANDOM ACCESS, at a feature-section-relative
    `offset`. Mirrors the IterationMode::OffsetList branch of
    FeatureIterator::next (reader.cpp:182-221).

    Deliberately NOT shared with select_all's loop: that loop's
    short-read handling encodes truncation semantics that only make
    sense while walking forward with (or without) a known feature count
    -- here a short read is always corruption, because an index told us
    a feature starts at this offset.
    """
    total_size = reader.total_size()
    at = feature_begin + offset
    if at < feature_begin or at + _LENGTH_PREFIX_SIZE > total_size:
        raise FcbError(
            ErrorCode.INDEX_OUT_OF_BOUNDS,
            f"feature offset {offset} lies outside the feature section",
        )
    prefix = reader.read(at, _LENGTH_PREFIX_SIZE)
    if len(prefix) < _LENGTH_PREFIX_SIZE:
        raise FcbError(ErrorCode.IO_ERROR, "truncated feature size prefix")
    (length,) = struct.unpack_from("<I", prefix, 0)
    # Bound the allocation BEFORE making it, as select_all does.
    if length == 0 or length > MAX_FEATURE_SIZE:
        raise FcbError(
            ErrorCode.INVALID_FLATBUFFER,
            f"implausible feature size: {length}",
        )
    want = _LENGTH_PREFIX_SIZE + length
    if at + want > total_size:
        raise FcbError(
            ErrorCode.IO_ERROR,
            "feature body extends past end of resource",
        )
    raw_buf = reader.read(at, want)
    if len(raw_buf) < want:
        raise FcbError(ErrorCode.IO_ERROR, "truncated feature body")
    return _parse_feature(raw_buf, offset)


class FcbReader:
    """The library's entry point. Mirrors fcb::FcbReader
    (reader.hpp:69-101), minus select_bbox."""

    def __init__(self, reader: RangeReader, header: HeaderView) -> None:
        self._reader = reader
        self.header = header

    @classmethod
    def open_file(cls, path: str | Path) -> FcbReader:
        return cls.open(FileRangeReader(path))

    @classmethod
    def open(cls, reader: RangeReader) -> FcbReader:
        return cls(reader, read_header(reader))

    def select_attr(
        self,
        conditions: Sequence[AttrCondition],
        exact_index_only: bool = False,
    ) -> list[SearchResultItem]:
        """Run an attribute query and return the features that REALLY
        match, as feature-section-relative offsets sorted ascending.
        Mirrors FcbReader::select_attr (reader.cpp:326-397), returning
        stree.search_stree's result shape rather than C++'s
        FeatureIterator (offset-list iteration is not part of this
        reader's surface yet).

        The difference from stree.search_stree is POST-FILTERING. For
        fixed-width string columns the tree can only answer with
        candidates -- keys are truncated at 50 (or 100) bytes, possibly
        mid-codepoint -- so stree_query deliberately WIDENS those
        operators (Gt/Lt include the equal-prefix band; Ne is a full
        scan). This method undoes the widening by re-checking each
        candidate against the decoded, untruncated attribute, which is
        what makes the Python reader agree with Rust and C++ on string
        columns.

        Verification is EXISTENTIAL over a feature's CityObjects, each
        decoded with its OWN schema (CityObjectView.columns when set,
        Header.columns otherwise): a feature matches if any one of its
        CityObjects carries a value satisfying the condition. A
        CityObject that lacks the attribute entirely never matches --
        so, as in C++, `Ne` does NOT return features that simply do not
        have the attribute.

        `exact_index_only` mirrors AttrQueryOptions::exact_index_only
        (reader.cpp:388): it returns the raw candidates, skipping
        verification. Verification can only REMOVE candidates, never add
        them.

        SearchResultItem.index is passed through from the index layer --
        see search_stree's docstring for what it means (C++ zeroes it
        here; keeping it loses no information and costs nothing).
        """
        candidates = search_stree(self._reader, self.header, conditions)

        # Which conditions the index answered only approximately. The
        # column resolution has already been validated by search_stree,
        # so this cannot raise for a query that got this far.
        inexact = [
            c
            for c in conditions
            if needs_post_filter(condition_key_kind(self.header, c))
        ]
        if exact_index_only or not inexact or not candidates:
            return candidates

        buffered = BufferedRangeReader(self._reader, _FEATURE_FETCH_SIZE)
        feature_begin = self.header.layout.feature_begin
        verified: list[SearchResultItem] = []
        for hit in candidates:
            feature = _read_feature_at(buffered, feature_begin, hit.offset)
            if self._feature_satisfies(feature, inexact):
                verified.append(hit)
        return verified

    def _feature_satisfies(
        self, feature: Feature, conditions: Sequence[AttrCondition]
    ) -> bool:
        """reader.cpp:399-427 -- the body of select_attr's verification
        loop, one feature at a time."""
        for condition in conditions:
            matched = False
            for obj in feature.city_objects():
                if not obj.attributes:
                    continue
                schema = (
                    obj.columns
                    if obj.has_columns and obj.columns is not None
                    else self.header.info.columns
                )
                decoded = decode_attributes(obj.attributes, schema)
                if condition.column not in decoded:
                    continue
                if value_satisfies(
                    decoded[condition.column],
                    condition.operator,
                    condition.value,
                ):
                    matched = True
                    break
            if not matched:
                return False
        return True

    def select_all(self) -> Iterator[Feature]:
        """Iterate every feature in stored (Hilbert) order. Mirrors
        FcbReader::select_all (reader.cpp:439-446) plus the
        IterationMode::SequentialScan branch of FeatureIterator::next
        (reader.cpp:124-221); the OffsetList branch backing
        select_bbox/select_attr is Tasks 9-10.

        features_count == 0 means UNKNOWN (header.fbs:136 comment), not
        "no features": with a known count, iteration stops after
        exactly that many features and any bytes left over are a
        truncated-index error (reader.cpp:130-148); with an unknown
        count, iteration runs to end of resource and a short read there
        is the normal terminus, not an error (reader.cpp:166-180).
        """
        buffered = BufferedRangeReader(self._reader, _FEATURE_FETCH_SIZE)
        total_size = buffered.total_size()
        features_count = self.header.info.features_count
        known = features_count > 0
        feature_begin = self.header.layout.feature_begin
        cursor = feature_begin
        produced = 0

        while True:
            if known and produced >= features_count:
                if cursor < total_size:
                    raise FcbError(
                        ErrorCode.IO_ERROR,
                        f"trailing bytes after {features_count} features",
                    )
                return
            if not known and cursor >= total_size:
                return
            if cursor >= total_size:
                raise FcbError(
                    ErrorCode.IO_ERROR,
                    "feature offset past end of resource",
                )

            prefix = buffered.read(cursor, _LENGTH_PREFIX_SIZE)
            if len(prefix) < _LENGTH_PREFIX_SIZE:
                # Reaching EOF before features_count features is a
                # TRUNCATED file, not a clean end of iteration -- unless
                # the count was unknown all along, in which case a
                # short read at the end is exactly what "run to EOF"
                # looks like.
                if not known:
                    return
                raise FcbError(
                    ErrorCode.IO_ERROR,
                    "truncated feature section: expected "
                    f"{features_count} features, got {produced}",
                )

            (length,) = struct.unpack_from("<I", prefix, 0)
            # Bound the allocation BEFORE making it: a crafted
            # 0xFFFFFFFF prefix would otherwise ask for ~4 GiB.
            if length == 0 or length > MAX_FEATURE_SIZE:
                raise FcbError(
                    ErrorCode.INVALID_FLATBUFFER,
                    f"implausible feature size: {length}",
                )
            want = _LENGTH_PREFIX_SIZE + length
            if cursor + want > total_size:
                raise FcbError(
                    ErrorCode.IO_ERROR,
                    "feature body extends past end of resource",
                )
            raw_buf = buffered.read(cursor, want)
            if len(raw_buf) < want:
                raise FcbError(ErrorCode.IO_ERROR, "truncated feature body")

            yield _parse_feature(raw_buf, cursor - feature_begin)

            cursor += want
            produced += 1
