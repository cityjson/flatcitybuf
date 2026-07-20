from __future__ import annotations

import struct
from pathlib import Path
from typing import Iterator

from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.feature import Feature
from flatcitybuf.generated.feature_generated import CityFeature as _CityFeature
from flatcitybuf.header import HeaderView, read_header
from flatcitybuf.layout import MAX_FEATURE_SIZE
from flatcitybuf.range_reader import (
    BufferedRangeReader,
    FileRangeReader,
    RangeReader,
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


class FcbReader:
    """The library's entry point. Mirrors fcb::FcbReader
    (reader.hpp:69-101), minus select_bbox/select_attr (Tasks 9-10)."""

    def __init__(self, reader: RangeReader, header: HeaderView) -> None:
        self._reader = reader
        self.header = header

    @classmethod
    def open_file(cls, path: str | Path) -> FcbReader:
        return cls.open(FileRangeReader(path))

    @classmethod
    def open(cls, reader: RangeReader) -> FcbReader:
        return cls(reader, read_header(reader))

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
