"""Walking the FlatBuffers geometry directly, for analysis.

    python examples/geometry_analysis.py in.fcb [count] [lod]

Every other example converts to CityJSON first. This one does not: it
reads the format's OWN representation -- five flat count arrays plus a
flat vertex-index list -- and computes over them. That is the
representation to use for analysis, because nothing has to be nested,
allocated, or turned into JSON to get a number out of it.

The arrays, per Geometry:

    Solids(i)      shell count of solid i
    Shells(i)      surface count of shell i
    Surfaces(i)    ring count of surface i
    Strings(i)     vertex count of ring i
    Boundaries     the flat vertex-index list
    Semantics(i)   semantic-object index of surface i (u32::MAX = none)

THE NESTING DEPTH COMES FROM Geometry.Type(), NEVER FROM THE ARRAYS. A
Solid with one shell and a MultiSolid with one solid flatten to
byte-identical arrays -- only the type tells them apart. Inferring depth
from which array is populated is upstream finding #8. This example never
needs the depth: surface areas sum the same however the surfaces are
grouped, so it walks Surfaces/Strings straight through. Anything that
DOES care about grouping (per-shell volume, say) must switch on the type.

Vertices are quantised integers shared by the whole feature: multiply by
transform.scale and add transform.translate for real-world coordinates.
For area the translate cancels, but the scale does not.

The flat walk is checked twice: against to_cityjson_feature's nested
output, and against the dataset's own published ground area.
"""

from __future__ import annotations

import math
import sys
import time
from typing import Any, Dict, List, Sequence, Tuple

import flatcitybuf as fcb

_U32_MAX = 0xFFFFFFFF


def ring_area(pts: Sequence[Tuple[float, float, float]]) -> float:
    """Area of one planar polygon in 3D, by Newell's method: the
    magnitude of the summed edge cross-products, halved. Works for any
    simple polygon and needs no projection or triangulation."""
    nx = ny = nz = 0.0
    n = len(pts)
    for i in range(n):
        x0, y0, z0 = pts[i]
        x1, y1, z1 = pts[(i + 1) % n]
        nx += y0 * z1 - z0 * y1
        ny += z0 * x1 - x0 * z1
        nz += x0 * y1 - y0 * x1
    return math.sqrt(nx * nx + ny * ny + nz * nz) / 2.0


def _uints(length_fn: Any, at_fn: Any) -> List[int]:
    """A generated uint vector as a list. numpy's *AsNumpy accessors are
    faster where available; this keeps the example dependency-free."""
    return [at_fn(i) for i in range(length_fn())]


def area_by_surface_type(
    feature: fcb.Feature, header: fcb.HeaderView, lod: str
) -> Dict[str, float]:
    """Area per semantic surface type, from the flat arrays alone.

    Only geometries at `lod` are counted. A City Object carries ONE
    geometry per level of detail -- a 3DBAG BuildingPart has lod 1.2,
    1.3 and 2.2, and its parent Building an lod 0 footprint -- so
    summing every geometry would count each building three or four
    times over.
    """
    out: Dict[str, float] = {}
    info = header.info
    scale = info.scale or (1.0, 1.0, 1.0)
    translate = info.translate or (0.0, 0.0, 0.0)

    # One flat int vector of x,y,z triples, shared by every geometry in
    # this feature. Indices in Boundaries point into it.
    # Vertices() hands back a Vertex struct per index (X/Y/Z), not a
    # flat int array as the TypeScript port's vertices() does.
    raw_feature = fcb.raw_city_feature(feature)
    verts = [
        raw_feature.Vertices(i) for i in range(raw_feature.VerticesLength())
    ]

    for view in feature.city_objects():
        obj = fcb.raw_city_object(view)
        for g in range(obj.GeometryLength()):
            geom = obj.Geometry(g)
            geom_lod = geom.Lod()
            if geom_lod is None or geom_lod.decode() != lod:
                continue

            surfaces = _uints(geom.SurfacesLength, geom.Surfaces)
            strings = _uints(geom.StringsLength, geom.Strings)
            boundaries = _uints(geom.BoundariesLength, geom.Boundaries)
            semantics = _uints(geom.SemanticsLength, geom.Semantics)
            if not surfaces or not strings or not boundaries:
                continue

            ring = 0  # index into strings
            vertex = 0  # index into boundaries

            for s, ring_count in enumerate(surfaces):
                area = 0.0
                for r in range(ring_count):
                    n = strings[ring]
                    pts = []
                    for k in range(n):
                        v = verts[boundaries[vertex + k]]
                        pts.append(
                            (
                                v.X() * scale[0] + translate[0],
                                v.Y() * scale[1] + translate[1],
                                v.Z() * scale[2] + translate[2],
                            )
                        )
                    # Ring 0 is the outer boundary; the rest are holes,
                    # which subtract (CityJSON 2.0 section 6).
                    area += (1 if r == 0 else -1) * ring_area(pts)
                    vertex += n
                    ring += 1

                # Semantics is one entry per surface, in surface order,
                # so it indexes directly -- no regrouping needed for a
                # per-surface question. u32::MAX means "no semantic".
                label = "unassigned"
                if s < len(semantics) and semantics[s] != _U32_MAX:
                    so = geom.SemanticsObjects(semantics[s])
                    if so is not None:
                        ext = so.ExtensionType()
                        label = (
                            ext.decode()
                            if ext is not None
                            else fcb.semantic_surface_type_name(so.Type())
                        )
                out[label] = out.get(label, 0.0) + area
    return out


