"""Plugging in your own byte source by implementing RangeReader.

    python examples/custom_reader.py in.fcb [minx miny maxx maxy]

`RangeReader` is a Protocol -- two methods, no base class to inherit.
Implement it and every reader, index and query works unchanged over
whatever transport you have: S3, a database blob, an mmap, a test
double. The one below wraps a file and counts what the library asks
for, which is how the request counts in the docs were measured.

The contract, in full (range_reader.py):

* `read(offset, length)` returns EXACTLY `length` bytes, unless the
  range crosses the end of the resource, in which case it returns
  exactly the bytes that exist (possibly zero).
* `length == 0` returns `b""` WITHOUT contacting the transport.
* Errors raise `FcbError`. Returning garbage is not an option --
  callers cannot tell it from data.
* Instances are NOT thread-safe.
"""

from __future__ import annotations

import sys

import flatcitybuf as fcb


class CountingFileReader:
    """A RangeReader over a local file that records every read."""

    def __init__(self, path: str) -> None:
        self._fh = open(path, "rb")
        self._fh.seek(0, 2)
        self._size = self._fh.tell()
        self.reads: list[tuple[int, int]] = []

    def read(self, offset: int, length: int) -> bytes:
        if length == 0:
            return b""  # the contract: no transport call at all
        self.reads.append((offset, length))
        self._fh.seek(offset)
        return self._fh.read(length)

    def total_size(self) -> int:
        return self._size

    def close(self) -> None:
        self._fh.close()

    @property
    def bytes_read(self) -> int:
        return sum(n for _, n in self.reads)


def main(argv: list[str]) -> int:
    path = argv[0]
    box = tuple(float(v) for v in argv[1:5]) if len(argv) == 5 else None

    source = CountingFileReader(path)
    try:
        reader = fcb.FcbReader.open(source)
        info = reader.header.info
        print(
            f"opened: {info.features_count} features, "
            f"{source.total_size()} bytes on disk, "
            f"{len(source.reads)} read(s) so far"
        )

        if box is None:
            print("pass a bbox to see how little a query actually reads")
            return 0

        before = len(source.reads), source.bytes_read
        hits = fcb.search_rtree(
            reader.range_reader,
            reader.header.layout.rtree_begin,
            info.features_count,
            info.index_node_size,
            box,  # type: ignore[arg-type]
        )
        for hit in hits[:5]:
            print(f"  {reader.feature_at(hit).id}")
        if len(hits) > 5:
            print(f"  ... {len(hits) - 5} more")

        n = len(source.reads) - before[0]
        b = source.bytes_read - before[1]
        pct = 100.0 * b / source.total_size()
        print(
            f"{len(hits)} hit(s): {n} read(s), {b} bytes "
            f"({pct:.1f}% of the file)"
        )
        return 0
    finally:
        source.close()


if __name__ == "__main__":
    if len(sys.argv) not in (2, 6):
        print(__doc__)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1:]))
