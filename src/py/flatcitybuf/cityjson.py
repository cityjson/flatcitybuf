from __future__ import annotations

import json
from typing import Any, Callable, Dict, List, Optional, Sequence

from flatcitybuf.attribute import decode_attributes
from flatcitybuf.errors import ErrorCode, FcbError
from flatcitybuf.errors import reraise_as_invalid_flatbuffer
from flatcitybuf.feature import Feature, raw_city_feature, raw_city_object
from flatcitybuf.generated.geometry_generated import GeometryType
from flatcitybuf.generated.header_generated import ColumnType
from flatcitybuf.generated.header_generated import TextureFormat
from flatcitybuf.generated.header_generated import Vector as _Vector
from flatcitybuf.geometry import decode_boundaries
from flatcitybuf.geometry import decode_material_values
from flatcitybuf.geometry import decode_texture_values
from flatcitybuf.geometry import geometry_type_name
from flatcitybuf.header import ColumnInfo, HeaderView

# u32::MAX marks "no semantic surface for this boundary" and becomes
# JSON null (geom_decoder.rs:284, cityjson.cpp:54-59).
_U32_MAX = 0xFFFFFFFF

# Names must match CityJSON exactly; the enum order comes from
# feature.fbs's CityObjectType declaration (cityjson.cpp:24-37).
_CITY_OBJECT_TYPE_NAMES = [
    "Bridge",
    "BridgePart",
    "BridgeInstallation",
    "BridgeConstructiveElement",
    "BridgeRoom",
    "BridgeFurniture",
    "Building",
    "BuildingPart",
    "BuildingInstallation",
    "BuildingConstructiveElement",
    "BuildingFurniture",
    "BuildingStorey",
    "BuildingRoom",
    "BuildingUnit",
    "CityFurniture",
    "CityObjectGroup",
    "GenericCityObject",
    "LandUse",
    "OtherConstruction",
    "PlantCover",
    "SolitaryVegetationObject",
    "TINRelief",
    "Road",
    "Railway",
    "Waterway",
    "TransportSquare",
    "Tunnel",
    "TunnelPart",
    "TunnelInstallation",
    "TunnelConstructiveElement",
    "TunnelHollowSpace",
    "TunnelFurniture",
    "WaterBody",
    "ExtensionObject",
]

# geometry.fbs's SemanticSurfaceType declaration (cityjson.cpp:44-52).
_SEMANTIC_SURFACE_TYPE_NAMES = [
    "RoofSurface",
    "GroundSurface",
    "WallSurface",
    "ClosureSurface",
    "OuterCeilingSurface",
    "OuterFloorSurface",
    "Window",
    "Door",
    "InteriorWallSurface",
    "CeilingSurface",
    "FloorSurface",
    "WaterSurface",
    "WaterGroundSurface",
    "WaterClosureSurface",
    "TrafficArea",
    "AuxiliaryTrafficArea",
    "TransportationMarking",
    "TransportationHole",
    "ExtraSemanticSurface",
]

# header.fbs's WrapMode/TextureType declarations. CityJSON spells these
# lower case; the reference falls back to the first enumerator for an
# unrecognised value rather than failing (cityjson.cpp:155-172).
_WRAP_MODE_NAMES = {
    0: "none",
    1: "wrap",
    2: "mirror",
    3: "clamp",
    4: "border",
}
_TEXTURE_TYPE_NAMES = {0: "unknown", 1: "specific", 2: "typical"}

# SemanticObject { type, attributes, children, parent, extension_type }
# (geometry.fbs) -- field 1 (attributes) sits at vtable slot
# (1 + 2) * 2 = 6. Read directly rather than through the generated
# per-element Attributes(j) accessor, for the same reason feature.py's
# _read_attribute_bytes reaches into CityObject.attributes: an O(n)
# Python-level call per byte, and no numpy at module scope allowed.
_SEMANTIC_ATTRIBUTES_VTABLE_OFFSET = 6


