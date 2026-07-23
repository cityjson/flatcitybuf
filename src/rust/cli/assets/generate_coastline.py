#!/usr/bin/env python3
"""Generate assets/coastline.csv (lon,lat per line) from Natural Earth 110m.

Run once; the produced CSV is committed so builds need no network. Source:
Natural Earth 1:110m coastline (public domain).

Usage:
    python3 generate_coastline.py \
        https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_110m_coastline.geojson

Points are decimated to at most one per ~0.75 degrees along each line so the
file stays a few tens of KB and reads well at terminal resolution.
"""
import json
import sys
import urllib.request

STEP_DEG = 0.75


def emit(coords, out):
    last = None
    for lon, lat in coords:
        if last is None or abs(lon - last[0]) + abs(lat - last[1]) >= STEP_DEG:
            out.append((round(lon, 3), round(lat, 3)))
            last = (lon, lat)


def main() -> None:
    url = sys.argv[1]
    with urllib.request.urlopen(url) as resp:
        gj = json.load(resp)
    out: list[tuple[float, float]] = []
    for feat in gj["features"]:
        geom = feat["geometry"]
        if geom["type"] == "LineString":
            emit(geom["coordinates"], out)
        elif geom["type"] == "MultiLineString":
            for line in geom["coordinates"]:
                emit(line, out)
    with open("coastline.csv", "w", encoding="utf-8") as f:
        f.write("# lon,lat decimated Natural Earth 110m coastline (public domain)\n")
        for lon, lat in out:
            f.write(f"{lon},{lat}\n")
    print(f"wrote {len(out)} points")


if __name__ == "__main__":
    main()
