"""Pure-Python reader for FlatCityBuf.

FlatCityBuf holds CityJSON's semantics in FlatBuffers, alongside a
packed Hilbert R-tree for spatial queries and a static B+tree for
attribute queries, so a client can fetch only the bytes it needs. This
package parses those bytes directly -- no FFI and no compiled
extension, just a `py3-none-any` wheel on CPython 3.9+.

Public API re-exports only -- every name below is defined in a
submodule. The usual entry point is:

    import flatcitybuf as fcb

    reader = fcb.FcbReader.open_file("city.fcb")
    header = fcb.to_cityjson_metadata(reader.header)
    for feature in reader.select_all():
        cj = fcb.to_cityjson_feature(feature, reader.header)

## Query paths

`FcbReader.select_all` scans sequentially, in stored (Hilbert) order,
and yields `Feature` objects. The two INDEXED paths do not: both hand
back `SearchResultItem`, a feature-section-relative BYTE OFFSET rather
than a decoded feature, which `FcbReader.feature_at` turns into a
`Feature`.

* `search_rtree` answers a bounding box from the packed R-tree.
* `FcbReader.select_attr` answers `AttrCondition`s from the attribute
  B+trees; `search_stree` is the raw, unverified layer beneath it.

Bytes come from any `RangeReader`: `FileRangeReader` for a local file,
`HttpRangeReader` for a URL over HTTP range requests (synchronously,
on stdlib `urllib.request` -- there is no asyncio API here), and
`BufferedRangeReader` as a per-query caching decorator over either.

## Two things that surprise callers

**`u32::MAX` (4294967295) means NULL**, not the number 4294967295, in
a geometry's `semantics.values` and in the material/texture appearance
index arrays. The CityJSON that `to_cityjson_feature` emits already
maps it to JSON `null`; code reading those arrays by any other route
has to do so itself.

**String index keys are truncated to 50 bytes**, possibly
mid-codepoint, so distinct values sharing a prefix are
indistinguishable to the index. (The format truncates Json and Binary
columns at 100 bytes instead; this reader rejects both column types
outright -- see `search_stree`.) `search_stree` therefore returns
CANDIDATES for string columns, and
`FcbReader.select_attr` re-checks each one against the full,
untruncated attribute value before returning it -- pass
`exact_index_only=True` to skip that and take the raw candidates.

Anything not listed in `__all__` is internal and may change without
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
