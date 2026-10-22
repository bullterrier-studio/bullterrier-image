"""Discovery of effect folders: `src/photo_fx/effects/<name>/` holds one effect's code, and importing the package finds
it with no other edit. Every rule is tried on a copy of the package, from a fresh interpreter, so the real
package is never touched; each known-bad tree must stop the import with a message naming the folder and the rule.
(TRANSITIONAL: loose modules are still found too; see test_contract.py.)
"""

import json
import os
import shutil
import subprocess
import sys
import textwrap
from pathlib import Path

import pytest

import photo_fx
from photo_fx import effects


def _effect_module(name: str, category: str = "utilities") -> str:
    """The source of an `effect.py` registering an effect called `name`."""
    return textwrap.dedent(f'''
        import numpy as np

        from photo_fx.core.params import EffectParams, param
        from photo_fx.core.registry import effect

        class {name.title().replace("_", "")}Params(EffectParams):
            """Settings."""

            amount: float = param(0.5, unit="fraction", min=0, max=1, doc="Unused.")


        @effect(category="{category}", params={name.title().replace("_", "")}Params)
        def {name}(rgb: np.ndarray, params: {name.title().replace("_", "")}Params) -> np.ndarray:
            """A probe that returns the picture unchanged."""
            return rgb.copy()
    ''')


TRIGGER = "from . import effect  # registers the effect\n"

_REPORT = """
    import contextlib, io, json, sys
    from photo_fx import effects, fx
    from photo_fx.cli import main
    from photo_fx.recipe import Step

    listing = io.StringIO()
    with contextlib.redirect_stdout(listing):
        main(["effects"])
    json.dump({
        "package": effects.__file__,
        "registered": sorted(effects.describe()),
        "module": effects.get("zz_folder").__module__,
        "category": effects.category("zz_folder"),
        "fx": fx.zz_folder(amount=0.2).effect,
        "recipe_step": Step("zz_folder", {"amount": 0.2}).effect,
        "cli_lists_it": "zz_folder" in listing.getvalue(),
    }, sys.stdout)
"""


def _import_a_copy_with(
    tmp_path: Path, files: dict[str, str], code: str = "import photo_fx"
) -> subprocess.CompletedProcess:
    """Copy the package, add `files` (paths relative to its effects/ folder; a path ending in / makes an empty folder),
    then run `code` in a fresh interpreter that imports the copy."""
    package = tmp_path / "photo_fx"
    shutil.copytree(Path(photo_fx.__file__).parent, package, ignore=shutil.ignore_patterns("__pycache__"))
    for relative, source in files.items():
        target = package / "effects" / relative
        if relative.endswith("/"):
            target.mkdir(parents=True, exist_ok=True)
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(source)
    environment = {**os.environ, "PYTHONPATH": str(tmp_path)}
    return subprocess.run(
        [sys.executable, "-c", textwrap.dedent(code)], env=environment, capture_output=True, text=True, check=False
    )


def test_a_new_folder_in_the_effects_package_is_found_with_no_other_edit(tmp_path):
    done = _import_a_copy_with(
        tmp_path,
        {
            "zz_folder/__init__.py": TRIGGER,
            "zz_folder/effect.py": _effect_module("zz_folder", "texture"),
            "zz_stale/__pycache__/effect.cpython-313.pyc": "",  # what a moved-away folder leaves behind
            "zz_empty/": "",
        },
        _REPORT,
    )
    assert done.returncode == 0, done.stderr
    report = json.loads(done.stdout)
    assert Path(report["package"]).is_relative_to(tmp_path)  # it really looked at the copy
    assert set(report["registered"]) == set(effects.describe()) | {"zz_folder"}
    assert report["module"] == "photo_fx.effects.zz_folder.effect"
    assert report["category"] == "texture"
    assert report["fx"] == report["recipe_step"] == "zz_folder"
    assert report["cli_lists_it"]


KNOWN_BAD = {
    "a folder that registers nothing": (
        {"zz_bad/__init__.py": "", "zz_bad/effect.py": '"""No effect here."""\n'},
        ["effects/zz_bad/", "registers no effect"],
    ),
    "a folder whose effect has another name": (
        {"zz_bad/__init__.py": TRIGGER, "zz_bad/effect.py": _effect_module("zz_other")},
        ["effects/zz_bad/", "zz_other", "named like the folder"],
    ),
    "two effects in one folder": (
        {
            "zz_bad/__init__.py": TRIGGER + "from . import second  # a second effect\n",
            "zz_bad/effect.py": _effect_module("zz_bad"),
            "zz_bad/second.py": _effect_module("zz_bad_too"),
        },
        ["effects/zz_bad/", "zz_bad_too", "exactly one effect"],
    ),
    "an effect registered outside effect.py": (
        {"zz_bad/__init__.py": "from . import helpers  # registers\n", "zz_bad/helpers.py": _effect_module("zz_bad")},
        ["effects/zz_bad/", "zz_bad.helpers", "effect.py"],
    ),
    "a folder and a module of the same name": (
        {
            "zz_bad/__init__.py": TRIGGER,
            "zz_bad/effect.py": _effect_module("zz_bad"),
            "zz_bad.py": '"""A module that the folder would silently hide."""\n',
        },
        ["effects/zz_bad/", "effects/zz_bad.py"],
    ),
    "a folder without __init__.py": (
        {"zz_bad/effect.py": _effect_module("zz_bad")},
        ["effects/zz_bad/", "__init__.py"],
    ),
    "a folder and a module defining the same effect": (
        {
            "zz_bad/__init__.py": TRIGGER,
            "zz_bad/effect.py": _effect_module("zz_bad"),
            "zz_loose.py": _effect_module("zz_bad"),
        },
        ["two effects are called 'zz_bad'", "photo_fx.effects.zz_bad.effect", "photo_fx.effects.zz_loose"],
    ),
}


@pytest.mark.parametrize(("files", "named"), KNOWN_BAD.values(), ids=KNOWN_BAD.keys())
def test_a_broken_effect_folder_stops_the_import_naming_the_folder_and_the_rule(tmp_path, files, named):
    done = _import_a_copy_with(tmp_path, files)
    assert done.returncode != 0
    assert "ValueError" in done.stderr
    for text in named:
        assert text in done.stderr, done.stderr