def city_object_type_name(type_: int) -> str:
    """cityjson.cpp:339-347. Raises on any ubyte value outside the
    CityObjectType enum (feature.fbs)."""
    if not 0 <= type_ < len(_CITY_OBJECT_TYPE_NAMES):
        raise FcbError(
            ErrorCode.INVALID_FLATBUFFER,
            f"unknown city object type {type_}",
        )
    return _CITY_OBJECT_TYPE_NAMES[type_]


def _decode_str(b: bytes) -> str:
    # Ordinary length-prefixed FlatBuffers strings throughout this
    # module (theme/name/image/lod/id/...), never the fixed-width
    # B+tree keys gotcha 4 warns about.
    return b.decode("utf-8", errors="replace")


def _uint_list(
    length: int,
    get: Callable[[int], int],
    as_numpy: Optional[Callable[[], Any]] = None,
) -> List[int]:
    """Materialises a generated `length`/`get(j)` accessor pair (e.g.
    Geometry.SolidsLength/Solids) into a plain Python list, the input
    shape geometry.py's decoders expect. Equivalent to cityjson.cpp's
    as_uint_view, minus the zero-copy view (Python has no borrowed-slice
    equivalent over a FlatBuffers vector worth the ceremony here).

    `as_numpy` is the field's generated `XxxAsNumpy` accessor (e.g.
    `Geometry.SolidsAsNumpy`) -- flatbuffers-python already generates
    one for every scalar-vector field, wrapping `numpy.frombuffer` over
    the raw vector bytes (flatbuffers/encode.py). Profiling a full scan
    of delft.fcb (Task 12's benchmark) found this per-element `get(j)`
    loop to be the dominant cost -- millions of individual FlatBuffers
    table reads for a handful of large boundaries/solids/shells arrays
    per feature. When numpy is importable, this decodes the whole
    vector in ONE bulk call instead; when it is not, or the vector is
    empty (where the generated accessor returns a bare `0`, not an
    array), it falls back to the per-element loop unchanged. The two
    paths are asserted to agree in test_cityjson.py -- see the task
    report.
    """
    if length == 0:
        return []
    if as_numpy is not None:
        try:
            import numpy as np
        except ImportError:
            np = None  # type: ignore[assignment]
        if np is not None:
            arr = as_numpy()
            if isinstance(arr, np.ndarray):
                result: List[int] = arr.tolist()
                return result
    return [get(j) for j in range(length)]


def _decode_attributes_for_json(
    blob: bytes, schema: Sequence[ColumnInfo]
) -> Dict[str, Any]:
    """decode_attributes plus the one thing it deliberately leaves for
    CityJSON emission (its own docstring): a `Json`-typed column's text
    is re-parsed so it nests as real JSON, rather than surviving as a
    string. Mirrors the Rust reader's serde_json::from_str
    (deserializer.rs:363-369) and cityjson.cpp's nlohmann::json::parse
    (attribute.cpp:179-183).

    Both reference readers assume the embedded text IS valid JSON (Rust
    unwraps, C++ passes allow_exceptions=false but then still asserts on
    the writer having produced valid text); this raises rather than
    silently emitting a string or a null on malformed input, since a
    reader must not paper over a corrupt file.
    """
    values = decode_attributes(blob, schema)
    json_columns = {c.name for c in schema if c.type == ColumnType.Json}
    if not json_columns:
        return values
    for name in json_columns:
        raw = values.get(name)
        if isinstance(raw, str):
            try:
                values[name] = json.loads(raw)
            except ValueError as exc:
                raise FcbError(
                    ErrorCode.INVALID_ATTRIBUTE_VALUE,
                    f"column '{name}' has type Json but its stored "
                    "text is not valid JSON",
                ) from exc
    return values


