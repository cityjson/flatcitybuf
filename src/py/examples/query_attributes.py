"""Attribute queries through the static B+tree.

    python examples/query_attributes.py f.fcb b3_h_dak_50p gt 20
    python examples/query_attributes.py f.fcb b3_h_dak_50p gt 20 \\
                                              b3_dak_type eq slanted

Several conditions are ANDed. The comparison value is a typed KeyValue
and **its type must match the column's type on disk** -- a mismatch does
not throw, it reinterprets bytes and returns plausible garbage. So this
looks the column up in the header and builds the KeyValue from its
declared type rather than guessing from how the argument looks.
"""

from __future__ import annotations

import sys

import flatcitybuf as fcb
from flatcitybuf.generated.header_generated import ColumnType

# The raw ubyte in ColumnInfo.type, spelled back out for display.
_TYPE_NAMES = {
    getattr(ColumnType, n): n for n in dir(ColumnType) if not n.startswith("_")
}

_OPS = {
    "eq": fcb.Operator.EQ,
    "ne": fcb.Operator.NE,
    "gt": fcb.Operator.GT,
    "ge": fcb.Operator.GE,
    "lt": fcb.Operator.LT,
    "le": fcb.Operator.LE,
}


def _key_value(col_type: int, text: str) -> fcb.KeyValue:
    """Build a KeyValue of the column's own type."""
    if col_type in (ColumnType.Float, ColumnType.Double):
        return fcb.KeyValue.from_f64(float(text))
    if col_type == ColumnType.Bool:
        return fcb.KeyValue.from_bool(text.lower() in ("1", "true", "yes"))
    if col_type in (ColumnType.String, ColumnType.Json):
        return fcb.KeyValue.from_string(fcb.KeyKind.STRING50, text)
    if col_type in (
        ColumnType.Byte,
        ColumnType.Short,
        ColumnType.Int,
        ColumnType.Long,
    ):
        return fcb.KeyValue.from_i64(int(text))
    return fcb.KeyValue.from_u64(int(text))


def main(argv: list[str]) -> int:
    path, rest = argv[0], argv[1:]
    if len(rest) % 3 != 0 or not rest:
        print(__doc__)
        return 2

    reader = fcb.FcbReader.open_file(path)
    by_name = {c.name: c for c in reader.header.info.columns}

    conditions = []
    for field, op, text in zip(rest[0::3], rest[1::3], rest[2::3]):
        col = by_name.get(field)
        if col is None:
            print(f"error: no column named {field!r}", file=sys.stderr)
            return 1
        if op not in _OPS:
            print(f"error: unknown operator {op!r}", file=sys.stderr)
            return 1
        kind = _TYPE_NAMES.get(col.type, f"?{col.type}")
        print(f"condition: {field} {op} {text} (column type {kind})")
        conditions.append(
            fcb.AttrCondition(field, _OPS[op], _key_value(col.type, text))
        )

    hits = reader.select_attr(conditions)
    total = reader.header.info.features_count
    print(f"{len(hits)} of {total} features matched")
    for hit in hits[:20]:
        print(f"  {reader.feature_at(hit).id}")
    if len(hits) > 20:
        print(f"  ... {len(hits) - 20} more")
    return 0


if __name__ == "__main__":
    if len(sys.argv) < 5:
        print(__doc__)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1:]))
