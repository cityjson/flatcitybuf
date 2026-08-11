"""Runs every script in examples/ so they cannot rot.

The examples are documentation, and documentation that is never
executed drifts silently: an API rename or a changed accessor breaks
them without breaking anything the suite covers. Each is run as a real
subprocess -- the way a reader would run it -- and asserted on exit
status plus a line only a working run can print.

The live-HTTP example is exercised for its argument handling only; the
network path is covered by test_http.py's opt-in remote test.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
EXAMPLES = ROOT / "examples"
DELFT = ROOT.parents[1] / "examples" / "data" / "delft.fcb"
CORPUS = ROOT.parents[1] / "conformance"
BBOX = ["84500", "445800", "85000", "446500"]

needs_delft = pytest.mark.skipif(
    not DELFT.exists(), reason="examples/data/delft.fcb missing"
)


def number(pattern: str, text: str) -> float:
    """The one capture group of `pattern`, as a float. Asserts rather
    than returning Optional so a changed output line fails the test
    loudly instead of silently skipping the comparison."""
    m = re.search(pattern, text)
    assert m is not None, f"{pattern!r} did not match:\n{text}"
    return float(m.group(1))


def run(script: str, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(EXAMPLES / script), *args],
        capture_output=True,
        text=True,
        cwd=ROOT,
    )


def test_every_example_is_covered_here() -> None:
    # A new example must be added to this file too, or it ships
    # unexecuted -- exactly the drift these tests exist to prevent.
    on_disk = {p.name for p in EXAMPLES.glob("*.py")}
    covered = {
        "inspect_header.py",
        "read_local.py",
        "query_attributes.py",
        "read_features.py",
        "to_cityjson.py",
        "custom_reader.py",
        "read_http.py",
        "geometry_analysis.py",
    }
    assert on_disk == covered


@needs_delft
def test_inspect_header_reports_the_queryable_columns() -> None:
    r = run("inspect_header.py", str(DELFT))
    assert r.returncode == 0, r.stderr
    assert "features      1115" in r.stdout
    assert "44 of 44 columns are queryable" in r.stdout


@needs_delft
def test_read_local_emits_a_header_line_plus_every_feature() -> None:
    r = run("read_local.py", str(DELFT))
    assert r.returncode == 0, r.stderr
    assert len(r.stdout.strip().splitlines()) == 1116


@needs_delft
def test_read_local_with_a_bbox_emits_only_the_matches() -> None:
    r = run("read_local.py", str(DELFT), *BBOX)
    assert r.returncode == 0, r.stderr
    # 170 features plus the CityJSON header line. The same 170 the C++
    # reader and the Rust writer's own bbox filter agree on.
    assert len(r.stdout.strip().splitlines()) == 171


@needs_delft
def test_query_attributes_matches_on_two_anded_conditions() -> None:
    r = run(
        "query_attributes.py",
        str(DELFT),
        "b3_h_dak_50p",
        "gt",
        "20",
        "b3_dak_type",
        "eq",
        "slanted",
    )
    assert r.returncode == 0, r.stderr
    assert "1 of 1115 features matched" in r.stdout
    assert "NL.IMBAG.Pand.0503100000032914" in r.stdout


@needs_delft
def test_query_attributes_rejects_an_unknown_column() -> None:
    r = run("query_attributes.py", str(DELFT), "nope", "eq", "1")
    assert r.returncode == 1
    assert "no column named" in r.stderr


@needs_delft
def test_read_features_shows_the_per_object_schema_override() -> None:
    r = run("read_features.py", str(DELFT), "1")
    assert r.returncode == 0, r.stderr
    assert "schema   own" in r.stdout
    assert "1 object(s) carried their own schema" in r.stdout


@needs_delft
def test_to_cityjson_reaches_into_the_decoded_dicts() -> None:
    r = run("to_cityjson.py", str(DELFT), "0")
    assert r.returncode == 0, r.stderr
    assert "== metadata (to_cityjson_metadata) ==" in r.stdout
    assert "NL.IMBAG.Pand.0503100000031902" in r.stdout


def test_to_cityjson_shows_templates_and_palette_together() -> None:
    # geom_temp is the fixture whose header carries BOTH geometry
    # templates and the appearance palette those templates index --
    # the pair that finding #31 was about.
    r = run("to_cityjson.py", str(CORPUS / "geom_temp.fcb"), "0")
    assert r.returncode == 0, r.stderr
    assert "templates 3" in r.stdout
    assert "palette   2 material(s), 2 texture(s)" in r.stdout


@needs_delft
def test_custom_reader_reads_a_fraction_of_the_file_for_a_bbox() -> None:
    r = run("custom_reader.py", str(DELFT), *BBOX)
    assert r.returncode == 0, r.stderr
    assert "170 hit(s)" in r.stdout
    assert "% of the file)" in r.stdout


@needs_delft
def test_geometry_analysis_agrees_with_json_and_with_the_dataset() -> None:
    r = run("geometry_analysis.py", str(DELFT), "20", "2.2")
    assert r.returncode == 0, r.stderr
    # The flat walk must match this library's own nested path...
    assert "flat walk vs nested CityJSON: AGREE" in r.stdout
    # ...and 3DBAG's published ground area, a number this library did
    # not produce. They differ only by the 1 mm coordinate quantisation.
    assert number(r"\(([0-9.]+)% apart\)", r.stdout) < 0.05
    # Sloped roofs at lod 2.2 exceed the footprint they sit on.
    roof = number(r"RoofSurface\s+([0-9.]+)", r.stdout)
    ground = number(r"GroundSurface\s+([0-9.]+)", r.stdout)
    assert roof > ground


@needs_delft
def test_geometry_analysis_lod12_roof_equals_its_footprint() -> None:
    # An LoD1.2 building is a flat extrusion of its footprint, so roof
    # and ground area coincide. A walk that mis-grouped surfaces or
    # mis-read the semantics would not reproduce that.
    r = run("geometry_analysis.py", str(DELFT), "20", "1.2")
    assert r.returncode == 0, r.stderr
    roof = number(r"RoofSurface\s+([0-9.]+)", r.stdout)
    ground = number(r"GroundSurface\s+([0-9.]+)", r.stdout)
    assert abs(roof - ground) < 0.01


def test_read_http_prints_usage_without_arguments() -> None:
    # The network path itself is test_http.py's opt-in remote test.
    r = run("read_http.py")
    assert r.returncode == 2
    assert "range requests" in r.stdout
