"""The dependency gate can fail: deptry, run with the project's configuration, rejects library code that imports a
package the project does not declare, or one declared only for development, and passes the package as it is.

An architecture test of the tooling: its subject is the `[tool.deptry]` configuration in pyproject.toml, so it runs
deptry the way `uv run poe check` does, on a copy of the package into which the offending import is written.
"""

import shutil
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]


@pytest.fixture
def checkout(tmp_path) -> Path:
    shutil.copytree(ROOT / "src", tmp_path / "src", ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
    shutil.copy(ROOT / "pyproject.toml", tmp_path)
    return tmp_path


def deptry(checkout: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, "-m", "deptry", "--no-ansi", "."],
        cwd=checkout,
        capture_output=True,
        encoding="utf-8",
        check=False,
    )


def test_the_package_as_it_is_declares_what_it_imports(checkout):
    result = deptry(checkout)
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize(
    ("module", "rule"),
    [
        ("yaml", "DEP004"),  # pyyaml is a development dependency: an installed library would not have it
        ("surely_not_installed_anywhere", "DEP001"),  # declared nowhere
    ],
)
def test_library_code_importing_an_undeclared_package_fails_naming_it(checkout, module, rule):
    probe = checkout / "src" / "photo_fx" / "ops" / "probe.py"
    probe.write_text(f'"""Probe."""\n\nimport {module}\n', encoding="utf-8")
    result = deptry(checkout)
    report = result.stdout + result.stderr
    assert result.returncode != 0, report
    [finding] = [line for line in report.splitlines() if rule in line]
    assert "probe.py" in finding
    assert module in finding