def _read_semantic_attribute_bytes(so: Any) -> bytes:
    """Raw bytes of SemanticObject.attributes. See the module-level
    _SEMANTIC_ATTRIBUTES_VTABLE_OFFSET comment for why this bypasses
    the generated per-element accessor, and feature.py's
    _read_attribute_bytes for the identical pattern applied to
    CityObject.attributes."""
    o = so._tab.Offset(_SEMANTIC_ATTRIBUTES_VTABLE_OFFSET)
    if o == 0:
        return b""
    length = so._tab.VectorLen(o)
    if length != so.AttributesLength():
        raise FcbError(
            ErrorCode.INVALID_FLATBUFFER,
            "SemanticObject.attributes vtable slot mismatch: "
            f"hand-counted offset {_SEMANTIC_ATTRIBUTES_VTABLE_OFFSET} "
            "disagrees with the generated accessor -- geometry.fbs "
            "field order may have changed",
        )
    start = so._tab.Vector(o)
    return bytes(so._tab.Bytes[start : start + length])


def _color_to_json(
    is_none: bool, length: int, get: Callable[[int], float]
) -> Optional[List[float]]:
    """A [double] colour vector as a JSON array, or None when the field
    is absent. Mirrors cityjson.cpp's color_to_json (cityjson.cpp:106-
    117): the reference asserts the length (3 for materials, 4 for
    border colours) and PANICS otherwise, but a reader must not abort
    on a malformed file, so a wrong length is emitted as-is here too."""
    if is_none:
        return None
    return [get(j) for j in range(length)]


def _decode_semantics_values(
    geom_type: int,
    solids: Sequence[int],
    shells: Sequence[int],
    values: Sequence[int],
) -> List[Any]:
    """Nesting depth of `semantics.values`, chosen by geometry type:
    solids nest twice, Solid once, everything else not at all. The
    values array is sliced by the shell/solid counts. Mirrors
    cityjson.cpp's decode_semantics_values (cityjson.cpp:61-104)."""
    two_deep = geom_type in (
        GeometryType.MultiSolid,
        GeometryType.CompositeSolid,
    )
    one_deep = geom_type == GeometryType.Solid

    if not two_deep and not one_deep:
        return [None if v == _U32_MAX else v for v in values]

    cursor = 0
    per_shell: List[Any] = []
    for n in shells:
        grp: List[Any] = []
        k = 0
        while k < n and cursor < len(values):
            v = values[cursor]
            grp.append(None if v == _U32_MAX else v)
            cursor += 1
            k += 1
        per_shell.append(grp)

    if one_deep:
        return per_shell

    # MultiSolid/CompositeSolid add one more level, grouped by solid.
    shell_cursor = 0
    per_solid: List[Any] = []
    for count in solids:
        grp = []
        k = 0
        while k < count and shell_cursor < len(per_shell):
            grp.append(per_shell[shell_cursor])
            shell_cursor += 1
            k += 1
        per_solid.append(grp)
    return per_solid


def _materials_to_json(
    length: int, get: Callable[[int], Any]
) -> Dict[str, Any]:
    """`material`: theme -> either a single shared-material index or a
    nested values array. Mirrors cityjson.cpp's materials_to_json
    (cityjson.cpp:209-225).

    `MaterialMapping.value` is `uint = null` (geometry.fbs) -- an
    OPTIONAL scalar. The generated Python accessor returns None when
    the field is absent and the real value (including 0) when present,
    which is what lets this tell "shared material 0" apart from "no
    shared value"; test_cityjson.py pins this against a real encoded
    file rather than trusting it silently (task brief gotcha #1).
    """
    out: Dict[str, Any] = {}
    for j in range(length):
        m = get(j)
        if m is None:
            continue
        theme_bytes = m.Theme()
        theme = (
            _decode_str(theme_bytes) if theme_bytes is not None else "theme"
        )

        value = m.Value()
        if value is not None:
            out[theme] = {"value": value}
            continue

        if m.VerticesIsNone():
            continue
        solids = _uint_list(m.SolidsLength(), m.Solids, m.SolidsAsNumpy)
        shells = _uint_list(m.ShellsLength(), m.Shells, m.ShellsAsNumpy)
        vertices = _uint_list(
            m.VerticesLength(), m.Vertices, m.VerticesAsNumpy
        )
        out[theme] = {
            "values": decode_material_values(solids, shells, vertices)
        }
    return out


