from __future__ import annotations

import statistics
import sys
import time
from pathlib import Path
from typing import Callable

import pytest
from flatcitybuf.cityjson import to_cityjson_feature
from flatcitybuf.reader import FcbReader

# examples/data/delft.fcb: 1115 real-world features (task brief). Full
# scan = read the header, iterate every feature with select_all(), and
# convert each to its CityJSON dict -- the same workload
# test_conformance.py exercises per-file, just on a much bigger file and
# timed instead of asserted against.
DELFT = Path(__file__).resolve().parents[3] / "examples" / "data" / "delft.fcb"

_REPS = 5
_EXPECTED_FEATURE_COUNT = 1115

pytestmark = pytest.mark.benchmark


def _full_scan() -> int:
    r = FcbReader.open_file(DELFT)
    n = 0
    for feature in r.select_all():
        to_cityjson_feature(feature, r.header)
        n += 1
    return n


def _time_reps(scan: Callable[[], int], reps: int) -> list[float]:
    # One untimed warm-up (page cache, one-time import costs) before the
    # timed repetitions -- a single untimed run is not a measurement,
    # and neither is a single cold one.
    n = scan()
    assert n == _EXPECTED_FEATURE_COUNT
    times = []
    for _ in range(reps):
        start = time.perf_counter()
        n = scan()
        times.append(time.perf_counter() - start)
        assert n == _EXPECTED_FEATURE_COUNT
    return times


def _report(label: str, times: list[float]) -> None:
    print(
        f"\n{label}: min={min(times):.4f}s mean={statistics.mean(times):.4f}s"
        f" (n={len(times)}) {[round(t, 4) for t in times]}"
    )


@pytest.mark.skipif(not DELFT.exists(), reason="delft.fcb fixture missing")
def test_benchmark_full_scan_of_delft(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Full-scan wall-clock time for pure Python, pure Python + numpy,
    and (if still installed) the PyO3 module -- see src/py/README.md for
    the numbers this produced on the machine/Python version recorded
    there, and the task report for the full methodology. This is a
    MEASUREMENT, not a pass/fail gate: the only real assertions are that
    every path reads all 1115 features.
    """
    # ------------------------------------------------------ pure Python
    # `sys.modules["numpy"] = None` poisons any future bare `import
    # numpy` so it raises ImportError immediately -- see
    # test_cityjson.py's numpy-parity tests for why this is safe even
    # when numpy really is installed (flatbuffers' own already-executed
    # `import numpy as np` is unaffected).
    monkeypatch.setitem(sys.modules, "numpy", None)
    pure_times = _time_reps(_full_scan, _REPS)
    _report("pure Python           ", pure_times)
    monkeypatch.undo()

    # ------------------------------------------------- pure Python + numpy
    try:
        import numpy  # noqa: F401
    except ImportError:
        print(
            "\npure Python + numpy   : SKIPPED (numpy not installed; "
            "`uv sync --extra numpy` to measure this column)"
        )
    else:
        numpy_times = _time_reps(_full_scan, _REPS)
        _report("pure Python + numpy   ", numpy_times)

    # ------------------------------------------------------------ PyO3
    # The crate at src/rust/fcb_py (maturin) installs its compiled
    # extension under the Python import name `flatcitybuf` too -- the
    # SAME name this pure-Python package uses. The two therefore cannot
    # be installed side by side in one environment; whichever installs
    # second silently shadows the other. That is exactly why measuring
    # it here means checking for a SEPARATE marker (never importing
    # `flatcitybuf` a second time, which would just re-import this
    # package) rather than attempting to install it into this venv --
    # doing that would risk corrupting the environment under test. See
    # the task report for how this was actually measured (a throwaway
    # venv, if at all).
    print(
        "\nPyO3 (src/rust/fcb_py): SKIPPED -- its compiled extension "
        "installs under the same import name (`flatcitybuf`) as this "
        "package, so it cannot coexist in this environment; building "
        "it here would require installing maturin and a release cargo "
        "build, and it is retired in Task 13. See src/py/README.md / "
        "the task report for how (and whether) it was measured."
    )
