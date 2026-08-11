"""Reading over HTTP range requests -- the point of the format.

    python examples/read_http.py <url> [minx miny maxx maxy]

Opening reads only the header. A bbox query then reads a few index
pages plus the matching features, so a query against a file of tens of
gigabytes transfers kilobytes. `request_count` is printed throughout
because the request count, not the wall clock, is what the format is
designed to minimise.

ALWAYS pass a bbox on a large file: with no bbox this scans everything.
"""

from __future__ import annotations

import sys
import time

import flatcitybuf as fcb


def main(argv: list[str]) -> int:
    url = argv[0]
    box = tuple(float(v) for v in argv[1:5]) if len(argv) == 5 else None

    t0 = time.time()
    source = fcb.HttpRangeReader(url)
    reader = fcb.FcbReader.open(source)
    info = reader.header.info
    print(f"{info.features_count} features, CityJSON {info.cityjson_version}")
    print(
        f"opened in {source.request_count} HTTP request(s), "
        f"{time.time() - t0:.1f}s"
    )

    if box is None:
        print("no bbox given; not scanning the whole file -- pass one")
        return 0

    t1 = time.time()
    hits = fcb.search_rtree(
        reader.range_reader,
        reader.header.layout.rtree_begin,
        info.features_count,
        info.index_node_size,
        box,  # type: ignore[arg-type]
    )
    print(
        f"{len(hits)} feature(s) in the query bbox, "
        f"{source.request_count} HTTP request(s), {time.time() - t1:.1f}s"
    )

    # feature_at reads through a window shared across calls, so walking
    # a sorted hit list costs a fetch per window, not one per feature.
    t2 = time.time()
    for hit in hits:
        fcb.to_cityjson_feature(reader.feature_at(hit), reader.header)
    print(
        f"decoded all {len(hits)}, {source.request_count} HTTP request(s) "
        f"total, +{time.time() - t2:.1f}s"
    )
    return 0


if __name__ == "__main__":
    if len(sys.argv) not in (2, 6):
        print(__doc__)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1:]))