def _textures_to_json(
    length: int, get: Callable[[int], Any]
) -> Dict[str, Any]:
    """`texture`: theme -> nested values array. Mappings without
    vertices are skipped; mirrors cityjson.cpp's textures_to_json
    (cityjson.cpp:229-243)."""
    out: Dict[str, Any] = {}
    for j in range(length):
        m = get(j)
        if m is None:
            continue
        vertices = _uint_list(
            m.VerticesLength(), m.Vertices, m.VerticesAsNumpy
        )
        if not vertices:
            continue
        theme_bytes = m.Theme()
        theme = (
            _decode_str(theme_bytes) if theme_bytes is not None else "theme"
        )
        solids = _uint_list(m.SolidsLength(), m.Solids, m.SolidsAsNumpy)
        shells = _uint_list(m.ShellsLength(), m.Shells, m.ShellsAsNumpy)
        surfaces = _uint_list(
            m.SurfacesLength(), m.Surfaces, m.SurfacesAsNumpy
        )
        strings = _uint_list(m.StringsLength(), m.Strings, m.StringsAsNumpy)
        out[theme] = {
            "values": decode_texture_values(
                solids, shells, surfaces, strings, vertices
            )
        }
    return out


def _appearance_to_json(a: Any) -> Dict[str, Any]:
    """The `appearance` object a CityJSONFeature (or the header) carries:
    the materials and textures its geometry mappings index into, plus
    the UV vertices those textures address. Mirrors cityjson.cpp's
    appearance_to_json (cityjson.cpp:122-202).

    Every list-valued key here is emitted whenever the corresponding
    FlatBuffers vector is PRESENT, even if it is empty (checked via
    *IsNone(), not length) -- an empty `materials: []` still means "this
    file declares a materials array with zero entries", which differs
    from "this file has no materials array at all".
    """
    out: Dict[str, Any] = {}

    if not a.MaterialsIsNone():
        materials = []
        for j in range(a.MaterialsLength()):
            m = a.Materials(j)
            if m is None:
                continue
            entry: Dict[str, Any] = {}
            name = m.Name()
            entry["name"] = _decode_str(name) if name is not None else ""
            # Every other field is optional (`= null`) and omitted when
            # unset -- mirrors serde's skip_serializing_if on the Rust
            # side, and the same vtable-presence trap as
            # MaterialMapping.value applies to each of these.
            v = m.AmbientIntensity()
            if v is not None:
                entry["ambientIntensity"] = v
            c = _color_to_json(
                m.DiffuseColorIsNone(), m.DiffuseColorLength(), m.DiffuseColor
            )
            if c is not None:
                entry["diffuseColor"] = c
            c = _color_to_json(
                m.EmissiveColorIsNone(),
                m.EmissiveColorLength(),
                m.EmissiveColor,
            )
            if c is not None:
                entry["emissiveColor"] = c
            c = _color_to_json(
                m.SpecularColorIsNone(),
                m.SpecularColorLength(),
                m.SpecularColor,
            )
            if c is not None:
                entry["specularColor"] = c
            v = m.Shininess()
            if v is not None:
                entry["shininess"] = v
            v = m.Transparency()
            if v is not None:
                entry["transparency"] = v
            v = m.IsSmooth()
            if v is not None:
                entry["isSmooth"] = v
            materials.append(entry)
        out["materials"] = materials

    if not a.TexturesIsNone():
        textures = []
        for j in range(a.TexturesLength()):
            t = a.Textures(j)
            if t is None:
                continue
            entry = {}
            entry["type"] = "JPG" if t.Type() == TextureFormat.JPG else "PNG"
            image = t.Image()
            entry["image"] = _decode_str(image) if image is not None else ""
            w = t.WrapMode()
            if w is not None:
                entry["wrapMode"] = _WRAP_MODE_NAMES.get(w, "none")
            tt = t.TextureType()
            if tt is not None:
                entry["textureType"] = _TEXTURE_TYPE_NAMES.get(tt, "unknown")
            c = _color_to_json(
                t.BorderColorIsNone(), t.BorderColorLength(), t.BorderColor
            )
            if c is not None:
                entry["borderColor"] = c
            textures.append(entry)
        out["textures"] = textures

    if not a.VerticesTextureIsNone():
        uvs = []
        for j in range(a.VerticesTextureLength()):
            v2 = a.VerticesTexture(j)
            uvs.append([v2.U(), v2.V()])
        out["vertices-texture"] = uvs

    default_theme_texture = a.DefaultThemeTexture()
    if default_theme_texture is not None:
        out["default-theme-texture"] = _decode_str(default_theme_texture)
    default_theme_material = a.DefaultThemeMaterial()
    if default_theme_material is not None:
        out["default-theme-material"] = _decode_str(default_theme_material)

    return out


