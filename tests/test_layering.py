"""The package's layering: which way imports may point (import-linter, configured in pyproject.toml), and what a fresh
import sees. Each layer may import only the layers below it:

    app        cli > pipeline | gallery > sheet | checks | fx > recipe
    adapters   imageio, matte
    effects    one folder per effect (TRANSITIONAL: or a loose module); never import each other
    core       loader | runner > steps > registry > params | timing | vocabulary > types; may import ops
    ops        imports nothing from photo_fx outside ops

These are architecture tests: their subject is the package's structure (its import graph and its source files), so,
unlike behaviour tests, they read source (the linter, and `ast` as an independent list of the declared effects).
"""

import ast
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

import photo_fx

PACKAGE = Path(photo_fx.__file__).parent
ROOT = PACKAGE.parent.parent  # the repository: pyproject.toml holds the contracts
LINT_IMPORTS = Path(sys.executable).with_name("lint-imports")  # `uv run lint-imports` from the repository root

LAYERS = "Layers: app > adapters > effects > core > ops"
OPS_ALONE = "ops imports nothing else from photo_fx"
EFFECTS_APART = "Effects never import each other"
CORE_ORDER = "Core order: loader | runner > steps > registry > params | timing > types"
LOADER_DIRECT = "loader imports only steps, timing and errors from core"
TIMING_ALONE = "timing imports nothing else from photo_fx"
CONTRACTS = [LAYERS, OPS_ALONE, EFFECTS_APART, CORE_ORDER, LOADER_DIRECT, TIMING_ALONE]


def fresh_python(code: str) -> subprocess.CompletedProcess:
    """Run `code` in a fresh interpreter that imports this very package (the one these tests read the source of), with
    neither the current directory nor the user's site on its path."""
    environment = {**os.environ, "PYTHONPATH": str(PACKAGE.parent)}
    return subprocess.run(
        [sys.executable, "-P", "-s", "-c", code], env=environment, capture_output=True, text=True, check=False
    )


def lint(cwd: Path, pythonpath: Path | None = None) -> subprocess.CompletedProcess:
    environment = {**os.environ, "PYTHONPATH": str(pythonpath)} if pythonpath else None
    return subprocess.run(
        [str(LINT_IMPORTS), "--no-cache"], cwd=cwd, env=environment, capture_output=True, text=True, check=False
    )


def lint_a_copy_with(tmp_path: Path, module: str, line: str) -> subprocess.CompletedProcess:
    """Lint a copy of the package (and its contracts) in which `module` also has the import `line`."""
    shutil.copytree(PACKAGE, tmp_path / "photo_fx", ignore=shutil.ignore_patterns("__pycache__"))
    shutil.copy(ROOT / "pyproject.toml", tmp_path)
    target = tmp_path / "photo_fx" / module
    target.write_text(target.read_text() + f"\n{line}  # injected by the test\n")
    return lint(tmp_path, pythonpath=tmp_path)


def test_the_package_keeps_every_import_contract():
    result = lint(ROOT)
    assert result.returncode == 0, result.stdout + result.stderr
    for name in CONTRACTS:  # every contract was really checked (a dropped or renamed one cannot pass silently)
        assert re.search(rf"{re.escape(name)} KEPT", result.stdout), f"{name!r} not reported kept:\n{result.stdout}"


