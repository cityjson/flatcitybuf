from __future__ import annotations

import re
import urllib.error
import urllib.request
from typing import Any

from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.range_reader import RangeReader

# http_reader/mod.rs:42, cited in the Format Reference's "HTTP
# constants" table (docs/superpowers/plans/2026-07-19-native-cpp-core.md,
# line 161) as DEFAULT_HTTP_FETCH_SIZE. The Rust source comments it
# "the largest request we'll speculatively make" -- a CAP, not a
# minimum floor. That is exactly how it is used here: `read()` never
# issues a single physical HTTP request larger than `fetch_size`,
# splitting a bigger request into several sequential ones instead.
#
# This deliberately does NOT mirror the other three "HTTP constants"
# table entries (the 12944-byte open prefetch, and the 256 KiB / 1 MiB
# combine thresholds): those already live in header.py
# (`_OPEN_PREFETCH_SIZE`), reader.py (`_FEATURE_FETCH_SIZE`) and
# stree.py (`_INDEX_FETCH_SIZE`), each wrapping whatever raw
# `RangeReader` it is given -- FileRangeReader or this one -- in its
# OWN per-phase `BufferedRangeReader`. HttpRangeReader is that same
# kind of raw, per-call adapter (like FileRangeReader): it does not
# cache or widen requests on its own beyond the `fetch_size` safety
# cap below, precisely so it composes with those existing wrappers
# instead of fighting them -- see the task report for the reasoning
# (a default-buffering HttpRangeReader would silently balloon the
# 12944-byte open prefetch into a much larger first request).
DEFAULT_HTTP_FETCH_SIZE = 1_048_576

# RFC 7233 Content-Range, as sent on a 206: "bytes <start>-<end>/<total>".
_CONTENT_RANGE_RE = re.compile(r"^bytes (\d+)-(\d+)/(\d+)$")
# ...and on a 416: "bytes */<total>".
_UNSATISFIED_RANGE_RE = re.compile(r"^bytes \*/(\d+)$")

# Bound on a single socket read() while draining a response body, so a
# short read (guaranteed possible per http.client/urllib semantics)
# never turns into "assume one read() call returns everything".
_READ_CHUNK = 65536


def _read_body(response: Any, limit: int) -> bytes:
    """Read AT MOST `limit` bytes from `response`, looping because a
    single `.read()` call may return fewer bytes than asked. Never
    requests more than `limit` bytes in total, so an oversized or
    hostile response cannot provoke an unbounded read here -- the
    caller has already bounded `limit` to the requested range."""
    chunks: list[bytes] = []
    remaining = limit
    while remaining > 0:
        chunk = response.read(min(remaining, _READ_CHUNK))
        if not chunk:
            break
        chunks.append(chunk)
        remaining -= len(chunk)
    return b"".join(chunks)