def _geometry_instance_to_json(gi: Any) -> Dict[str, Any]:
    """Mirrors cityjson.cpp's geometry_instance_to_json
    (cityjson.cpp:245-271)."""
    out: Dict[str, Any] = {
        "type": "GeometryInstance",
        "template": gi.Template(),
        # The boundaries array holds exactly one vertex index: CityGML's
        # "referencePoint" for the instance.
        "boundaries": _uint_list(
            gi.BoundariesLength(), gi.Boundaries, gi.BoundariesAsNumpy
        ),
    }

    tm = gi.Transformation()
    if tm is not None:
        # 16 doubles in row-major order.
        out["transformationMatrix"] = [
            tm.M00(),
            tm.M01(),
            tm.M02(),
            tm.M03(),
            tm.M10(),
            tm.M11(),
            tm.M12(),
            tm.M13(),
            tm.M20(),
            tm.M21(),
            tm.M22(),
            tm.M23(),
            tm.M30(),
            tm.M31(),
            tm.M32(),
            tm.M33(),
        ]
    return out


def _geometry_to_json(
    g: Any, semantic_columns: Sequence[ColumnInfo]
) -> Dict[str, Any]:
    """Mirrors cityjson.cpp's geometry_to_json (cityjson.cpp:273-335).

    Semantic surfaces carry their own attributes, decoded against
    `semantic_columns` -- a schema separate from the feature attribute
    columns -- and merged inline, as the reference does.

    An EMPTY material/texture mapping vector omits the key entirely:
    checked via *Length() > 0, matching the reference's `size() > 0`
    guard (cityjson.cpp:327-332). Only a vector whose mappings were all
    skipped (inside _materials_to_json/_textures_to_json) yields `{}`.
    """
    out: Dict[str, Any] = {"type": geometry_type_name(g.Type())}
    lod = g.Lod()
    if lod is not None:
        out["lod"] = _decode_str(lod)

    solids = _uint_list(g.SolidsLength(), g.Solids, g.SolidsAsNumpy)
    shells = _uint_list(g.ShellsLength(), g.Shells, g.ShellsAsNumpy)
    surfaces = _uint_list(g.SurfacesLength(), g.Surfaces, g.SurfacesAsNumpy)
    strings = _uint_list(g.StringsLength(), g.Strings, g.StringsAsNumpy)
    boundaries = _uint_list(
        g.BoundariesLength(), g.Boundaries, g.BoundariesAsNumpy
    )

    out["boundaries"] = decode_boundaries(
        solids, shells, surfaces, strings, boundaries
    )

    if g.SemanticsObjectsLength() > 0:
        surfaces_json = []
        for j in range(g.SemanticsObjectsLength()):
            so = g.SemanticsObjects(j)
            if so is None:
                continue
            s: Dict[str, Any] = {}
            ext = so.ExtensionType()
            t = so.Type()
            if ext is not None:
                s["type"] = _decode_str(ext)
            elif 0 <= t < len(_SEMANTIC_SURFACE_TYPE_NAMES):
                s["type"] = _SEMANTIC_SURFACE_TYPE_NAMES[t]
            else:
                s["type"] = "ExtraSemanticSurface"

            # SemanticObject.parent is `uint = null` -- the same
            # optional-scalar shape as MaterialMapping.value, and the
            # Rust reference reader DOES emit it (pinned against a
            # round trip through the Rust writer in test_cityjson.py,
            # since no committed corpus fixture happens to set it).
            # src/cpp/src/cityjson.cpp does not emit this field at all;
            # see the task report for why Python diverges from it here.
            parent = so.Parent()
            if parent is not None:
                s["parent"] = parent

            if so.AttributesLength() > 0:
                blob = _read_semantic_attribute_bytes(so)
                s.update(_decode_attributes_for_json(blob, semantic_columns))

            if so.ChildrenLength() > 0:
                s["children"] = _uint_list(
                    so.ChildrenLength(), so.Children, so.ChildrenAsNumpy
                )

            surfaces_json.append(s)

        out["semantics"] = {
            "surfaces": surfaces_json,
            "values": _decode_semantics_values(
                g.Type(),
                solids,
                shells,
                _uint_list(
                    g.SemanticsLength(), g.Semantics, g.SemanticsAsNumpy
                ),
            ),
        }

    # Appearance: per-geometry mappings only. The header's `appearance`
    # object (the materials/textures/vertices-texture arrays these
    # index into) is deliberately not emitted here -- the Rust reader
    # does not emit it either, and CityJSONSeq consumers read it from
    # the source file. The per-FEATURE appearance (materials, textures,
    # vertices-texture) is a different thing and IS emitted, by
    # to_cityjson_feature below.
    if g.MaterialLength() > 0:
        out["material"] = _materials_to_json(g.MaterialLength(), g.Material)
    if g.TextureLength() > 0:
        out["texture"] = _textures_to_json(g.TextureLength(), g.Texture)

    return out


