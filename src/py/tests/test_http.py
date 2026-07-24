from __future__ import annotations

import os
import select
import subprocess
import sys
import time
from pathlib import Path
from typing import Iterator

import pytest
from flatcitybuf.cityjson import to_cityjson_feature
from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.http_reader import HttpRangeReader
from flatcitybuf.keys import KeyValue
from flatcitybuf.packed_rtree import search_rtree
from flatcitybuf.range_reader import FileRangeReader
from flatcitybuf.reader import FcbReader
from flatcitybuf.stree import AttrCondition, Operator

REPO_ROOT = Path(__file__).resolve().parents[3]
CORPUS = REPO_ROOT / "conformance"
DELFT = REPO_ROOT / "examples" / "data" / "delft.fcb"
RANGE_SERVER = REPO_ROOT / "src" / "cpp" / "tests" / "range_server.py"

SMALL = CORPUS / "small.fcb"

# ---------------------------------------------------- live remote fixture ---
#
# Opt-in integration test against the real, published 3DBAG file
# (~68 GB, EPSG:28992). It is SKIPPED unless FCB_REMOTE_HTTP_URL is set --
# CI never touches the network here, and nobody downloads 68 GB by running
# `pytest`. Enable it with `just test-remote` (which sets the env var to the
# default URL below), or point FCB_REMOTE_HTTP_URL at any current-format file.
#
# The expected values were cross-checked across the Rust, C++, Python and
# TypeScript readers on 2026-07-23; all four agree. If the file is
# regenerated they must be updated in lock-step here and in the other three
# suites (src/rust/fcb_core/tests/http.rs, src/cpp/tests/test_http.cpp,
# src/ts/test/http.test.ts).
REMOTE_URL = os.environ.get("FCB_REMOTE_HTTP_URL", "")
REMOTE_DEFAULT_URL = (
    "https://storage.googleapis.com/flatcitybuf/3dbag_all_index.fcb"
)
REMOTE_FEATURES_COUNT = 10_771_547
# A ~1 km box over central Amsterdam (minx, miny, maxx, maxy), well inside
# the national extent.
REMOTE_BBOX = (120_000.0, 486_000.0, 121_000.0, 487_000.0)
REMOTE_BBOX_COUNT = 2762

# ---------------------------------------------------------- server fixture ---
#
# src/cpp/tests/range_server.py is the C++ suite's own range-capable test
# server (real Range/Content-Range handling, plus ?ignore_range=1,
# ?bad_range=1, ?wrong_offset=1, ?no_etag=1 query-param misbehaviours --
# see its own module docstring). Reused here rather than writing a second
# one. It binds port 0 and prints the chosen port on stdout.


def _wait_for_port(proc: subprocess.Popen[str], timeout: float = 10.0) -> int:
    """Read the port range_server.py prints, without ever blocking past
    `timeout` -- a server that fails to start must not hang the suite."""
    deadline = time.monotonic() + timeout
    assert proc.stdout is not None
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            proc.kill()
            pytest.fail("range_server.py did not report a port in time")
        if proc.poll() is not None:
            stderr = proc.stderr.read() if proc.stderr else ""
            pytest.fail(
                "range_server.py exited before reporting a port "
                f"(returncode={proc.returncode}): {stderr}"
            )
        ready, _, _ = select.select([proc.stdout], [], [], remaining)
        if ready:
            line = proc.stdout.readline()
            if line.strip():
                return int(line.strip())