class HttpRangeReader(RangeReader):
    """HTTP range-request adapter on stdlib `urllib.request`. Mirrors
    fcb::CurlRangeReader (curl_range_reader.hpp/.cpp), with one
    deliberate divergence: see "server ignoring Range" below and the
    task report.

    Like FileRangeReader, this is a RAW, per-call adapter: `read`
    issues one or more physical Range GETs covering exactly what was
    asked (a single request when `length <= fetch_size`, several
    sequential ones otherwise -- see `fetch_size` above), and does not
    cache bytes across calls. Buffering/prefetching is layered on
    externally by `BufferedRangeReader`, exactly as the rest of this
    codebase already does for both FileRangeReader and this class
    (header.py, reader.py, stree.py).

    `total_size()` is derived from the very first request's own
    `Content-Range` header (RFC 7233's `bytes a-b/N`) rather than a
    separate HEAD request: HEAD support is not universal, and a
    Range GET answers the same question in one round trip while also
    exercising the identical 200-vs-206 check every other read uses.
    Content-Length is deliberately never trusted for this -- on a 206
    response it is the length of the slice, not the resource.

    Correctness traps this addresses (see the task brief):

    1. A server that ignores Range and answers 200 with the whole body
       is REJECTED (`FcbError(IO_ERROR)`), not silently sliced as if it
       had honoured the request. This is a deliberate DIVERGENCE from
       fcb::CurlRangeReader, which slices a 200 body correctly instead
       of rejecting it -- the task brief for this Python port requires
       the stricter behaviour explicitly.
    2. total_size() comes from Content-Range on a 206, never from
       Content-Length (which is only the slice length on a partial
       response).
    3. Every network failure (URLError, HTTPError, timeout, a short
       body) surfaces as FcbError(IO_ERROR); nothing raw escapes.
    4. `fetch_size` bounds every single physical request/allocation
       this reader makes, so a corrupt or hostile `length` cannot
       provoke an unbounded fetch -- see `read`.
    """

    def __init__(
        self,
        url: str,
        fetch_size: int = DEFAULT_HTTP_FETCH_SIZE,
        timeout: float = 30.0,
    ) -> None:
        if fetch_size <= 0:
            raise ValueError("fetch_size must be a positive byte count")
        self._url = url
        self._fetch_size = fetch_size
        self._timeout = timeout
        self._total_size: int | None = None

        # Not part of the RangeReader protocol -- exists purely so
        # tests can assert on the buffering/prefetch behaviour that is
        # this whole format's reason for existing (mirrors
        # CurlRangeReader::request_count(), curl_range_reader.hpp:48).
        self.request_count = 0
        self.bytes_fetched = 0

    def total_size(self) -> int:
        if self._total_size is None:
            # A 1-byte probe is enough to learn Content-Range's total,
            # and it goes through the exact same 200/206/416 handling
            # as every other read (including the "reject a 200"
            # check), so a Range-ignoring server is rejected right
            # here rather than only on a later, larger read.
            self._fetch_range(0, 1)
        assert self._total_size is not None
        return self._total_size

    def read(self, offset: int, length: int) -> bytes:
        if length == 0:
            return b""  # contract: never contact the transport

        total = self.total_size()
        if offset > total:
            raise FcbError(
                ErrorCode.INDEX_OUT_OF_BOUNDS,
                f"read offset {offset} is past end of resource "
                f"(size={total}): {self._url}",
            )
        # Clamp to EOF: a range crossing the end returns exactly what
        # exists, matching FileRangeReader.
        n = min(length, total - offset)
        if n == 0:
            return b""

        # `fetch_size` caps a single physical request. Almost always
        # this loop runs once -- every call site in this codebase
        # wraps its raw reader in its own per-phase
        # BufferedRangeReader, so `length` here is already bounded by
        # that phase's own fetch size. This loop exists for the case
        # where it isn't (a caller using HttpRangeReader unwrapped, or
        # a request larger than `fetch_size`), so no single request or
        # allocation this reader makes can be driven arbitrarily large
        # by a corrupt/hostile length.
        out = bytearray()
        pos = offset
        remaining = n
        while remaining > 0:
            chunk_len = min(remaining, self._fetch_size)
            out += self._fetch_range(pos, chunk_len)
            pos += chunk_len
            remaining -= chunk_len
        return bytes(out)

    def _fetch_range(self, offset: int, length: int) -> bytes:
        """One physical HTTP request for the half-open byte range
        [offset, offset + length)."""
        last = offset + length - 1
        request = urllib.request.Request(
            self._url,
            headers={"Range": f"bytes={offset}-{last}"},
        )
        response: Any
        try:
            response = urllib.request.urlopen(request, timeout=self._timeout)
        except urllib.error.HTTPError as exc:
            # HTTPError doubles as a response object (status, headers,
            # body) for 4xx/5xx -- 416 in particular is meaningful, not
            # just a hard failure.
            response = exc
        except urllib.error.URLError as exc:
            raise FcbError(
                ErrorCode.IO_ERROR,
                f"HTTP request to {self._url} failed: {exc}",
            ) from exc
        except OSError as exc:
            # Covers timeouts and other socket-level failures that are
            # not routed through URLError.
            raise FcbError(
                ErrorCode.IO_ERROR,
                f"HTTP request to {self._url} failed: {exc}",
            ) from exc

        try:
            self.request_count += 1
            status = response.getcode()
            headers = response.headers

            if status == 200:
                # The server ignored Range and is about to hand back
                # the WHOLE representation. Reject before ever reading
                # the body: for a large or hostile remote file that
                # body could be gigabytes, and reading it would both
                # be slow and silently mis-slice (bytes [0, length),
                # not [offset, offset+length)) if truncated naively.
                raise FcbError(
                    ErrorCode.IO_ERROR,
                    "server ignored the Range header and answered 200 "
                    "with the full body; a 206 Partial Content "
                    "response is required",
                )

            if status == 206:
                content_range = headers.get("Content-Range", "")
                match = _CONTENT_RANGE_RE.match(content_range.strip())
                if not match:
                    raise FcbError(
                        ErrorCode.IO_ERROR,
                        "malformed or missing Content-Range on a 206 "
                        f"response: {content_range!r}",
                    )
                start = int(match.group(1))
                end = int(match.group(2))
                total = int(match.group(3))
                if start != offset:
                    raise FcbError(
                        ErrorCode.IO_ERROR,
                        f"server returned range starting at {start}, "
                        f"expected {offset}",
                    )
                if self._total_size is None:
                    self._total_size = total
                elif self._total_size != total:
                    raise FcbError(
                        ErrorCode.IO_ERROR,
                        "resource size changed between requests (was "
                        f"{self._total_size}, now {total}); the URL "
                        "is not stable",
                    )
                want = end - start + 1
                body = _read_body(response, want)
                if len(body) != want:
                    raise FcbError(
                        ErrorCode.IO_ERROR,
                        "truncated response body for range "
                        f"bytes={offset}-{last}",
                    )
                self.bytes_fetched += len(body)
                return body

            if status == 416:
                # Unsatisfiable. The only legitimate case is an empty
                # (zero-length) resource, which the very first 1-byte
                # probe in total_size() will hit.
                content_range = headers.get("Content-Range", "")
                match2 = _UNSATISFIED_RANGE_RE.match(content_range.strip())
                if match2 is not None and int(match2.group(1)) == 0:
                    self._total_size = 0
                    return b""
                raise FcbError(
                    ErrorCode.IO_ERROR,
                    f"server returned 416 for range bytes={offset}-{last}",
                )

            raise FcbError(
                ErrorCode.IO_ERROR,
                f"unexpected HTTP status {status} for {self._url}",
            )
        finally:
            response.close()