def _poc_address(hdr: Any) -> Optional[Dict[str, Any]]:
    """`Header.poc_address_*` -> CityJSON `pointOfContact.address`.
    Mirrors deserializer.rs's to_cj_address: ALL of thoroughfare number
    (parsed as an integer from its string field)/name, locality,
    postal code and country must be present, or the whole `address`
    object is omitted -- not emitted partially. There is no C++
    equivalent to port from; see the task report."""
    number_bytes = hdr.PocAddressThoroughfareNumber()
    if number_bytes is None:
        return None
    try:
        number = int(_decode_str(number_bytes))
    except ValueError:
        return None
    name = hdr.PocAddressThoroughfareName()
    locality = hdr.PocAddressLocality()
    postcode = hdr.PocAddressPostcode()
    country = hdr.PocAddressCountry()
    if None in (name, locality, postcode, country):
        return None
    return {
        "thoroughfareNumber": number,
        "thoroughfareName": _decode_str(name),
        "locality": _decode_str(locality),
        "postalCode": _decode_str(postcode),
        "country": _decode_str(country),
    }


def _point_of_contact(hdr: Any) -> Optional[Dict[str, Any]]:
    """`Header.poc_*` -> CityJSON `metadata.pointOfContact`. Mirrors
    deserializer.rs's to_cj_point_of_contact: emitted only when
    `poc_contact_name` is present (deserializer.rs:78-81); `poc_email`
    is then REQUIRED, matching the Rust reference reader's own
    Error::MissingRequiredField rather than silently dropping the
    object. There is no C++ equivalent to port from; see the task
    report."""
    name = hdr.PocContactName()
    if name is None:
        return None
    email = hdr.PocEmail()
    if email is None:
        raise FcbError(
            ErrorCode.MISSING_REQUIRED_FIELD,
            "pointOfContact.emailAddress is required once "
            "pointOfContact.contactName is present",
        )
    poc: Dict[str, Any] = {
        "contactName": _decode_str(name),
        "emailAddress": _decode_str(email),
    }
    contact_type = hdr.PocContactType()
    if contact_type is not None:
        poc["contactType"] = _decode_str(contact_type)
    role = hdr.PocRole()
    if role is not None:
        poc["role"] = _decode_str(role)
    phone = hdr.PocPhone()
    if phone is not None:
        poc["phone"] = _decode_str(phone)
    website = hdr.PocWebsite()
    if website is not None:
        poc["website"] = _decode_str(website)
    address = _poc_address(hdr)
    if address is not None:
        poc["address"] = address
    return poc


