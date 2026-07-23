from __future__ import annotations

import struct
from pathlib import Path

import pytest
from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.header import AttrIndexInfo
from flatcitybuf.header import ColumnInfo
from flatcitybuf.header import FileInfo
from flatcitybuf.header import HeaderView
from flatcitybuf.header import read_header
from flatcitybuf.range_reader import FileRangeReader

CORPUS = Path(__file__).resolve().parents[3] / "conformance"
DELFT = Path(__file__).resolve().parents[3] / "examples" / "data" / "delft.fcb"

# Expected values below were obtained two ways and cross-checked:
#
# 1. `cd src/rust && ./target/release/fcb info <file>` -- the Rust CLI,
#    an oracle independent of this module, printed (rounded):
#    features=3 (small) / 1115 (delft), version "2.0", title "3DBAG",
#    extent min=[84501.55, 445805.03, -3.75] max=[85675.23, 446983.47,
#    95.04], scale=[0.001, 0.001, 0.001],
#    translate=[85088.390625, 446394.250000, 45.648003], and 44 columns
#    carrying a B+tree attribute index -- for BOTH fixtures (same schema).
# 2. The full-precision floats below were read directly with the
#    committed flatc-generated Python bindings
#    (flatcitybuf.generated.header_generated.Header/Vector) against the
#    real fixture bytes -- i.e. the same reference bindings read_header()
#    itself parses, not hand-derived arithmetic. Their rounded values
#    agree with the independent Rust CLI oracle above, which is the
#    cross-check.
#
# The CRS ("EPSG:7415") was NOT printed by `fcb info` (it has no CRS
# line) -- it was read the same way (generated bindings against the raw
# file: ReferenceSystem.Authority()=b"EPSG", Code()=7415), but it does
# have a second, independent oracle: conformance/small.expected.jsonl
# and examples/data/delft.city.jsonl both carry
# "referenceSystem":"https://www.opengis.net/def/crs/EPSG/0/7415".
#
# Column 0's exact fields (index=0, name="b3_bag_bag_overlap", type=10,
# nullable=True -- for BOTH fixtures, same schema) were read directly
# with the committed flatc-generated bindings
# (header_generated.Header.Columns(0)) against the raw fixture bytes,
# independently of flatcitybuf.header. Cross-checked against the Rust
# CLI oracle above: `fcb info` walks `header.attribute_index()` (which
# writer/mod.rs:190-192 sorts ascending by schema/column index before
# writing) and prints the matching column name first -- "1.
# b3_bag_bag_overlap" -- confirming column index 0's name independently
# of the Python bindings.
_COL0_INDEX = 0
_COL0_NAME = "b3_bag_bag_overlap"
_COL0_TYPE = 10
_COL0_NULLABLE = True

_SCALE = (0.001, 0.001, 0.001)
_TRANSLATE = (85088.390625, 446394.25, 45.64800262451172)
_EXTENT = (
    84501.5546875,
    445805.03125,
    -3.746997833251953,
    85675.234375,
    446983.46875,
    95.04200744628906,
)


def _read(path: Path) -> HeaderView:
    return read_header(FileRangeReader(path))


# --------------------------------------------------------- the trap ---


def test_attribute_index_struct_is_sixteen_bytes() -> None:
    # header.fbs:65-70 -- AttributeIndex has 4 fields (ushort, uint,
    # ushort, uint) but is 16 bytes, not 12: field order forces 2 bytes
    # of padding after each ushort. 0:u16 index, 2:pad, 4:u32 length,
    # 8:u16 branching_factor, 10:pad, 12:u32 num_unique_items.
    assert struct.calcsize("<HxxIHxxI") == 16


# ------------------------------------------------------- small.fcb ---


def test_small_features_count() -> None:
    view = _read(CORPUS / "small.fcb")
    assert view.info.features_count == 3


def test_small_version() -> None:
    view = _read(CORPUS / "small.fcb")
    assert view.info.cityjson_version == "2.0"


def test_small_title_and_crs() -> None:
    view = _read(CORPUS / "small.fcb")
    assert view.info.title == "3DBAG"
    assert view.info.crs == "EPSG:7415"


def test_small_transform() -> None:
    view = _read(CORPUS / "small.fcb")
    assert view.info.scale == _SCALE
    assert view.info.translate == _TRANSLATE


def test_small_geographical_extent() -> None:
    view = _read(CORPUS / "small.fcb")
    assert view.info.geographical_extent == _EXTENT


def test_small_column_count() -> None:
    view = _read(CORPUS / "small.fcb")
    assert len(view.info.columns) == 44
    assert len(view.info.semantic_columns) == 1


def test_small_column_shape() -> None:
    view = _read(CORPUS / "small.fcb")
    col = view.info.columns[0]
    assert isinstance(col, ColumnInfo)
    assert isinstance(col.index, int)
    assert isinstance(col.name, str)
    assert col.name != ""
    assert isinstance(col.type, int)
    assert isinstance(col.nullable, bool)


