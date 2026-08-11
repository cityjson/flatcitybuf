"""Raw feature access, without converting to CityJSON.

    python examples/read_features.py in.fcb [count]

Shows the thing most easily got wrong: **attribute schemas are per
object**. A CityObject that carries its own `columns` overrides the
header's, and that is the normal case, not the exception. Attribute
blobs are not self-delimiting, so decoding one against the wrong schema
yields plausible garbage rather than an error.
"""

from __future__ import annotations

import itertools
import sys

import flatcitybuf as fcb


def main(path: str, limit: int) -> int:
    reader = fcb.FcbReader.open_file(path)
    header_columns = reader.header.info.columns

    own_schema = 0
    shown = 0
    for feature in itertools.islice(reader.select_all(), limit):
        objects = feature.city_objects()
        print(f"feature {feature.id}  ({len(objects)} CityObjects)")
        for obj in objects:
            print(f"  object {obj.id}")

            # Presence, not emptiness: an object that declares an empty
            # column list still overrides the header's schema.
            if obj.has_columns and obj.columns is not None:
                schema = obj.columns
                own_schema += 1
                print(f"    schema   own ({len(schema)} columns)")
            else:
                schema = header_columns
                print(f"    schema   header ({len(schema)} columns)")

            if not obj.has_attributes:
                print("    (no attributes)")
                continue
            blob = obj.attributes or b""
            if not blob:
                print("    (attribute blob present but empty)")
                continue
            attrs = fcb.decode_attributes(blob, schema)
            for key, value in itertools.islice(attrs.items(), 4):
                print(f"    {key} = {value!r}")
            if len(attrs) > 4:
                print(f"    ... {len(attrs) - 4} more attribute(s)")
        shown += 1

    print()
    print(
        f"{shown} feature(s) shown; {own_schema} object(s) carried "
        f"their own schema"
    )
    return 0


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3):
        print(__doc__)
        raise SystemExit(2)
    n = int(sys.argv[2]) if len(sys.argv) == 3 else 1
    raise SystemExit(main(sys.argv[1], n))
