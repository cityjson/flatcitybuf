from __future__ import annotations

from pathlib import Path

import pytest
from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.range_reader import BufferedRangeReader
from flatcitybuf.range_reader import FileRangeReader


class CountingReader:
    """In-memory RangeReader that records every request, so tests can
    assert on IO behaviour (buffering, request counts) deterministically.
    Mirrors fcb::testing::FakeRangeReader (fake_range_reader.hpp)."""

    def __init__(self, data: bytes) -> None:
        self.data = data
        self.reads: list[tuple[int, int]] = []

    def read(self, offset: int, length: int) -> bytes:
        self.reads.append((offset, length))
        return self.data[offset : offset + length]

    def total_size(self) -> int:
        return len(self.data)


# ---------------------------------------------------------------- brief ---


def test_buffered_reader_serves_sequential_reads_from_one_fetch() -> None:
    inner = CountingReader(bytes(range(256)) * 8)
    r = BufferedRangeReader(inner, fetch_size=512)
    assert r.read(0, 4) == bytes(range(4))
    assert r.read(4, 4) == bytes(range(4, 8))
    assert len(inner.reads) == 1, "second read must be served from the buffer"


def test_read_past_end_raises(tmp_path: Path) -> None:
    # `some_path` in the brief is undefined; using a real temp file here
    # (pytest's tmp_path) rather than conformance/small.fcb, since that
    # fixture is gitignored/generated and this test needs no FCB-specific
    # content -- any file with a known size will do.
    some_path = tmp_path / "data.bin"
    some_path.write_bytes(b"0123456789")
    r = FileRangeReader(some_path)
    with pytest.raises(FcbError):
        r.read(r.total_size() + 1, 4)


# ------------------------------------------------------- FileRangeReader ---


def test_file_range_reader_reads_exact_ranges_and_reports_total_size(
    tmp_path: Path,
) -> None:
    # Port of test_range_reader.cpp:20-47, minus the "past EOF is empty"
    # case: this Python reader raises there instead (see
    # test_read_past_end_raises_with_error_code below and the report's
    # "differs from C++" section).
    data = bytes(i & 0xFF for i in range(1000))
    path = tmp_path / "test_frr.bin"
    path.write_bytes(data)

    r = FileRangeReader(path)
    assert r.total_size() == 1000

    chunk = r.read(100, 10)
    assert chunk == data[100:110]

    # A range crossing EOF (but starting inside the file) returns exactly
    # the bytes that exist -- range_reader.cpp:32-33.
    tail = r.read(995, 50)
    assert tail == data[995:1000]
    assert len(tail) == 5

    # Zero length never contacts the transport -- range_reader.cpp:29,
    # even when offset is itself past the end.
    assert r.read(0, 0) == b""
    assert r.read(2000, 0) == b""


def test_file_range_reader_raises_on_missing_file(tmp_path: Path) -> None:
    # Port of test_range_reader.cpp:49-51.
    with pytest.raises(FcbError) as exc_info:
        FileRangeReader(tmp_path / "definitely_not_a_file.bin")
    assert exc_info.value.code is ErrorCode.IO_ERROR


def test_read_past_end_raises_with_error_code(tmp_path: Path) -> None:
    # Pins the specific error code for the brief's test_read_past_end_raises,
    # and the deliberate divergence from range_reader.cpp:30 (which returns
    # empty for offset >= total_size(), not an error). See the report.
    path = tmp_path / "data.bin"
    path.write_bytes(b"0123456789")
    r = FileRangeReader(path)
    with pytest.raises(FcbError) as exc_info:
        r.read(r.total_size(), 1)
    assert exc_info.value.code is ErrorCode.INDEX_OUT_OF_BOUNDS


def test_file_range_reader_read_ending_exactly_at_total_size(
    tmp_path: Path,
) -> None:
    # Edge case: offset + length == total_size() is a fully valid read,
    # not a clamp and not an error -- offset (990) is still < total_size.
    data = bytes(i & 0xFF for i in range(1000))
    path = tmp_path / "test_frr_end.bin"
    path.write_bytes(data)
    r = FileRangeReader(path)

    got = r.read(990, 10)
    assert got == data[990:1000]
    assert len(got) == 10


# ----------------------------------------------------- BufferedRangeReader


