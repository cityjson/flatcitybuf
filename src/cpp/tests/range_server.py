#!/usr/bin/env python3
"""A minimal HTTP server that actually implements Range requests.

Python's own `http.server` does NOT: SimpleHTTPRequestHandler has no Range
or Content-Range handling in any current version, so it answers every
request with 200 and the whole body. Testing a range client against it would
validate nothing and would mask exactly the bugs this exists to catch.

Query parameters select deliberately awkward server behaviour so the client's
fallback paths are exercised:

    ?ignore_range=1     answer 200 with the entire body despite a Range header
    ?bad_range=1        answer 206 with a malformed Content-Range
    ?wrong_offset=1     answer 206 with a range the client did not ask for
    ?long_end=1         answer 206 with an end far past what was requested
                        (start still matches -- only the end is wrong)
    ?stall_body=1       send 206 headers, then hang forever without ever
                        writing the body (simulates a stalled connection;
                        the client's own timeout must be what fires)
    ?no_etag=1          omit the ETag/Last-Modified validators
    ?no_cors_expose=1   send Access-Control-Allow-Origin but NOT
                        Access-Control-Expose-Headers, so a cross-origin
                        browser client cannot read Content-Range/
                        Content-Length off the response even though the
                        server sent them -- exercises the client's
                        RangeHeadersNotExposed failure path (Task 19)

Every response (regardless of the modes above) carries
`Access-Control-Allow-Origin: *` and, unless `no_cors_expose` is set,
`Access-Control-Expose-Headers: Content-Range, Content-Length` -- real
servers must opt in to exposing those two to cross-origin JS, and a Node
`fetch` (which does not enforce CORS at all) cannot exercise that failure
mode. `no_cors_expose` exists so a browser-based test can.

Binds port 0 and prints the chosen port on stdout so a test harness can find
it without guessing.
"""

import os
import re
import sys
import time

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

DATA_DIR = sys.argv[1] if len(sys.argv) > 1 else "."
ETAG = '"fcb-test-etag"'

RANGE_RE = re.compile(r"^bytes=(\d*)-(\d*)$")


class RangeHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, format, *args):  # noqa: A002 - keep test output quiet
        pass

    def _resolve(self):
        parsed = urlparse(self.path)
        name = os.path.basename(parsed.path)
        path = os.path.join(DATA_DIR, name)
        if not os.path.isfile(path):
            self.send_error(404)
            return None, None
        return path, parse_qs(parsed.query) or {}

    def _send_common(self, opts, extra_len):
        if "no_etag" not in opts:
            self.send_header("ETag", ETAG)
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("Content-Length", str(extra_len))
        # A real cross-origin client only gets to read a handful of
        # "safelisted" response headers unless the server opts in via
        # Access-Control-Expose-Headers -- Content-Range is not one of the
        # safelisted ones, so without this a browser fetch() would see a
        # 206 with `Content-Range: null`, indistinguishable here from a
        # server that never sent it. `no_cors_expose` omits the second
        # header (but keeps Allow-Origin) so that failure path is
        # reproducible on demand.
        self.send_header("Access-Control-Allow-Origin", "*")
        if "no_cors_expose" not in opts:
            self.send_header("Access-Control-Expose-Headers", "Content-Range, Content-Length")

    def do_HEAD(self):
        path, opts = self._resolve()
        if path is None:
            return
        size = os.path.getsize(path)
        self.send_response(200)
        self._send_common(opts, size)
        self.end_headers()

    def do_GET(self):
        path, opts = self._resolve()
        if path is None:
            return
        size = os.path.getsize(path)

        # If-Match must fail loudly when the client pins a different version.
        if_match = self.headers.get("If-Match")
        if if_match and "no_etag" not in opts and if_match != ETAG:
            self.send_response(412)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return

        rng = self.headers.get("Range")
        if not rng or "ignore_range" in opts:
            with open(path, "rb") as f:
                body = f.read()
            self.send_response(200)
            self._send_common(opts, len(body))
            self.end_headers()
            self.wfile.write(body)
            return

        m = RANGE_RE.match(rng.strip())
        if not m:
            self.send_response(400)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return

        start = int(m.group(1)) if m.group(1) else 0
        end = int(m.group(2)) if m.group(2) else size - 1

        if start >= size:
            self.send_response(416)
            self.send_header("Content-Range", f"bytes */{size}")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return

        end = min(end, size - 1)
        if "wrong_offset" in opts and start + 8 <= end:
            start += 8  # answer a range the client did not request
        if "long_end" in opts and end < size - 1:
            end = size - 1  # answer with far more than was requested

        with open(path, "rb") as f:
            f.seek(start)
            body = f.read(end - start + 1)

        self.send_response(206)
        if "bad_range" in opts:
            self.send_header("Content-Range", "totally not a range")
        else:
            self.send_header("Content-Range", f"bytes {start}-{end}/{size}")
        self._send_common(opts, len(body))
        self.end_headers()
        if "stall_body" in opts:
            # Headers are already flushed; never write the body and
            # never close the connection. The client's own read
            # timeout is what must fire here -- nothing on this side
            # ever unblocks it. The request-handling thread sits here
            # until the whole server process is killed at test
            # teardown (see test_http.py's `server` fixture), which
            # does not wait on this thread.
            time.sleep(3600)
            return
        self.wfile.write(body)


def main():
    server = ThreadingHTTPServer(("127.0.0.1", 0), RangeHandler)
    print(server.server_address[1], flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
