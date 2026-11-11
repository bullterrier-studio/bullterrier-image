"""tools/crate_versions.py: the two published crates carry the package's version, spelled as SemVer, and a release
writes it (`[tool.release] after-bump`). The files of this repository must agree; a copy that disagrees is named."""

import importlib.util
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "tools" / "crate_versions.py"


def _load():
    spec = importlib.util.spec_from_file_location("crate_versions", TOOL)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


crate_versions = _load()


def test_the_crates_of_this_repository_carry_the_packages_version():
    result = subprocess.run([sys.executable, str(TOOL)], cwd=ROOT, capture_output=True, encoding="utf-8", check=False)
    assert result.returncode == 0, result.stderr


@pytest.mark.parametrize(
    ("pep440", "semver"),
    [("7.2.0", "7.2.0"), ("7.2.0rc1", "7.2.0-rc.1"), ("9.0.0a3", "9.0.0-alpha.3"), ("9.0.0b12", "9.0.0-beta.12")],
)
def test_a_release_version_is_spelled_as_semver(pep440, semver):
    assert crate_versions.semver(pep440) == semver


@pytest.mark.parametrize("bad", ["0.2", "7.2.0.dev0", "7.2.0.post1", "v7.2.0", "7.2.0-rc.1"])
def test_a_version_release_yml_does_not_accept_is_refused(bad):
    with pytest.raises(ValueError, match=r"MAJOR\.MINOR\.PATCH"):
        crate_versions.semver(bad)


def project(folder: Path, package: str, crates: str, requirement: str) -> None:
    """A workspace with the derive crate, at `crates` and required `=requirement`, of a package at `package`."""
    (folder / "pyproject.toml").write_text(f'[project]\nname = "demo"\nversion = "{package}"\n', encoding="utf-8")
    (folder / "crates" / "derive").mkdir(parents=True)
    (folder / "crates" / "derive" / "Cargo.toml").write_text(
        '[package]\nname = "bullterrier-image-derive"\nversion.workspace = true\nedition = "2024"\n'
        '[lib]\npath = "lib.rs"\n',
        encoding="utf-8",
    )
    (folder / "crates" / "derive" / "lib.rs").write_text("", encoding="utf-8")
    (folder / "Cargo.toml").write_text(
        f'[workspace]\nmembers = ["crates/derive"]\nresolver = "3"\n\n'
        f'[workspace.package]\nversion = "{crates}"  # kept\n\n[workspace.dependencies]\n'
        f'bullterrier-image-derive = {{ path = "crates/derive", version = "={requirement}" }}  # kept\n',
        encoding="utf-8",
    )


def run(folder: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, str(TOOL), *args], cwd=folder, capture_output=True, encoding="utf-8", check=False
    )


@pytest.mark.parametrize(
    ("crates", "requirement", "named"),
    [("7.1.0", "7.2.0", "[workspace.package] version"), ("7.2.0", "7.1.0", "derive dependency")],
    ids=["crates behind", "requirement behind"],
)
def test_crates_that_disagree_with_the_package_are_named(tmp_path, crates, requirement, named):
    project(tmp_path, "7.2.0", crates, requirement)
    result = run(tmp_path)
    assert result.returncode == 1
    assert named in result.stderr


def test_writing_a_release_version_sets_the_crates_the_requirement_and_the_lock(tmp_path):
    project(tmp_path, "7.1.0", "7.1.0", "7.1.0")
    result = run(tmp_path, "7.2.0rc1")
    assert result.returncode == 0, result.stderr
    cargo = (tmp_path / "Cargo.toml").read_text(encoding="utf-8")
    assert 'version = "7.2.0-rc.1"  # kept' in cargo
    assert 'version = "=7.2.0-rc.1" }  # kept' in cargo  # the comments after the values stay
    assert 'version = "7.2.0-rc.1"' in (tmp_path / "Cargo.lock").read_text(encoding="utf-8")


@pytest.mark.parametrize("crate", ["core", "derive"])
def test_a_published_crate_ships_the_licence_of_the_repository(crate):
    assert (ROOT / "crates" / crate / "LICENSE").read_text(encoding="utf-8") == (ROOT / "LICENSE").read_text(
        encoding="utf-8"
    )
