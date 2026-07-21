import pytest
from flatcitybuf.errors import ErrorCode, FcbError


def test_error_carries_its_code() -> None:
    err = FcbError(ErrorCode.INVALID_MAGIC_BYTES, "bad magic")
    assert err.code is ErrorCode.INVALID_MAGIC_BYTES
    assert "bad magic" in str(err)


def test_error_is_catchable_as_exception() -> None:
    with pytest.raises(FcbError):
        raise FcbError(ErrorCode.IO_ERROR, "boom")
