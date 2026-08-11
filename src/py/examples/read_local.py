"""Whole file (or one bbox) out as CityJSONSeq, on stdout.

    python examples/read_local.py in.fcb > out.city.jsonl
    python examples/read_local.py in.fcb 84500 445800 85000 446500

With a bbox the R-tree answers first and only the matching features are
decoded; without one this is a straight sequential scan. Progress goes
to stderr so stdout stays a clean CityJSONSeq stream.
"""

from __future__ import annotations

import json
import sys

import flatcitybuf as fcb


def main(argv: list[str]) -> int:
    path = argv[0]
    box = tuple(float(v) for v in argv[1:5]) if len(argv) == 5 else None

    reader = fcb.FcbReader.open_file(path)
    info = reader.header.info
    print(
        f"{info.features_count} features, CityJSON {info.cityjson_version}"
        f"{', ' + info.crs if info.crs else ''}",
        file=sys.stderr,
    )

    # Line 0 is the CityJSON header: transform, metadata, and the
    # geometry templates and appearance palette if the file has them.
    print(json.dumps(fcb.to_cityjson_metadata(reader.header)))

    if box is None:
        features = reader.select_all()
    else:
        # search_rtree is a free function over a RangeReader and hands
        # back byte offsets, not features; feature_at is the way back.
        hits = fcb.search_rtree(
            reader.range_reader,
            reader.header.layout.rtree_begin,
            info.features_count,
            info.index_node_size,
            box,  # type: ignore[arg-type]
        )
        print(f"{len(hits)} feature(s) in the bbox", file=sys.stderr)
        features = (reader.feature_at(h) for h in hits)

    n = 0
    for feature in features:
        print(json.dumps(fcb.to_cityjson_feature(feature, reader.header)))
        n += 1
    print(f"wrote {n} feature(s)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    if len(sys.argv) not in (2, 6):
        print(__doc__)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1:]))