def area_by_surface_type_via_json(
    feature: fcb.Feature, header: fcb.HeaderView, lod: str
) -> Dict[str, float]:
    """The same totals via the nested CityJSON, used only to check the
    walk above. The slow path: it allocates the whole nested structure
    and the semantics arrays for every feature."""
    out: Dict[str, float] = {}
    cj = fcb.to_cityjson_feature(feature, header)
    info = header.info
    scale = info.scale or (1.0, 1.0, 1.0)
    translate = info.translate or (0.0, 0.0, 0.0)

    def at(i: int) -> Tuple[float, float, float]:
        v = cj["vertices"][i]
        return (
            v[0] * scale[0] + translate[0],
            v[1] * scale[1] + translate[1],
            v[2] * scale[2] + translate[2],
        )

    for obj in cj["CityObjects"].values():
        for geom in obj.get("geometry", []):
            if geom.get("lod") != lod:
                continue

            surfaces: List[List[List[int]]] = []

            def collect(node: Any) -> None:
                if not isinstance(node, list):
                    return
                if (
                    node
                    and isinstance(node[0], list)
                    and node[0]
                    and isinstance(node[0][0], int)
                ):
                    surfaces.append(node)
                    return
                for child in node:
                    collect(child)

            collect(geom["boundaries"])

            values: List[Any] = []

            def flatten(node: Any) -> None:
                if isinstance(node, list):
                    for child in node:
                        flatten(child)
                else:
                    values.append(node)

            flatten((geom.get("semantics") or {}).get("values", []))
            sem_surfaces = (geom.get("semantics") or {}).get("surfaces", [])

            for s, surface in enumerate(surfaces):
                area = 0.0
                for r, ring_idx in enumerate(surface):
                    area += (1 if r == 0 else -1) * ring_area(
                        [at(i) for i in ring_idx]
                    )
                label = "unassigned"
                if s < len(values) and values[s] is not None:
                    label = sem_surfaces[values[s]].get("type", label)
                out[label] = out.get(label, 0.0) + area
    return out


def main(path: str, limit: int, lod: str) -> int:
    reader = fcb.FcbReader.open_file(path)
    print(f"analysing the first {limit} feature(s) of {path}, lod {lod}")
    print("walking the flat FlatBuffers arrays -- no CityJSON, no nesting")
    print()

    totals: Dict[str, float] = {}
    features = 0
    mismatches = 0
    # 3DBAG publishes its own computed ground area per building, so the
    # walk can be checked against the dataset and not only against this
    # library's other code path.
    published_ground = 0.0
    have_published = False

    t0 = time.time()
    for feature in reader.select_all():
        if features >= limit:
            break
        flat = area_by_surface_type(feature, reader.header, lod)
        for k, v in flat.items():
            totals[k] = totals.get(k, 0.0) + v

        for view in feature.city_objects():
            if not view.has_attributes:
                continue
            schema = (
                view.columns
                if view.has_columns
                else reader.header.info.columns
            )
            attrs = fcb.decode_attributes(view.attributes or b"", schema or [])
            if isinstance(attrs.get("b3_opp_grond"), (int, float)):
                published_ground += float(attrs["b3_opp_grond"])
                have_published = True

        via_json = area_by_surface_type_via_json(feature, reader.header, lod)
        for k, v in flat.items():
            if abs(v - via_json.get(k, 0.0)) > 1e-6 * max(1.0, abs(v)):
                mismatches += 1
        features += 1
    ms = (time.time() - t0) * 1000

    order = ["RoofSurface", "GroundSurface", "WallSurface"]
    labels = [k for k in order if k in totals] + [
        k for k in totals if k not in order
    ]

    print(f"surface area at lod {lod} over {features} feature(s), m^2")
    total = 0.0
    for label in labels:
        total += totals[label]
        print(f"  {label:<16} {totals[label]:>12.2f}")
    print(f"  {'TOTAL':<16} {total:>12.2f}")

    print()
    print(
        "flat walk vs nested CityJSON: "
        + ("AGREE" if mismatches == 0 else f"{mismatches} MISMATCH(ES)")
    )
    if have_published:
        ground = totals.get("GroundSurface", 0.0)
        delta = abs(ground - published_ground)
        pct = 100.0 * delta / max(1.0, published_ground)
        print(
            "GroundSurface vs the dataset's own b3_opp_grond: "
            f"{ground:.2f} vs {published_ground:.2f} m^2 ({pct:.3f}% apart)"
        )
        print(
            "  a sanity check against a number this library did not "
            "produce. Ordinary\n  buildings agree to well under 1% (the 1 mm "
            "coordinate grid); a few\n  large multi-part ones differ more, "
            "because b3_opp_grond came from the\n  source geometry by a "
            "different pipeline. The READER check is above."
        )
    print(f"{features} feature(s) in {ms:.0f} ms")
    return 0


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3, 4):
        print(__doc__)
        raise SystemExit(2)
    n = int(sys.argv[2]) if len(sys.argv) >= 3 else 20
    level = sys.argv[3] if len(sys.argv) == 4 else "2.2"
    raise SystemExit(main(sys.argv[1], n, level))