# One forbidden import per rule, each in a copy of the package: the linter must fail, naming the module and the rule.
FORBIDDEN = {
    "effect to effect": (
        "effects/glitch/effect.py",
        "import photo_fx.effects.ascii",
        "photo_fx.effects.glitch.effect",
        EFFECTS_APART,
    ),
    "ops to core": ("ops/cells.py", "import photo_fx.core.timing", "photo_fx.ops.cells", OPS_ALONE),
    "core to effects": ("core/steps.py", "import photo_fx.effects", "photo_fx.core.steps", LAYERS),
    "core to imageio": ("core/runner.py", "import photo_fx.imageio", "photo_fx.core.runner", LAYERS),
    "effects to imageio": (
        "effects/grade/effect.py",
        "import photo_fx.imageio",
        "photo_fx.effects.grade.effect",
        LAYERS,
    ),
    "effect folder to effect folder": (
        "effects/grade/effect.py",
        "import photo_fx.effects.clarity.effect",
        "photo_fx.effects.grade.effect",
        EFFECTS_APART,
    ),
    "recipe to pipeline": ("recipe.py", "import photo_fx.pipeline", "photo_fx.recipe", LAYERS),
    "fx to gallery": ("fx.py", "import photo_fx.gallery", "photo_fx.fx", LAYERS),
    "pipeline to cli": ("pipeline.py", "import photo_fx.cli", "photo_fx.pipeline", LAYERS),
    "loader to runner": ("core/loader.py", "import photo_fx.core.runner", "photo_fx.core.loader", CORE_ORDER),
    "steps to loader": ("core/steps.py", "import photo_fx.core.loader", "photo_fx.core.steps", CORE_ORDER),
    "registry to steps": ("core/registry.py", "import photo_fx.core.steps", "photo_fx.core.registry", CORE_ORDER),
    "loader to registry": ("core/loader.py", "import photo_fx.core.registry", "photo_fx.core.loader", LOADER_DIRECT),
    "params to registry": ("core/params.py", "import photo_fx.core.registry", "photo_fx.core.params", CORE_ORDER),
    "timing to registry": ("core/timing.py", "import photo_fx.core.registry", "photo_fx.core.timing", CORE_ORDER),
    "timing to ops": ("core/timing.py", "import photo_fx.ops.cells", "photo_fx.core.timing", TIMING_ALONE),
}


@pytest.mark.parametrize(("module", "line", "importer", "contract"), FORBIDDEN.values(), ids=FORBIDDEN.keys())
def test_a_forbidden_import_breaks_its_contract(module, line, importer, contract, tmp_path):
    result = lint_a_copy_with(tmp_path, module, line)
    assert result.returncode != 0
    assert re.search(rf"{re.escape(contract)} BROKEN", result.stdout), result.stdout + result.stderr
    assert importer in result.stdout, result.stdout


def test_core_may_import_ops(tmp_path):
    """Owner ruling: the layers are linear, app > adapters > effects > core > ops, so core may use ops."""
    result = lint_a_copy_with(tmp_path, "core/steps.py", "import photo_fx.ops.cells")
    assert result.returncode == 0, result.stdout + result.stderr
    for name in CONTRACTS:
        assert re.search(rf"{re.escape(name)} KEPT", result.stdout), f"{name!r} not reported kept:\n{result.stdout}"


def _effects_declared_in_the_source() -> set[str]:
    """Names of the functions decorated with `effect(...)` in the effect modules that discovery imports (the
    `effect.py` of every folder of `effects/`, and every TRANSITIONAL loose module, not starting with `_`), read from
    the source with `ast`: an oracle independent of the registry."""
    names = set()
    effects = PACKAGE / "effects"
    for path in sorted([*effects.glob("*.py"), *effects.glob("*/effect.py")]):
        if path.relative_to(effects).parts[0].startswith("_"):
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
    result = fresh_python(code)
    assert result.returncode == 0, result.stderr
    declared = _effects_declared_in_the_source()
    assert len(declared) >= 10  # the oracle really found the effects (13 today)
    assert set(json.loads(result.stdout)) == declared


def test_the_recipe_namespace_is_reachable_from_a_fresh_import_of_the_package():
    """`import photo_fx; photo_fx.recipe` worked before the split, with every public name of the old module."""
    names = [
        "Animated",
        "Animation",
        "Recipe",
        "Stage",
        "Step",
        "load_recipe",
        "ramp",
        "render",
        "render_all",
        "render_frames",
        "trace",
    ]
    code = (
        "import photo_fx\n"
        f"names = {names!r}\n"
        "missing = [n for n in names if not hasattr(photo_fx.recipe, n)]\n"
        "assert not missing, missing\n"
        "assert photo_fx.recipe.render is photo_fx.render and photo_fx.recipe.load_recipe is photo_fx.load_recipe\n"
    )
    result = fresh_python(code)
    assert result.returncode == 0, result.stderr
