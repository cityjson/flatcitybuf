from __future__ import annotations

from pathlib import Path

import flatcitybuf.generated as gen

CORPUS = Path(__file__).resolve().parents[3] / "conformance"


def test_can_read_a_real_header_as_a_size_prefixed_root() -> None:
    """Pins HOW this runtime exposes size-prefixed roots. Do not assume.

    ANSWER (flatc 25.9.23 + flatbuffers pip runtime 25.12.19): there is no
    `GetSizePrefixedRootAs` here at all -- flatc only ever generates
    `GetRootAs(cls, buf, offset=0)` per table (see
    flatcitybuf/generated/header_generated.py, class Header). That method
    reads a uoffset starting AT `offset` in `buf` and inits the table at
    `offset + uoffset`. A size-prefixed buffer is just a 4-byte little-
    endian uint32 root-table-size prepended before the normal root offset,
    so passing `offset=4` to the *same* `GetRootAs` -- with the 4-byte
    prefix left in `buf` -- IS how you read a size-prefixed root with this
    runtime. (`flatbuffers.util.GetSizePrefix(buf, offset)` exists only to
    read the length value itself, e.g. for streaming/chunked I/O; it does
    not construct the root.)
    """
    raw = (CORPUS / "small.fcb").read_bytes()
    header_size = int.from_bytes(raw[8:12], "little")
    # prefix INCLUDED, per the format reference
    buf = raw[8 : 12 + header_size]
    # confirmed correct -- see ANSWER above. Generated code is untyped, so
    # this call needs an explicit ignore under mypy strict.
    header = gen.Header.GetRootAs(buf, 4)  # type: ignore[no-untyped-call]
    assert header.FeaturesCount() > 0
    assert header.Version().decode() == "2.0"
