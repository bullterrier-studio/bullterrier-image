"""The build gate can fail: `poe build-check` builds the sdist and the wheel and rejects a distribution PyPI would
refuse to render (twine), and a wheel with a file that cannot be imported or with anything but the library's code (the
scaffold template, a picture, a test: check-wheel-contents), and passes the project as it is.

An architecture test of the tooling: its subject is the `build-check` task in pyproject.toml, so it runs that task, the
way `uv run poe check` does, in a copy of the project into which one fault at a time is written.
"""

import subprocess
import sys
import tomllib
from pathlib import Path

import pytest


@pytest.fixture
def project(project_copy) -> Path:
    return project_copy("pyproject.toml", "README.md", "LICENSE")


def build_check(project: Path, env: dict[str, str]) -> subprocess.CompletedProcess:
    command = [sys.executable, "-m", "poethepoet", "build-check"]
    return subprocess.run(command, cwd=project, env=env, capture_output=True, encoding="utf-8", check=False)


def test_the_project_as_it_is_builds_clean_distributions(project, uv_run_env):
    result = build_check(project, uv_run_env)
    assert result.returncode == 0, result.stdout + result.stderr
    built = sorted(path.name for path in (project / "dist" / "check").iterdir())
    version = tomllib.loads((project / "pyproject.toml").read_text(encoding="utf-8"))["project"]["version"]
    assert built == [
        f"bullterrier_image-{version}-py3-none-any.whl",
        f"bullterrier_image-{version}.tar.gz",
    ]  # sdist and wheel, nothing else


def test_a_long_description_pypi_cannot_render_fails_the_gate(project, uv_run_env):
    (project / "README.md").unlink()
    (project / "README.rst").write_text("A title\n=====\n\n.. not-a-directive::\n\n`unclosed\n", encoding="utf-8")
    pyproject = project / "pyproject.toml"
    pyproject.write_text(
        pyproject.read_text(encoding="utf-8").replace('readme = "README.md"', 'readme = "README.rst"'), encoding="utf-8"
    )
    result = build_check(project, uv_run_env)
    assert result.returncode != 0
    assert "long_description" in result.stdout + result.stderr  # twine: "`long_description` has syntax errors"


def test_a_wheel_file_that_cannot_be_imported_fails_the_gate(project, uv_run_env):
    (project / "src" / "bullterrier_image" / "not-importable.py").write_text(
        '"""A module with a dash."""\n', encoding="utf-8"
    )
    result = build_check(project, uv_run_env)
    assert result.returncode != 0
    report = result.stdout + result.stderr
    assert "W004" in report  # check-wheel-contents: module is not located at importable path
    assert "not-importable.py" in report


def _without_the_template_exclusion(project: Path) -> None:
    pyproject = project / "pyproject.toml"
    text = pyproject.read_text(encoding="utf-8")
    assert 'wheel-exclude = ["bullterrier_image/effects/_template"]' in text
    pyproject.write_text(text.replace('wheel-exclude = ["bullterrier_image/effects/_template"]', ""), encoding="utf-8")


def _picture_in_the_package(project: Path) -> None:
    (project / "src" / "bullterrier_image" / "effects" / "grade" / "grade.jpg").write_bytes(b"\xff\xd8\xff\xd9")


def _test_in_the_package(project: Path) -> None:
    (project / "src" / "bullterrier_image" / "effects" / "grade" / "test_grade.py").write_text(
        "def test_x(): ...\n", encoding="utf-8"
    )


def test_a_wheel_that_ships_more_than_the_library_code_fails_the_gate_naming_each_file(project, uv_run_env):
    # one build for the three faults (a build takes seconds); each shipped file must be named
    for fault in (_without_the_template_exclusion, _picture_in_the_package, _test_in_the_package):
        fault(project)
    result = build_check(project, uv_run_env)
    report = result.stdout + result.stderr
    assert result.returncode != 0
    assert "W102" in report  # check-wheel-contents: wheel library contains files not in the package tree
    for shipped in (
        "bullterrier_image/effects/_template/effect.py.tmpl",
        "bullterrier_image/effects/grade/grade.jpg",
        "test_grade.py",
    ):
        assert shipped in report
