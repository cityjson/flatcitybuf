"""The CityJSON representation, and how to reach into its fields.

    python examples/to_cityjson.py in.fcb [feature_index]

`to_cityjson_metadata` gives the CityJSONSeq header line and
`to_cityjson_feature` gives one feature line; both return plain dicts,
so everything below is ordinary dict access, not a bespoke API.
"""

from __future__ import annotations

import itertools
import json
import sys

import flatcitybuf as fcb


def main(path: str, index: int) -> int:
    reader = fcb.FcbReader.open_file(path)

    print("== metadata (to_cityjson_metadata) ==")
    meta = fcb.to_cityjson_metadata(reader.header)
    print(f"  version   {meta['version']}")
    tr = meta["transform"]
    print(f"  scale     {tr['scale']}")
    print(f"  translate {tr['translate']}")
    md = meta.get("metadata", {})
    if "referenceSystem" in md:
        print(f"  CRS       {md['referenceSystem']}")
    if "geographicalExtent" in md:
        print(f"  extent    {md['geographicalExtent']}")

    # Present only when the file has them. The templates' material and
    # texture mappings index the header's OWN appearance palette, which
    # is why both must be emitted together.
    if "geometry-templates" in meta:
        t = meta["geometry-templates"]["templates"]
        print(f"  templates {len(t)}")
    if "appearance" in meta:
        ap = meta["appearance"]
        print(
            f"  palette   {len(ap.get('materials', []))} material(s), "
            f"{len(ap.get('textures', []))} texture(s)"
        )
    if "extensions" in meta:
        print(f"  extensions {sorted(meta['extensions'])}")

    print()
    print(f"== feature {index} (to_cityjson_feature) ==")
    feature = next(itertools.islice(reader.select_all(), index, index + 1))
    cj = fcb.to_cityjson_feature(feature, reader.header)
    print(f"  id        {cj['id']}")
    print(f"  vertices  {len(cj['vertices'])}")
    for obj_id, obj in cj["CityObjects"].items():
        geoms = obj.get("geometry", [])
        lods = [g.get("lod") for g in geoms]
        print(f"  object    {obj_id}")
        print(f"    type      {obj['type']}")
        print(f"    geometry  {len(geoms)} (lod {lods})")
        attrs = obj.get("attributes", {})
        if attrs:
            first = dict(itertools.islice(attrs.items(), 3))
            print(f"    attrs     {len(attrs)}, e.g. {first}")

    print()
    print("  the whole feature as one JSON line is what read_local.py")
    print(f"  writes; it is {len(json.dumps(cj))} bytes here")
    return 0


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3):
        print(__doc__)
        raise SystemExit(2)
    i = int(sys.argv[2]) if len(sys.argv) == 3 else 0
    raise SystemExit(main(sys.argv[1], i))
