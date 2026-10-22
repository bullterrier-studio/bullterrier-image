"""The package's layering: what a fresh import sees, and (with import-linter) which way imports may point."""

import ast
import json
import subprocess
import sys
from pathlib import Path

import photo_fx

PACKAGE = Path(photo_fx.__file__).parent


def _effects_declared_in_the_source() -> set[str]:
    """Names of the functions decorated with `effect(...)` in the effect modules that discovery imports (every module
    of `effects/` not starting with `_`), read from the source with `ast`: an oracle independent of the registry."""
    names = set()
    for path in sorted((PACKAGE / "effects").glob("*.py")):
        if path.name.startswith("_"):
            continue
        for node in ast.walk(ast.parse(path.read_text())):
            if isinstance(node, ast.FunctionDef) and any(
                isinstance(d, ast.Call) and getattr(d.func, "id", None) == "effect" for d in node.decorator_list
            ):
                names.add(node.name)
    return names


def test_importing_one_core_module_alone_registers_every_effect():
    """`core` never imports `effects`; the package's `__init__`, which Python runs before any submodule, does."""
    code = (
        "import json, sys\n"
        "import photo_fx.core.runner\n"
        "from photo_fx.core import registry\n"
        "json.dump(sorted(registry.all_effects()), sys.stdout)\n"
    )
    result = subprocess.run([sys.executable, "-I", "-c", code], capture_output=True, text=True, check=False)
    assert result.returncode == 0, result.stderr
    declared = _effects_declared_in_the_source()
    assert len(declared) >= 10  # the oracle really found the effects (13 today)
    assert set(json.loads(result.stdout)) == declared


def test_the_recipe_namespace_is_reachable_from_a_fresh_import_of_the_package():
    """`import photo_fx; photo_fx.recipe` worked before the split, with every public name of the old module."""
    names = "Animated Animation Recipe Stage Step load_recipe ramp render render_all render_frames trace".split()
    code = (
        "import photo_fx\n"
        f"names = {names!r}\n"
        "missing = [n for n in names if not hasattr(photo_fx.recipe, n)]\n"
        "assert not missing, missing\n"
        "assert photo_fx.recipe.render is photo_fx.render and photo_fx.recipe.load_recipe is photo_fx.load_recipe\n"
    )
    result = subprocess.run([sys.executable, "-I", "-c", code], capture_output=True, text=True, check=False)
    assert result.returncode == 0, result.stderr
