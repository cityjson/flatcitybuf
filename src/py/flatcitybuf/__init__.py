"""Pure-Python reader for FlatCityBuf.

Public API re-exports only -- every name below is defined in a
submodule. The usual entry point is::

    import flatcitybuf as fcb

    reader = fcb.FcbReader.open_file("city.fcb")
    header = fcb.to_cityjson_metadata(reader.header)
    for feature in reader.select_all():
        cj = fcb.to_cityjson_feature(feature, reader.header)

Anything not listed in ``__all__`` is internal and may change without
notice; importing it from its submodule is at your own risk.
"""

from __future__ import annotations

from importlib.metadata import PackageNotFoundError, version

from flatcitybuf.attribute import decode_attributes
from flatcitybuf.cityjson import (
    city_object_type_name,
    to_cityjson_feature,
    to_cityjson_metadata,
)
from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.feature import CityObjectView, Feature
from flatcitybuf.header import (
    AttrIndexInfo,
    ColumnInfo,
    FileInfo,
    HeaderView,
    read_header,
)
from flatcitybuf.http_reader import HttpRangeReader
from flatcitybuf.keys import KeyKind, KeyValue
from flatcitybuf.layout import (
    DEFAULT_NODE_SIZE,
    MAGIC_SIZE,
    MAX_FEATURE_SIZE,
    NODE_ITEM_SIZE,
    FileLayout,
    check_magic_bytes,
    compute_layout,
    rtree_index_size,
    validate_layout_against_size,
)
from flatcitybuf.packed_rtree import NodeItem, SearchResultItem, search_rtree
from flatcitybuf.range_reader import (
    BufferedRangeReader,
    FileRangeReader,
    RangeReader,
)
from flatcitybuf.reader import FcbReader
from flatcitybuf.stree import AttrCondition, Operator, search_stree

try:
    # Single source of truth: the version in pyproject.toml, which is
    # what the publish workflow rewrites (.github/workflows/
    # publish-python.yml). Hardcoding it here would silently drift from
    # the released version on every bump.
    __version__ = version("flatcitybuf")
except PackageNotFoundError:  # pragma: no cover
    # Running straight out of a source checkout (PYTHONPATH, not an
    # install): there is no distribution metadata to read.
    __version__ = "0.0.0+unknown"

__all__ = [
    # Reading
    "FcbReader",
    "Feature",
    "CityObjectView",
    "HeaderView",
    "FileInfo",
    "ColumnInfo",
    "AttrIndexInfo",
    "read_header",
    "decode_attributes",
    # CityJSON emission
    "to_cityjson_metadata",
    "to_cityjson_feature",
    "city_object_type_name",
    # Queries
    "search_rtree",
    "search_stree",
    "AttrCondition",
    "Operator",
    "KeyKind",
    "KeyValue",
    "SearchResultItem",
    "NodeItem",
    # Byte sources
    "RangeReader",
    "FileRangeReader",
    "BufferedRangeReader",
    "HttpRangeReader",
    # Errors
    "ErrorCode",
    "FcbError",
    # Layout
    "MAGIC_SIZE",
    "NODE_ITEM_SIZE",
    "DEFAULT_NODE_SIZE",
    "MAX_FEATURE_SIZE",
    "FileLayout",
    "check_magic_bytes",
    "rtree_index_size",
    "compute_layout",
    "validate_layout_against_size",
    "__version__",
]