def test_buffered_reader_over_fetches_and_serves_hits_from_cache() -> None:
    # Port of test_range_reader.cpp:65-86.
    inner = CountingReader(bytes(i & 0xFF for i in range(10000)))
    buf = BufferedRangeReader(inner, fetch_size=1024)

    a = buf.read(0, 8)
    assert len(a) == 8
    assert len(inner.reads) == 1
    assert inner.reads[0] == (0, 1024)  # over-fetched

    # Inside the cached window -> no new upstream request.
    b = buf.read(500, 20)
    assert len(b) == 20
    assert b[0] == 500 & 0xFF
    assert len(inner.reads) == 1

    # Outside the window -> exactly one more request.
    c = buf.read(5000, 4)
    assert len(c) == 4
    assert c[0] == 5000 & 0xFF
    assert len(inner.reads) == 2


def test_buffered_reader_honours_reads_larger_than_fetch_size() -> None:
    # Port of test_range_reader.cpp:88-96.
    inner = CountingReader(bytes(i & 0xFF for i in range(10000)))
    buf = BufferedRangeReader(inner, fetch_size=64)

    big = buf.read(100, 2000)
    assert len(big) == 2000
    assert len(inner.reads) == 1
    assert inner.reads[0] == (100, 2000)


def test_buffered_reader_zero_length_read_never_contacts_transport() -> None:
    # Port of test_range_reader.cpp:98-103.
    inner = CountingReader(bytes(range(256)) * 4)
    buf = BufferedRangeReader(inner, fetch_size=1024)
    assert buf.read(0, 0) == b""
    assert inner.reads == []


def test_buffered_reader_total_size_forwards_to_inner() -> None:
    inner = CountingReader(bytes(range(100)))
    buf = BufferedRangeReader(inner, fetch_size=16)
    assert buf.total_size() == 100


def test_buffered_reader_clamps_fetch_to_remaining_bytes_near_end() -> None:
    # clamped_fetch must not over-fetch past the resource's end, or a
    # transport with real byte-range semantics (HTTP) would request bytes
    # that do not exist -- range_reader.cpp:53-62.
    inner = CountingReader(bytes(i & 0xFF for i in range(100)))
    buf = BufferedRangeReader(inner, fetch_size=1024)

    got = buf.read(90, 5)
    assert got == bytes(i & 0xFF for i in range(90, 95))
    assert inner.reads == [(90, 10)]  # clamped to total_size() - offset


def test_buffered_reader_straddling_read_triggers_new_fetch() -> None:
    # Edge case: a read whose range straddles the end of the currently
    # buffered window (partly inside, partly outside) is a full cache
    # miss -- covers() (range_reader.cpp:64-72) requires the WHOLE range
    # to fit, so the new fetch starts at the request's own offset, not at
    # the old buffer's end; nothing is stitched together.
    inner = CountingReader(bytes(range(256)) * 8)  # 2048 bytes
    r = BufferedRangeReader(inner, fetch_size=512)

    r.read(0, 4)  # primes buffer to [0, 512)
    assert inner.reads == [(0, 512)]

    straddling = r.read(510, 10)  # [510, 520) -- 510 < 512 < 520
    assert straddling == inner.data[510:520]
    assert len(inner.reads) == 2
    assert inner.reads[1] == (510, 512)  # fetch starts at 510, not 512


def test_buffered_reader_backwards_seek_triggers_new_fetch() -> None:
    # Edge case: after the window has moved forward, seeking to an
    # earlier offset is a miss (covers() rejects offset < buf_offset_),
    # not a bug that reads garbage via a negative relative index.
    inner = CountingReader(bytes(i & 0xFF for i in range(10000)))
    r = BufferedRangeReader(inner, fetch_size=1024)

    r.read(5000, 4)  # buffer window becomes [5000, 6000)
    assert inner.reads == [(5000, 1024)]

    back = r.read(10, 4)
    assert back == inner.data[10:14]
    assert len(inner.reads) == 2
    assert inner.reads[1] == (10, 1024)


def test_buffered_reader_read_ending_exactly_at_total_size() -> None:
    # Edge case: the requested range's end coincides exactly with
    # total_size() -- both the outer read() and the inner over-fetch
    # must clamp to it without over-running or raising.
    inner = CountingReader(bytes(i & 0xFF for i in range(100)))
    r = BufferedRangeReader(inner, fetch_size=1024)

    got = r.read(90, 10)
    assert got == inner.data[90:100]
    assert inner.reads == [(90, 10)]