@pytest.fixture(scope="module")
def server(tmp_path_factory: pytest.TempPathFactory) -> Iterator[str]:
    data_dir = tmp_path_factory.mktemp("range_server_data")
    (data_dir / "small.fcb").symlink_to(SMALL)
    if DELFT.exists():
        (data_dir / "delft.fcb").symlink_to(DELFT)
    proc = subprocess.Popen(
        [sys.executable, str(RANGE_SERVER), str(data_dir)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    try:
        port = _wait_for_port(proc)
        yield f"http://127.0.0.1:{port}"
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)


# ------------------------------------------------------------- the brief ---


def test_total_size_matches_the_local_file(server: str) -> None:
    r = HttpRangeReader(f"{server}/small.fcb")
    assert r.total_size() == SMALL.stat().st_size


def test_full_scan_over_http_matches_the_local_reader_exactly(
    server: str,
) -> None:
    local = FcbReader.open_file(SMALL)
    remote = FcbReader.open(HttpRangeReader(f"{server}/small.fcb"))

    local_features = [
        to_cityjson_feature(f, local.header) for f in local.select_all()
    ]
    remote_features = [
        to_cityjson_feature(f, remote.header) for f in remote.select_all()
    ]

    assert len(remote_features) == 3
    assert remote_features == local_features


def test_server_ignoring_range_raises_instead_of_misslicing(
    server: str,
) -> None:
    # The dangerous case the brief calls out by name: a server that
    # answers 200 with the whole body must be rejected, not silently
    # sliced as if it had honoured the Range header.
    r = HttpRangeReader(f"{server}/small.fcb?ignore_range=1")
    with pytest.raises(FcbError) as exc_info:
        r.total_size()
    assert exc_info.value.code is ErrorCode.IO_ERROR


# --------------------------------------------------- beyond the brief ---


def test_reads_match_the_local_file_byte_for_byte(server: str) -> None:
    local = FileRangeReader(SMALL)
    remote = HttpRangeReader(f"{server}/small.fcb")

    assert remote.total_size() == local.total_size()
    assert remote.read(0, 64) == local.read(0, 64)
    assert remote.read(1000, 256) == local.read(1000, 256)

    n = local.total_size()
    assert remote.read(n - 16, 16) == local.read(n - 16, 16)


def test_zero_length_read_never_contacts_the_server(server: str) -> None:
    r = HttpRangeReader(f"{server}/small.fcb")
    r.total_size()
    before = r.request_count
    assert r.read(100, 0) == b""
    assert r.request_count == before


def test_read_past_end_raises_out_of_bounds(server: str) -> None:
    # Python's FileRangeReader raises past EOF (a deliberate divergence
    # from the C++ reference, documented on FileRangeReader itself);
    # HttpRangeReader matches that Python-side convention, not C++'s.
    r = HttpRangeReader(f"{server}/small.fcb")
    n = r.total_size()
    with pytest.raises(FcbError) as exc_info:
        r.read(n + 1, 4)
    assert exc_info.value.code is ErrorCode.INDEX_OUT_OF_BOUNDS


def test_a_range_crossing_eof_returns_exactly_the_bytes_that_exist(
    server: str,
) -> None:
    r = HttpRangeReader(f"{server}/small.fcb")
    n = r.total_size()
    assert len(r.read(n - 10, 100)) == 10


def test_malformed_content_range_is_rejected(server: str) -> None:
    r = HttpRangeReader(f"{server}/small.fcb?bad_range=1")
    with pytest.raises(FcbError) as exc_info:
        r.total_size()
    assert exc_info.value.code is ErrorCode.IO_ERROR


def test_a_range_answering_the_wrong_offset_is_rejected(server: str) -> None:
    r = HttpRangeReader(f"{server}/small.fcb?wrong_offset=1")
    # total_size()'s own 1-byte probe (bytes=0-0) is too small for
    # wrong_offset's "start + 8 <= end" trigger, so it still succeeds;
    # the shift shows up on the next, larger read.
    r.total_size()
    with pytest.raises(FcbError) as exc_info:
        r.read(0, 64)
    assert exc_info.value.code is ErrorCode.IO_ERROR


def test_a_range_answering_an_over_long_end_is_rejected(server: str) -> None:
    # A server that matches the requested START but answers with an
    # END far past it (here: the rest of the whole file) must be
    # rejected, not trusted: `want = end - start + 1` is derived
    # straight from the server's Content-Range, so an unvalidated end
    # turns directly into an unbounded read for what should have been
    # total_size()'s tiny 1-byte probe (`Range: bytes=0-0`).
    r = HttpRangeReader(f"{server}/small.fcb?long_end=1")
    with pytest.raises(FcbError) as exc_info:
        r.total_size()
    assert exc_info.value.code is ErrorCode.IO_ERROR
    # Direct evidence this is the bounded rejection and not the
    # unbounded-read bug: nothing was ever buffered into
    # bytes_fetched. Pre-fix, this assertion also fails -- the whole
    # file ends up counted here instead.
    assert r.bytes_fetched == 0


def test_a_stalled_connection_mid_body_raises_io_error(server: str) -> None:
    # A server that sends 206 headers and then hangs before writing
    # any body must not let a raw stdlib exception (e.g. TimeoutError)
    # escape -- every network failure must surface as
    # FcbError(IO_ERROR), per the RangeReader contract
    # (range_reader.py:25). A short client-side timeout keeps this
    # test itself bounded; the server-side request thread left
    # blocked is torn down by the `server` fixture's process-kill
    # teardown, not by anything here.
    r = HttpRangeReader(f"{server}/small.fcb?stall_body=1", timeout=0.5)
    with pytest.raises(FcbError) as exc_info:
        r.total_size()
    assert exc_info.value.code is ErrorCode.IO_ERROR


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_attribute_query_over_http_matches_the_local_reader(
    server: str,
) -> None:
    query = [AttrCondition("b3_bouwlagen", Operator.GE, KeyValue.from_u64(1))]
    local = FcbReader.open_file(DELFT)
    remote = FcbReader.open(HttpRangeReader(f"{server}/delft.fcb"))

    local_offsets = sorted(h.offset for h in local.select_attr(query))
    remote_offsets = sorted(h.offset for h in remote.select_attr(query))

    assert len(remote_offsets) > 0
    assert remote_offsets == local_offsets


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_http_reader_issues_ranged_requests_not_the_whole_file(
    server: str,
) -> None:
    # The entire point of a RangeReader: a query over a 7+ MB remote
    # file must fetch a small fraction of it, via more than one ranged
    # request -- never one big non-range GET of the whole body.
    query = [AttrCondition("b3_bouwlagen", Operator.GE, KeyValue.from_u64(1))]
    reader = HttpRangeReader(f"{server}/delft.fcb")
    total = reader.total_size()

    fcb = FcbReader.open(reader)
    hits = list(fcb.select_attr(query))

    assert hits
    assert reader.request_count > 1
    assert 0 < reader.bytes_fetched < total


# ---------------------------------------------------- live remote (opt-in) ---


@pytest.mark.skipif(
    not REMOTE_URL,
    reason="set FCB_REMOTE_HTTP_URL to run (see `just test-remote`)",
)
def test_remote_3dbag_opens_and_queries_over_http() -> None:
    """The published 3DBAG file, read over real HTTP range requests.

    Proves three things at once: the header passes verification (the file
    is in the post-alignment-fix format), the whole 68 GB is never
    downloaded (a bounded number of ranged requests), and the spatial
    index traverses to the same feature set the other three readers see.
    """
    reader = HttpRangeReader(REMOTE_URL)
    fcb = FcbReader.open(reader)
    header = fcb.header

    assert header.info.features_count == REMOTE_FEATURES_COUNT

    # Opening a 68 GB file must cost a handful of small ranged reads, not a
    # download. If this ever approaches total_size the range logic regressed.
    assert 0 < reader.bytes_fetched < 5_000_000

    hits = search_rtree(
        fcb.range_reader,
        header.layout.rtree_begin,
        header.info.features_count,
        header.info.index_node_size,
        REMOTE_BBOX,
    )
    # Exact, and identical to what the Rust, C++ and TypeScript readers
    # return for the same box -- see the constants' note above.
    assert len(hits) == REMOTE_BBOX_COUNT

    # The bbox scan reads more, but still a tiny fraction of the file.
    assert reader.bytes_fetched < total_size_guard(reader)


def total_size_guard(reader: HttpRangeReader) -> int:
    """A generous ceiling: 1% of the file. The Amsterdam box touches far
    less, but pinning an exact byte count would be brittle against future
    re-serializations, whereas 'never more than 1% of 68 GB' is a stable
    statement of the property under test."""
    return reader.total_size() // 100
