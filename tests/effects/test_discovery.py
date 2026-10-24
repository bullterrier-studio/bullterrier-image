"""Discovery of effect folders: `src/bullterrier_image/effects/<name>/` holds one effect's code, and the registry's
first query finds it with no other edit. Every rule is tried on a copy of the package, from a fresh interpreter, so the
real package is never touched; each known-bad tree must stop that first query with a message naming the folder and the
rule.
"""

import json
import os
import shutil
import subprocess
import sys
import textwrap
from pathlib import Path

import pytest

import bullterrier_image
from bullterrier_image.core import registry


def _effect_module(name: str, category: str = "utilities") -> str:
    """The source of an `effect.py` registering an effect called `name`."""
    return textwrap.dedent(f'''
        import numpy as np

        from bullterrier_image.core.params import EffectParams, param
        from bullterrier_image.core.registry import effect

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
    from bullterrier_image import effects, fx
    from bullterrier_image.core import registry
    from bullterrier_image.cli import main
    from bullterrier_image.recipe import Step

    listing = io.StringIO()
    with contextlib.redirect_stdout(listing):
        main(["effects"])
    json.dump({
        "package": effects.__file__,
        "registered": sorted(registry.describe()),
        "module": registry.get("zz_folder").__module__,
        "category": registry.category("zz_folder"),
        "fx": fx.zz_folder(amount=0.2).effect,
        "recipe_step": Step("zz_folder", {"amount": 0.2}).effect,
        "cli_lists_it": "zz_folder" in listing.getvalue(),
    }, sys.stdout)
"""


FIRST_QUERY = "from bullterrier_image.core import registry\nregistry.describe()\n"  # loads the effects


def _import_a_copy_with(tmp_path: Path, files: dict[str, str], code: str = FIRST_QUERY) -> subprocess.CompletedProcess:
    """Copy the package, add `files` (paths relative to its effects/ folder; a path ending in / makes an empty folder),
    then run `code` in a fresh interpreter that imports the copy."""
    package = tmp_path / "bullterrier_image"
    shutil.copytree(Path(bullterrier_image.__file__).parent, package, ignore=shutil.ignore_patterns("__pycache__"))
    for relative, source in files.items():
        target = package / "effects" / relative
        if relative.endswith("/"):
            target.mkdir(parents=True, exist_ok=True)
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(source, encoding="utf-8")
    environment = {**os.environ, "PYTHONPATH": str(tmp_path)}
    return subprocess.run(
        [sys.executable, "-c", textwrap.dedent(code)],
        env=environment,
        capture_output=True,
        encoding="utf-8",
        check=False,
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
    assert set(report["registered"]) == set(registry.describe()) | {"zz_folder"}
    assert report["module"] == "bullterrier_image.effects.zz_folder.effect"
    assert report["category"] == "texture"
    assert report["fx"] == report["recipe_step"] == "zz_folder"
    assert report["cli_lists_it"]


def test_folders_are_imported_in_sorted_order_and_private_ones_not_at_all(tmp_path):
    log = "from bullterrier_image.effects import _order_log\n_order_log.seen.append(__name__)\n"
    done = _import_a_copy_with(
        tmp_path,
        {
            "_order_log.py": "seen = []\n",
            "zz_b/__init__.py": TRIGGER,
            "zz_b/effect.py": log + _effect_module("zz_b"),
            "zz_a/__init__.py": TRIGGER,
            "zz_a/effect.py": log + _effect_module("zz_a"),
            "_zz_hidden/__init__.py": TRIGGER,
            "_zz_hidden/effect.py": _effect_module("zz_hidden"),
        },
        """
        import json, sys
        from bullterrier_image.core import registry
        from bullterrier_image.effects import _order_log
        json.dump({"seen": _order_log.seen, "registered": sorted(registry.describe())}, sys.stdout)
        """,
    )
    assert done.returncode == 0, done.stderr
    report = json.loads(done.stdout)
    assert report["seen"] == [
        "bullterrier_image.effects.zz_a.effect",
        "bullterrier_image.effects.zz_b.effect",
    ]  # sorted, not as found
    assert {"zz_a", "zz_b"} <= set(report["registered"])
    assert "zz_hidden" not in report["registered"]


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
        ["effects/zz_bad.py", "loose module"],
    ),
    "a folder without __init__.py": (
        {"zz_bad/effect.py": _effect_module("zz_bad")},
        ["effects/zz_bad/", "__init__.py"],
    ),
    "a loose module": (
        {"zz_loose.py": _effect_module("zz_loose")},
        ["effects/zz_loose.py", "loose module", "effects/zz_loose/"],
    ),
}


@pytest.mark.parametrize(("files", "named"), KNOWN_BAD.values(), ids=KNOWN_BAD.keys())
def test_a_broken_effect_folder_stops_the_first_query_naming_the_folder_and_the_rule(tmp_path, files, named):
    done = _import_a_copy_with(tmp_path, files)
    assert done.returncode != 0
    assert "BullterrierImageError" in done.stderr
    for text in named:
        assert text in done.stderr, done.stderr
