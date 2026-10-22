"""The lint and type gate can fail: each rule family it promises rejects a known-bad file, naming the rule.

An architecture test of the tooling: its subject is the configuration in pyproject.toml, so it runs ruff and mypy, the
way `uv run poe check` does, on a copy of the package and its tests into which one offending file at a time is
written. A known-good file passes both, so a failure is the offence's and not the copy's.
"""

import json
import shutil
import subprocess
import sys
from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
LIBRARY_MODULE = "src/photo_fx/ops/probe.py"  # a module of the package, under no per-file exemption
TEST_MODULE = "tests/test_probe.py"


@dataclass(frozen=True)
class Verdict:
    returncode: int
    report: str  # one finding per line, each naming its file and its rule code


def ruff(checkout: Path) -> Verdict:
    command = [sys.executable, "-m", "ruff", "check", "--no-cache", "--output-format=json", "."]
    result = subprocess.run(command, cwd=checkout, capture_output=True, text=True)
    findings = json.loads(result.stdout) if result.stdout.strip() else []
    report = "\n".join(f"{Path(f['filename']).relative_to(checkout)}: {f['code']} {f['message']}" for f in findings)
    return Verdict(result.returncode, report + result.stderr)


def mypy(checkout: Path) -> Verdict:
    # incremental, with its cache in the copy: after the first run only the probe file is checked again
    result = subprocess.run([sys.executable, "-m", "mypy"], cwd=checkout, capture_output=True, text=True)
    return Verdict(result.returncode, result.stdout + result.stderr)  # "path:line: error: message  [code]"


@pytest.fixture(scope="module")
def checkout(tmp_path_factory) -> Path:
    root = tmp_path_factory.mktemp("checkout")
    skip = shutil.ignore_patterns("__pycache__", "*.pyc")
    shutil.copytree(ROOT / "src", root / "src", ignore=skip)
    shutil.copytree(ROOT / "tests", root / "tests", ignore=skip)
    shutil.copy(ROOT / "pyproject.toml", root)
    return root


@pytest.fixture
def with_file(checkout) -> Iterator:
    """Write `source` at `relative` in the copy for one test; the copy is clean again afterwards."""
    written: list[Path] = []

    def write(relative: str, source: str) -> Path:
        path = checkout / relative
        path.write_text(source)
        written.append(path)
        return path

    yield write
    for path in written:
        path.unlink()


GOOD_MODULE = '''"""A probe module that keeps every rule."""


def scale(value: float, factor: float = 2.0) -> float:
    """`value` times `factor`."""
    return value * factor
'''

# One offence each: (file, source, tool, the rule's code in that tool's output)
OFFENCES = {
    "public function without type hints (ruff)": (
        LIBRARY_MODULE,
        '"""Probe."""\n\n\ndef double(value) -> float:\n    """Twice `value`."""\n    return value * 2\n',
        ruff,
        "ANN001",
    ),
    "public function without type hints (mypy)": (
        LIBRARY_MODULE,
        '"""Probe."""\n\n\ndef double(value) -> float:\n    """Twice `value`."""\n    return value * 2\n',
        mypy,
        "[no-untyped-def]",
    ),
    "public function without a docstring": (
        LIBRARY_MODULE,
        '"""Probe."""\n\n\ndef scale(value: float) -> float:\n    return value * 2\n',
        ruff,
        "D103",
    ),
    "Optional used without a None check": (
        LIBRARY_MODULE,
        '"""Probe."""\n\n\ndef first_word(text: str | None) -> str:\n    """The first word."""\n'
        "    return text.split()[0]\n",
        mypy,
        "[union-attr]",
    ),
    "print in a library module": (
        LIBRARY_MODULE,
        '"""Probe."""\n\n\ndef shout() -> None:\n    """Say it."""\n    print("hello")\n',
        ruff,
        "T201",
    ),
    "type: ignore without an error code": (
        LIBRARY_MODULE,
        '"""Probe."""\n\nCOUNT: int = "three"  # type: ignore\n',
        ruff,
        "PGH003",
    ),
    "a test importing a private name": (
        TEST_MODULE,
        "from photo_fx.ops.glyphs import _load_font\n\n\ndef test_probe():\n    assert callable(_load_font)\n",
        ruff,
        "PLC2701",
    ),
    "a test reading a private attribute": (
        TEST_MODULE,
        "from photo_fx import imageio\n\n\ndef test_probe():\n    assert imageio._SRGB is not None\n",
        ruff,
        "SLF001",
    ),
}


def test_the_copy_with_a_known_good_module_passes_both_tools(checkout, with_file):
    with_file(LIBRARY_MODULE, GOOD_MODULE)
    for tool in (ruff, mypy):
        verdict = tool(checkout)
        assert verdict.returncode == 0, verdict.report


def test_an_effect_added_before_fx_pyi_is_regenerated_type_checks(checkout, with_file):
    # fx.pyi is regenerated only at integration, so a branch's new effect is not in it yet; mypy must not mind
    with_file(TEST_MODULE, "from photo_fx import fx\n\n\ndef test_probe():\n    assert fx.not_in_the_stub(radius=1)\n")
    verdict = mypy(checkout)
    assert verdict.returncode == 0, verdict.report


@pytest.mark.parametrize(("relative", "source", "tool", "code"), OFFENCES.values(), ids=OFFENCES.keys())
def test_each_rule_family_fails_the_gate_naming_the_rule(checkout, with_file, relative, source, tool, code):
    with_file(relative, source)
    verdict = tool(checkout)
    assert verdict.returncode != 0, verdict.report
    [finding] = [line for line in verdict.report.splitlines() if code in line]  # exactly this offence, once
    assert finding.startswith(relative), verdict.report  # pointing at the offending file