def to_cityjson_metadata(header: HeaderView) -> Dict[str, Any]:
    """The CityJSONSeq header line: `type`/`version`/`transform`/
    `metadata`, plus geometry templates when the header declares both a
    templates array and a templates-vertices array. Mirrors
    cityjson.cpp's to_cityjson_metadata (cityjson.cpp:349-406) for
    everything except `pointOfContact`/`referenceDate` -- see the task
    report for why those two are read here even though cityjson.cpp
    does not emit them at all (a real gap found by comparing against
    delft.fcb through the Rust reader, the true oracle).

    A CityJSONSeq header line carries no features of its own, hence the
    empty `CityObjects`/`vertices`.
    """
    # Codex review (Task 12): reader.py's _parse_feature and
    # header.py's read_header wrap the FlatBuffers accessors they call
    # directly, but the deep field-by-field traversal this function and
    # to_cityjson_feature do -- geometry, semantics, appearance,
    # templates -- was not covered, so a corruption several levels deep
    # (e.g. a hostile templates_vertices count) could leak a bare
    # IndexError/struct.error/AttributeError past the public surface
    # instead of an FcbError. See reraise_as_invalid_flatbuffer's
    # docstring.
    with reraise_as_invalid_flatbuffer(
        "failed to build CityJSON metadata from the header"
    ):
        return _to_cityjson_metadata_impl(header)


def _to_cityjson_metadata_impl(header: HeaderView) -> Dict[str, Any]:
    info = header.info
    hdr = header._raw
    cj: Dict[str, Any] = {"type": "CityJSON", "version": info.cityjson_version}

    if info.scale is not None and info.translate is not None:
        cj["transform"] = {
            "scale": list(info.scale),
            "translate": list(info.translate),
        }

    meta: Dict[str, Any] = {}
    if info.geographical_extent is not None:
        meta["geographicalExtent"] = list(info.geographical_extent)
    if info.identifier:
        meta["identifier"] = info.identifier
    if hdr is not None:
        poc = _point_of_contact(hdr)
        if poc is not None:
            meta["pointOfContact"] = poc
        ref_date = hdr.ReferenceDate()
        if ref_date is not None:
            meta["referenceDate"] = _decode_str(ref_date)
    if info.crs:
        authority, _, code = info.crs.partition(":")
        meta["referenceSystem"] = (
            f"https://www.opengis.net/def/crs/{authority}/0/{code}"
        )
    if info.title:
        meta["title"] = info.title
    if meta:
        cj["metadata"] = meta

    cj["CityObjects"] = {}
    cj["vertices"] = []

    # Geometry templates: shapes shared by every GeometryInstance in the
    # file, with their own vertex list. Emitted only when BOTH arrays
    # are present -- a template without vertices indexes nothing.
    if (
        hdr is not None
        and not hdr.TemplatesIsNone()
        and not hdr.TemplatesVerticesIsNone()
    ):
        templates = [
            _geometry_to_json(t, info.semantic_columns)
            for j in range(hdr.TemplatesLength())
            if (t := hdr.Templates(j)) is not None
        ]

        # Template vertices are absolute doubles, NOT quantised: the
        # header transform does not apply to them.
        verts = []
        for j in range(hdr.TemplatesVerticesLength()):
            v = hdr.TemplatesVertices(j)
            verts.append([v.X(), v.Y(), v.Z()])

        cj["geometry-templates"] = {
            "templates": templates,
            "vertices-templates": verts,
        }

    return cj


