from __future__ import annotations

from flatcitybuf.errors import ErrorCode, FcbError
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

__version__ = "0.3.0"

__all__ = [
    "ErrorCode",
    "FcbError",
    "MAGIC_SIZE",
    "NODE_ITEM_SIZE",
    "DEFAULT_NODE_SIZE",
    "MAX_FEATURE_SIZE",
    "FileLayout",
    "check_magic_bytes",
    "rtree_index_size",
    "compute_layout",
    "validate_layout_against_size",
]
