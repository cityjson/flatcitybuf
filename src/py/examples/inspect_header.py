"""Header only: extent, CRS, transform, and which columns are queryable.

Opening reads the header and nothing else, so this costs one small read
even on a file of tens of gigabytes. Start here on an unfamiliar file.

    python examples/inspect_header.py ../../examples/data/delft.fcb
"""

from __future__ import annotations

import sys

import flatcitybuf as fcb
from flatcitybuf.generated.header_generated import ColumnType

# The raw ubyte in ColumnInfo.type, spelled back out for display.
_TYPE_NAMES = {
    getattr(ColumnType, n): n for n in dir(ColumnType) if not n.startswith("_")
}


def main(path: str) -> int:
    reader = fcb.FcbReader.open_file(path)
    info = reader.header.info

    print(f"file          {path}")
    print(f"features      {info.features_count}")
    print(f"CityJSON      {info.cityjson_version}")
    if info.title:
        print(f"title         {info.title}")
    if info.crs:
        print(f"CRS           {info.crs}")

    if info.geographical_extent is not None:
        e = info.geographical_extent
        print(
            f"extent        [{e[0]:.3f} {e[1]:.3f} {e[2]:.3f}]"
            f" .. [{e[3]:.3f} {e[4]:.3f} {e[5]:.3f}]"
        )
    if info.scale is not None and info.translate is not None:
        s, t = info.scale, info.translate
        print(
            f"transform     scale [{s[0]} {s[1]} {s[2]}]"
            f" translate [{t[0]:.3f} {t[1]:.3f} {t[2]:.3f}]"
        )

    has_rtree = reader.header.layout.rtree_size > 0
    node = info.index_node_size
    print(
        f"R-tree        {'yes (node size %d)' % node if has_rtree else 'no'}"
    )

    # A column is queryable only if the writer gave it a B+tree. The
    # header lists the indices separately from the columns, keyed by
    # column index -- so this is a set membership test, not a flag on
    # ColumnInfo.
    indexed = {a.column_index for a in reader.header.attr_indices}
    print()
    print(f"columns ({len(info.columns)}; * = queryable via select_attr)")
    for col in info.columns:
        mark = "*" if col.index in indexed else " "
        name = _TYPE_NAMES.get(col.type, f"?{col.type}")
        print(f"  {mark} {col.name:<34} {name}")
    print()
    print(f"{len(indexed)} of {len(info.columns)} columns are queryable")
    if info.semantic_columns:
        print(f"semantic columns: {len(info.semantic_columns)}")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1]))