def to_cityjson_feature(
    feature: Feature, header: HeaderView
) -> Dict[str, Any]:
    """One CityJSONFeature. Mirrors cityjson.cpp's to_cityjson_feature
    (cityjson.cpp:408-503).

    Wrapped in reraise_as_invalid_flatbuffer for the same reason
    to_cityjson_metadata is -- see its docstring (Codex review, Task
    12).
    """
    with reraise_as_invalid_flatbuffer(
        f"failed to build CityJSON for feature {feature.id!r}"
    ):
        return _to_cityjson_feature_impl(feature, header)


def _to_cityjson_feature_impl(
    feature: Feature, header: HeaderView
) -> Dict[str, Any]:
    cf = raw_city_feature(feature)
    if cf is None:
        raise FcbError(ErrorCode.MISSING_REQUIRED_FIELD, "empty feature")

    out: Dict[str, Any] = {"type": "CityJSONFeature", "id": feature.id}

    objects: Dict[str, Any] = {}
    for view in feature.city_objects():
        obj = raw_city_object(view)
        co: Dict[str, Any] = {
            "type": (
                view.extension_type
                if view.extension_type is not None
                else city_object_type_name(view.type)
            )
        }

        # Per-object schema when declared, header schema otherwise.
        # Emitted iff the object DECLARES an attributes vector -- a
        # present-but-empty one becomes `{}`, an absent one is omitted
        # entirely (feature.py's CityObjectView.has_attributes).
        if view.has_attributes:
            schema: Sequence[ColumnInfo] = header.info.columns
            if view.has_columns:
                # has_columns being True is exactly the invariant
                # (feature.py's CityObjectView) under which .columns is
                # populated rather than None; mypy cannot see that
                # relationship across two fields, hence the assert.
                assert view.columns is not None
                schema = view.columns
            blob = view.attributes or b""
            co["attributes"] = (
                {} if not blob else _decode_attributes_for_json(blob, schema)
            )

        extent = obj.GeographicalExtent()
        if extent is not None:
            # GeographicalExtent = { Vector min; Vector max; } (header.fbs)
            # is a plain struct: Min()/Max() take a pre-allocated Vector
            # to Init() in place, like Transform.Scale()/Translate() in
            # header.py, rather than returning one.
            mn = extent.Min(_Vector())
            mx = extent.Max(_Vector())
            co["geographicalExtent"] = [
                mn.X(),
                mn.Y(),
                mn.Z(),
                mx.X(),
                mx.Y(),
                mx.Z(),
            ]

        geoms = []
        for j in range(obj.GeometryLength()):
            g = obj.Geometry(j)
            if g is not None:
                geoms.append(
                    _geometry_to_json(g, header.info.semantic_columns)
                )
        for j in range(obj.GeometryInstancesLength()):
            gi = obj.GeometryInstances(j)
            if gi is not None:
                geoms.append(_geometry_instance_to_json(gi))
        if geoms:
            co["geometry"] = geoms

        if obj.ChildrenLength() > 0:
            co["children"] = [
                _decode_str(obj.Children(j))
                for j in range(obj.ChildrenLength())
            ]
        if obj.ParentsLength() > 0:
            co["parents"] = [
                _decode_str(obj.Parents(j)) for j in range(obj.ParentsLength())
            ]

        objects[view.id] = co
    out["CityObjects"] = objects

    # Vertices are quantised integers; the header transform maps them
    # back to world coordinates, so they stay integral here.
    out["vertices"] = [list(v) for v in feature.vertices()]

    # The materials, textures and UV vertices this feature's geometry
    # mappings index into. Without it the mappings reference nothing a
    # consumer can resolve. Appearance is a single nested table (not a
    # vector), so flatc generates no AppearanceIsNone() here -- unlike
    # Header.templates()/templates_vertices() above, `is not None` is
    # the only presence check available (same as GeographicalExtent()
    # below and Header.Transform() in header.py).
    appearance = cf.Appearance()
    if appearance is not None:
        out["appearance"] = _appearance_to_json(appearance)

    return out
