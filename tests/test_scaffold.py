"""`photo-fx new-effect`: a new effect is a set of new files named for it, written from the template, that works as
written: its package (`src/photo_fx/effects/<name>/`), its tests (`tests/effects/<name>/`) and its example recipe
(`examples/effects/<name>.toml`), where `gallery.effect_files` says they belong.

The scaffold always writes into the checkout photo-fx runs from, so these tests run it on a copy: in-process through
`gallery.new_effect(checkout=...)`, or as the command, in a fresh interpreter importing the copy. The last two prove
rule 4 of CLAUDE.md on a copy of the whole tree: adding an effect creates files and changes none, and the gate passes.
"""

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

import pytest

import photo_fx
from photo_fx import effects, gallery

ROOT = Path(__file__).resolve().parents[1]


def _in_a_git_checkout(folder: Path) -> bool:
    """Whether the git command exists and `folder` is inside a git work tree (a source archive is not)."""
    if shutil.which("git") is None:
        return False
    inside = subprocess.run(["git", "rev-parse", "--is-inside-work-tree"], cwd=folder, capture_output=True, check=False)
    return inside.returncode == 0


needs_git = pytest.mark.skipif(
    not _in_a_git_checkout(ROOT), reason="reads the repository with git: needs the git command and a git checkout"
)


@pytest.fixture
def checkout(tmp_path):
    """A scratch source checkout holding a copy of this photo-fx package (with its effect template)."""
    package = tmp_path / "checkout" / "src" / "photo_fx"
    shutil.copytree(Path(photo_fx.__file__).parent, package, ignore=shutil.ignore_patterns("__pycache__"))
    return tmp_path / "checkout"


def _tree(root: Path) -> dict[str, str]:
    """Every file under `root` (relative path -> sha256 of its bytes)."""
    return {
        path.relative_to(root).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in root.rglob("*")
        if path.is_file()
    }


def run_photo_fx(checkout: Path, *args: str) -> subprocess.CompletedProcess:
    """`photo-fx ARGS` from the copy of the package in `checkout` (so photo-fx takes the copy for its checkout)."""
    # no bytecode caches: the files in the checkout are exactly what the command wrote
    environment = {**os.environ, "PYTHONPATH": str(checkout / "src"), "PYTHONDONTWRITEBYTECODE": "1"}
    return subprocess.run(
        [sys.executable, "-m", "photo_fx.cli", *args], cwd=checkout, env=environment, capture_output=True, text=True
    )


def test_a_new_effect_is_new_files_in_the_places_its_name_gives(checkout):
    before = _tree(checkout)
    written = gallery.new_effect("lamp_probe", "light", checkout=checkout)
    files = gallery.effect_files("lamp_probe", checkout=checkout)
    after = _tree(checkout)
    assert {path: after[path] for path in before} == before  # nothing that was there changed
    assert sorted(checkout / path for path in set(after) - set(before)) == written  # it reports what it wrote
    for path in written:
        assert path.is_relative_to(files.package) or path in (files.tests, files.example), path
        assert "$" not in path.read_text(), f"{path.name} still holds a template placeholder"
    assert (files.package / "effect.py") in written
    assert files.tests in written
    recipe = tomllib.loads(files.example.read_text())
    assert recipe["name"] == "lamp_probe"
    assert recipe["size"] == 480
    assert [step["effect"] for step in recipe["step"]] == ["lamp_probe"]


def test_a_new_effect_is_registered_as_soon_as_the_package_is_imported(checkout):
    gallery.new_effect("lamp_probe", "light", checkout=checkout)
    code = (
        "import json, sys\n"
        "import numpy as np\n"
        "from photo_fx import effects\n"
        "out = effects.apply('lamp_probe', np.full((4, 4, 3), 0.25, np.float32), amount=1.0)\n"
        "json.dump([effects.category('lamp_probe'), float(out.max())], sys.stdout)\n"
    )
    environment = {**os.environ, "PYTHONPATH": str(checkout / "src")}
    done = subprocess.run([sys.executable, "-c", code], env=environment, capture_output=True, text=True, check=False)
    assert done.returncode == 0, done.stderr
    assert json.loads(done.stdout) == ["light", 0.75]  # the template's effect: the negative at amount 1


@pytest.mark.parametrize("name", ["grade", "glitch"])
def test_the_name_of_an_existing_effect_is_refused_and_nothing_is_written(checkout, name):
    before = _tree(checkout)
    with pytest.raises(FileExistsError, match=f"'{name}'"):
        gallery.new_effect(name, "light", checkout=checkout)
    assert _tree(checkout) == before


@pytest.mark.parametrize(
    "existing",
    [
        "src/photo_fx/effects/lamp_probe/notes.txt",
        "tests/effects/lamp_probe/test_lamp_probe.py",
        "examples/effects/lamp_probe.toml",
        "docs/gallery/lamp_probe.jpg",
    ],
    ids=["package", "tests", "example recipe", "picture"],
)
def test_a_name_any_of_whose_files_exists_is_refused(checkout, existing):
    (checkout / existing).parent.mkdir(parents=True, exist_ok=True)
    (checkout / existing).write_text("")
    before = _tree(checkout)
    with pytest.raises(FileExistsError, match="lamp_probe"):
        gallery.new_effect("lamp_probe", "light", checkout=checkout)
    assert _tree(checkout) == before


@pytest.mark.parametrize("name", ["Lamp", "2lamp", "lamp-probe", "class", "_lamp", ""])
def test_a_name_that_is_no_lowercase_identifier_is_refused(checkout, name):
    before = _tree(checkout)
    with pytest.raises(ValueError, match=repr(name)):
        gallery.new_effect(name, "light", checkout=checkout)
    assert _tree(checkout) == before


