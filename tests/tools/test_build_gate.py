"""The build gate can fail: `poe build-check` builds the sdist and the wheel and rejects a distribution PyPI would
refuse to render (twine), and a wheel with a file that cannot be imported or with anything but the library's code (the
scaffold template, a picture, a test: check-wheel-contents), and passes the project as it is.

An architecture test of the tooling: its subject is the `build-check` task in pyproject.toml, so it runs that task, the
way `uv run poe check` does, in a copy of the project into which one fault at a time is written.
"""

import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]


@pytest.fixture
def project(tmp_path) -> Path:
    shutil.copytree(ROOT / "src", tmp_path / "src", ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
    for name in ("pyproject.toml", "README.md", "LICENSE"):
        shutil.copy(ROOT / name, tmp_path)
    return tmp_path


def build_check(project: Path) -> subprocess.CompletedProcess:
    # the environment's own tools first on PATH, as under `uv run`
    environment = {**os.environ, "PATH": f"{Path(sys.executable).parent}{os.pathsep}{os.environ.get('PATH', '')}"}
    return subprocess.run(
        [sys.executable, "-m", "poethepoet", "build-check"],
        cwd=project,
        env=environment,
        capture_output=True,
        encoding="utf-8",
        check=False,
    )


def test_the_project_as_it_is_builds_clean_distributions(project):
    result = build_check(project)
    assert result.returncode == 0, result.stdout + result.stderr
    built = sorted(path.name for path in (project / "dist" / "check").iterdir())
    assert built == ["photo_fx-0.1.0-py3-none-any.whl", "photo_fx-0.1.0.tar.gz"]  # sdist and wheel, nothing else


def test_a_long_description_pypi_cannot_render_fails_the_gate(project):
    (project / "README.md").unlink()
    (project / "README.rst").write_text("A title\n=====\n\n.. not-a-directive::\n\n`unclosed\n", encoding="utf-8")
    pyproject = project / "pyproject.toml"
    pyproject.write_text(
        pyproject.read_text(encoding="utf-8").replace('readme = "README.md"', 'readme = "README.rst"'), encoding="utf-8"
    )
    result = build_check(project)
    assert result.returncode != 0
    assert "long_description" in result.stdout + result.stderr  # twine: "`long_description` has syntax errors"


def test_a_wheel_file_that_cannot_be_imported_fails_the_gate(project):
    (project / "src" / "photo_fx" / "not-importable.py").write_text('"""A module with a dash."""\n', encoding="utf-8")
    result = build_check(project)
    assert result.returncode != 0
    report = result.stdout + result.stderr
    assert "W004" in report  # check-wheel-contents: module is not located at importable path
    assert "not-importable.py" in report


def _without_the_template_exclusion(project: Path) -> None:
    pyproject = project / "pyproject.toml"
    text = pyproject.read_text(encoding="utf-8")
    assert 'wheel-exclude = ["photo_fx/effects/_template"]' in text
    pyproject.write_text(text.replace('wheel-exclude = ["photo_fx/effects/_template"]', ""), encoding="utf-8")


def _picture_in_the_package(project: Path) -> None:
    (project / "src" / "photo_fx" / "effects" / "grade" / "grade.jpg").write_bytes(b"\xff\xd8\xff\xd9")


def _test_in_the_package(project: Path) -> None:
    (project / "src" / "photo_fx" / "effects" / "grade" / "test_grade.py").write_text(
        "def test_x(): ...\n", encoding="utf-8"
    )


def test_a_wheel_that_ships_more_than_the_library_code_fails_the_gate_naming_each_file(project):
    # one build for the three faults (a build takes seconds); each shipped file must be named
    for fault in (_without_the_template_exclusion, _picture_in_the_package, _test_in_the_package):
        fault(project)
    result = build_check(project)
    report = result.stdout + result.stderr
    assert result.returncode != 0
    assert "W102" in report  # check-wheel-contents: wheel library contains files not in the package tree
    for shipped in ("photo_fx/effects/_template/effect.py.tmpl", "photo_fx/effects/grade/grade.jpg", "test_grade.py"):
        assert shipped in report