def test_small_column_zero_exact_values() -> None:
    # Pins the actual field values, not just their types: a swap of
    # index<->type (both plain ints) or a misread Nullable()/Name()
    # would still satisfy test_small_column_shape's isinstance-only
    # checks above but would fail here. See the module docstring for
    # the oracle used.
    view = _read(CORPUS / "small.fcb")
    col = view.info.columns[0]
    assert col.index == _COL0_INDEX
    assert col.name == _COL0_NAME
    assert col.type == _COL0_TYPE
    assert col.nullable == _COL0_NULLABLE


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_delft_column_zero_exact_values() -> None:
    # Same schema as small.fcb (both conformance fixtures share the
    # 44-column b3_* attribute schema); pinned separately in case the
    # fixtures ever diverge.
    view = _read(DELFT)
    col = view.info.columns[0]
    assert col.index == _COL0_INDEX
    assert col.name == _COL0_NAME
    assert col.type == _COL0_TYPE
    assert col.nullable == _COL0_NULLABLE


def test_small_attribute_indices() -> None:
    view = _read(CORPUS / "small.fcb")
    assert len(view.attr_indices) == 44
    first = view.attr_indices[0]
    assert isinstance(first, AttrIndexInfo)
    # column 0, from the generated-bindings read (see module docstring).
    assert first.column_index == 0
    assert first.length == 60
    assert first.branching_factor == 256
    assert first.num_unique_items == 1
    assert first.begin == view.layout.attr_index_begin


def test_small_attribute_index_begins_are_contiguous_and_sorted() -> None:
    view = _read(CORPUS / "small.fcb")
    indices = view.attr_indices
    assert [ai.column_index for ai in indices] == sorted(
        ai.column_index for ai in indices
    )
    for prev, cur in zip(indices, indices[1:]):
        assert cur.begin == prev.begin + prev.length
    total_len = sum(ai.length for ai in indices)
    assert indices[-1].begin + indices[-1].length == (
        view.layout.attr_index_begin + total_len
    )
    assert view.layout.feature_begin == (
        view.layout.attr_index_begin + total_len
    )


# ------------------------------------------------------- delft.fcb ---


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_delft_features_count() -> None:
    view = _read(DELFT)
    assert view.info.features_count == 1115


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_delft_version() -> None:
    view = _read(DELFT)
    assert view.info.cityjson_version == "2.0"


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_delft_transform() -> None:
    view = _read(DELFT)
    assert view.info.scale == _SCALE
    assert view.info.translate == _TRANSLATE


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_delft_geographical_extent() -> None:
    view = _read(DELFT)
    assert view.info.geographical_extent == _EXTENT


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_delft_column_count() -> None:
    # The task brief states delft.fcb's header declares 44 columns.
    view = _read(DELFT)
    assert len(view.info.columns) == 44


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_delft_layout_fits_inside_the_file() -> None:
    view = _read(DELFT)
    reader = FileRangeReader(DELFT)
    assert view.layout.feature_begin <= reader.total_size()
    assert view.layout.rtree_size > 0  # 1115 features, index_node_size 16


# ------------------------------------------------------- HeaderView ---


def test_header_view_shape() -> None:
    view = _read(CORPUS / "small.fcb")
    assert isinstance(view.info, FileInfo)
    assert view.layout.header_len == 8 + 4 + 2828
    assert view.layout.rtree_begin == view.layout.header_len
    assert view.layout.attr_index_begin == (
        view.layout.rtree_begin + view.layout.rtree_size
    )


# --------------------------------------------------- error handling ---


def test_read_header_rejects_bad_magic(tmp_path: Path) -> None:
    path = tmp_path / "bad_magic.fcb"
    path.write_bytes(b"xcb\x01fcb\x00" + b"\x00" * 100)
    with pytest.raises(FcbError) as exc_info:
        read_header(FileRangeReader(path))
    assert exc_info.value.code is ErrorCode.INVALID_MAGIC_BYTES


def test_read_header_rejects_a_header_size_below_the_minimum(
    tmp_path: Path,
) -> None:
    path = tmp_path / "tiny_header.fcb"
    # header_size=1 is below the 8-byte guard (const_vars.rs:8).
    path.write_bytes(b"fcb\x01fcb\x00" + struct.pack("<I", 1) + b"\x00")
    with pytest.raises(FcbError) as exc_info:
        read_header(FileRangeReader(path))
    assert exc_info.value.code is ErrorCode.ILLEGAL_HEADER_SIZE


def test_read_header_rejects_a_truncated_header(tmp_path: Path) -> None:
    path = tmp_path / "truncated.fcb"
    # header_size claims 100 bytes but only 8 magic + 4 size-field bytes
    # follow -- no actual header body.
    path.write_bytes(b"fcb\x01fcb\x00" + struct.pack("<I", 100))
    with pytest.raises(FcbError) as exc_info:
        read_header(FileRangeReader(path))
    assert exc_info.value.code is ErrorCode.ILLEGAL_HEADER_SIZE
