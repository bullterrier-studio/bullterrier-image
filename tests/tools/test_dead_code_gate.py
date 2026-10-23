"""The dead-code gate can fail: vulture, run with the project's configuration, rejects an unused function and an unused
class, and passes the package as it is (so it is not noise that people learn to ignore).

An architecture test of the tooling: its subject is the `[tool.vulture]` configuration in pyproject.toml, so it runs
vulture the way `uv run poe check` does, on a copy of the package into which the dead code is written.
"""

import shutil
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]

DEAD_CODE = '''"""A module with code nothing calls."""


def never_called_helper(value: float) -> float:
    """Nothing calls this."""
    return value * 3


class NeverUsedThing:
    """Nothing makes one of these."""
'''


@pytest.fixture
def checkout(tmp_path) -> Path:
    shutil.copytree(ROOT / "src", tmp_path / "src", ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
    shutil.copy(ROOT / "pyproject.toml", tmp_path)
    return tmp_path


def vulture(checkout: Path) -> subprocess.CompletedProcess:
    # no arguments: the paths and every setting come from pyproject.toml, as in the gate
    return subprocess.run(
        [sys.executable, "-m", "vulture"], cwd=checkout, capture_output=True, encoding="utf-8", check=False
    )


def test_the_package_as_it_is_has_no_dead_code(checkout):
    result = vulture(checkout)
    assert result.returncode == 0, result.stdout + result.stderr
    assert result.stdout == ""


def test_an_unused_function_and_an_unused_class_fail_the_gate_by_name(checkout):
    (checkout / "src" / "photo_fx" / "ops" / "leftover.py").write_text(DEAD_CODE, encoding="utf-8")
    result = vulture(checkout)
    assert result.returncode != 0
    assert "leftover.py" in result.stdout
    assert "'never_called_helper'" in result.stdout
    assert "'NeverUsedThing'" in result.stdout
