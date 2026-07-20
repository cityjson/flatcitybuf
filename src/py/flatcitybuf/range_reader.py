from __future__ import annotations

import os
from pathlib import Path
from typing import Protocol

from flatcitybuf.errors import ErrorCode, FcbError


class RangeReader(Protocol):
    """Synchronous byte-range source. Implement this to plug in any
    transport (file, HTTP, memory). Mirrors fcb::RangeReader
    (range_reader.hpp:50-70), minus read_batch: batching a decorator
    around a Protocol has no interface to name here, so it is left for
    whichever later task actually needs it (YAGNI).

    CONTRACT -- implementors must honour all of it:

    * read(offset, length) returns EXACTLY `length` bytes unless the
      range crosses the end of the resource, in which case it returns
      exactly the bytes that exist (possibly zero).
    * length == 0 returns b"" WITHOUT contacting the transport.
    * Errors are reported by raising FcbError. Returning garbage is not
      an option -- callers cannot distinguish it from data.
    * Instances are NOT thread-safe.
    """

    def read(self, offset: int, length: int) -> bytes:
        """Read `length` bytes at `offset`, subject to the contract
        above."""
        ...

    def total_size(self) -> int:
        """Total byte length of the resource."""
        ...


class FileRangeReader(RangeReader):
    """Local-file adapter. Mirrors fcb::FileRangeReader
    (range_reader.cpp:17-43).

    Divergence from the C++ reference: range_reader.cpp:30 returns
    empty for `offset >= total_size()`, treating it as a normal, non-
    error condition. This Python reader instead raises
    FcbError(INDEX_OUT_OF_BOUNDS) in that case. See the module's task
    report for why: the brief's own test_read_past_end_raises requires
    it, and unlike the C++ core -- which validates every offset against
    total_size() before it ever reaches this reader (range_reader.hpp:
    56-59) -- nothing upstream of this from-scratch Python reader
    performs that validation yet, so the lowest layer raises instead of
    silently handing back an empty read that could mask a bug.
    """

    def __init__(self, path: str | Path) -> None:
        self._path = Path(path)
        try:
            self._file = open(self._path, "rb")
        except OSError as exc:
            raise FcbError(
                ErrorCode.IO_ERROR,
                f"cannot open file: {self._path}",
            ) from exc
        try:
            self._size = os.fstat(self._file.fileno()).st_size
        except OSError as exc:
            self._file.close()
            raise FcbError(
                ErrorCode.IO_ERROR,
                f"cannot stat file: {self._path}",
            ) from exc

    def total_size(self) -> int:
        return self._size

    def read(self, offset: int, length: int) -> bytes:
        if length == 0:
            return b""
        if offset >= self._size:
            raise FcbError(
                ErrorCode.INDEX_OUT_OF_BOUNDS,
                f"read offset {offset} is past end of file "
                f"(size={self._size}): {self._path}",
            )
        # Clamp to EOF: a range crossing the end returns exactly what
        # exists -- range_reader.cpp:32-33.
        n = min(length, self._size - offset)
        try:
            self._file.seek(offset)
            data = self._file.read(n)
        except OSError as exc:
            raise FcbError(
                ErrorCode.IO_ERROR,
                f"short read from {self._path}",
            ) from exc
        if len(data) != n:
            raise FcbError(
                ErrorCode.IO_ERROR,
                f"short read from {self._path}",
            )
        return data


class BufferedRangeReader(RangeReader):
    """Caching decorator: over-fetches to `fetch_size` and serves
    subsequent reads inside the cached window without touching the
    inner reader. Mirrors fcb::BufferedRangeReader (range_reader.cpp:
    47-95), minus read_batch (see RangeReader's docstring).

    OWNERSHIP: this is a PER-QUERY object, exactly as in C++
    (range_reader.hpp:90-95) -- construct one per query phase with the
    fetch_size appropriate to it, and discard it when done.

    Unlike the C++ constructor, `fetch_size` defaults to 1 MiB
    (1_048_576) rather than being a required argument, per the task
    brief's exact signature.
    """

    def __init__(
        self, inner: RangeReader, fetch_size: int = 1_048_576
    ) -> None:
        self._inner = inner
        self._fetch_size = fetch_size
        self._buf_offset = 0
        self._buf = b""

    def total_size(self) -> int:
        return self._inner.total_size()

    def _covers(self, offset: int, length: int) -> bool:
        # range_reader.cpp:64-72. Python ints don't wrap on overflow
        # (unlike the C++ uint64_t this guards there), so the
        # checked-arithmetic half of that function has no Python
        # equivalent to port -- see the task report.
        if not self._buf or offset < self._buf_offset:
            return False
        return offset + length <= self._buf_offset + len(self._buf)

    def _clamped_fetch(self, offset: int, length: int) -> int:
        # range_reader.cpp:53-62. Over-fetch to fetch_size, but never
        # past the end of the resource.
        want = max(length, self._fetch_size)
        total = self._inner.total_size()
        if offset >= total:
            return length
        return min(want, total - offset)

    def read(self, offset: int, length: int) -> bytes:
        if length == 0:
            return b""  # contract: never contact the transport

        if not self._covers(offset, length):
            self._buf = self._inner.read(
                offset, self._clamped_fetch(offset, length)
            )
            self._buf_offset = offset

        rel = offset - self._buf_offset
        if rel >= len(self._buf):
            return b""
        n = min(length, len(self._buf) - rel)
        return self._buf[rel : rel + n]