@pytest.mark.parametrize(
    ("name", "why"),
    [
        ("np", "effect.py"),  # the function would hide numpy from its own body
        ("param", "effect.py"),
        ("effect", "effect.py"),
        ("stub_text", "fx"),
        ("registry", "fx"),  # fx.registry is a module fx imports for itself, so it would never build the effect
        ("cache", "fx"),
    ],
)
def test_a_name_the_generated_effect_or_fx_already_uses_is_refused_saying_why(checkout, name, why):
    before = _tree(checkout)
    with pytest.raises(ValueError, match=repr(name)) as raised:
        gallery.new_effect(name, "light", checkout=checkout)
    assert why in str(raised.value)
    assert _tree(checkout) == before


@pytest.mark.parametrize("name", ["os", "core", "sys"])
def test_a_name_shared_with_some_other_module_is_fine_when_neither_effect_py_nor_fx_uses_it(checkout, name):
    written = gallery.new_effect(name, "light", checkout=checkout)
    assert gallery.effect_files(name, checkout=checkout).tests in written


def test_an_unknown_category_is_refused_listing_the_categories(checkout):
    before = _tree(checkout)
    with pytest.raises(ValueError, match="'lights'") as raised:
        gallery.new_effect("lamp_probe", "lights", checkout=checkout)
    assert ", ".join(effects.CATEGORIES) in str(raised.value)
    assert _tree(checkout) == before


def test_the_command_writes_the_files_prints_them_and_says_how_to_make_the_picture(checkout):
    done = run_photo_fx(checkout, "new-effect", "lamp_probe", "--category", "light")
    assert done.returncode == 0, done.stderr
    printed = [Path(line) for line in done.stdout.split()]
    assert printed
    assert all(path.is_file() and "lamp_probe" in path.as_posix() for path in printed)
    assert "photo-fx gallery --only lamp_probe" in done.stderr


def test_the_command_reports_an_existing_name_and_a_bad_category(checkout):
    before = _tree(checkout)
    taken = run_photo_fx(checkout, "new-effect", "grade", "--category", "colour")
    assert taken.returncode == 1
    assert "error:" in taken.stderr
    assert "'grade'" in taken.stderr
    unknown = run_photo_fx(checkout, "new-effect", "lamp_probe", "--category", "lights")
    assert unknown.returncode == 2  # a usage error: argparse lists the choices
    assert "lights" in unknown.stderr
    assert _tree(checkout) == before


# --- rule 4: adding an effect creates new files only, and they work ---------------------------------------------

PROBE = "zz_scaffold_probe_with_a_rather_long_name"  # long, so the template's lines must still fit the line limit


def _copy_of_the_tree(destination: Path) -> Path:
    """A copy, at `destination`, of every file git tracks or has staged, as it is in the working tree (so a change not
    yet committed is in it, a stray untracked file is not)."""
    listed = subprocess.run(
        ["git", "ls-files", "--cached", "-z"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout
    for relative in filter(None, listed.split("\0")):
        if (ROOT / relative).is_file():  # a file deleted but not yet staged is listed too
            (destination / relative).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination / relative)
    return destination


def _scaffold_into(copy: Path) -> list[str]:
    """Scaffold PROBE in `copy`, check that no existing file changed, and return the new files (relative paths)."""
    before = _tree(copy)
    scaffolded = run_photo_fx(copy, "new-effect", PROBE, "--category", "utilities")
    assert scaffolded.returncode == 0, scaffolded.stderr
    after = _tree(copy)
    assert {path: after.get(path) for path in before} == before  # no existing file changed or went missing
    return sorted(set(after) - set(before))


@needs_git
def test_adding_an_effect_creates_only_new_files_named_for_it_and_changes_no_existing_file(tmp_path):
    created = _scaffold_into(_copy_of_the_tree(tmp_path / "tree"))
    assert created  # something was added ...
    places = (f"src/photo_fx/effects/{PROBE}/", f"tests/effects/{PROBE}/", f"examples/effects/{PROBE}.")
    assert all(path.startswith(places) for path in created), created  # ... and only files named for the effect


@needs_git
def test_a_scaffolded_effect_passes_the_contract_tests_its_own_tests_and_ruff_as_written(tmp_path):
    """Bounded on purpose, to keep the suite fast: in the copy, ruff on the new files, then the contract tests (which
    run over every effect) and the new effect's own tests; not the whole gate."""
    copy = _copy_of_the_tree(tmp_path / "tree")
    created = _scaffold_into(copy)
    environment = {**os.environ, "PYTHONPATH": str(copy / "src")}  # the copy's photo_fx, not the installed one
    contract_and_own = ["tests/effects/test_contract.py", f"tests/effects/{PROBE}"]
    checks = [
        [sys.executable, "-m", "ruff", "format", "--check", "--no-cache", *created],
        [sys.executable, "-m", "ruff", "check", "--no-cache", *created],
        [sys.executable, "-m", "pytest", "-q", "-rA", "-p", "no:cacheprovider", *contract_and_own],
    ]
    for command in checks:
        done = subprocess.run(command, cwd=copy, env=environment, capture_output=True, text=True, check=False)
        assert done.returncode == 0, f"{' '.join(command[2:5])} failed in the copy:\n{done.stdout[-4000:]}{done.stderr}"
    assert f"PASSED tests/effects/{PROBE}/test_{PROBE}.py::" in done.stdout  # its own tests ran, and passed
    assert f"[{PROBE}]" in done.stdout  # the contract tests ran over it too
