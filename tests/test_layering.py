"""The package's layering: which way imports may point (import-linter, configured in pyproject.toml), and what a fresh
import sees. Each layer may import only the layers below it:

    app        cli > pipeline | gallery > sheet | checks | fx > recipe
    adapters   imageio, matte
    effects    one module per effect; never import each other
    core       loader | runner > steps > registry | timing | types | vocabulary; may import ops
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
CORE_ORDER = "Core modules: loader | runner > steps > registry | timing | types | vocabulary"
LOADER_DIRECT = "loader imports only steps and timing from core"
TIMING_ALONE = "timing imports nothing else from photo_fx"
CONTRACTS = [LAYERS, OPS_ALONE, EFFECTS_APART, CORE_ORDER, LOADER_DIRECT, TIMING_ALONE]


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
        "effects/glitch.py",
        "import photo_fx.effects.ascii",
        "photo_fx.effects.glitch",
        EFFECTS_APART,
    ),
    "ops to core": ("ops/warp.py", "import photo_fx.core.timing", "photo_fx.ops.warp", OPS_ALONE),
    "core to effects": ("core/steps.py", "import photo_fx.effects", "photo_fx.core.steps", LAYERS),
    "core to imageio": ("core/runner.py", "import photo_fx.imageio", "photo_fx.core.runner", LAYERS),
    "effects to imageio": ("effects/color.py", "import photo_fx.imageio", "photo_fx.effects.color", LAYERS),
    "recipe to pipeline": ("recipe.py", "import photo_fx.pipeline", "photo_fx.recipe", LAYERS),
    "fx to gallery": ("fx.py", "import photo_fx.gallery", "photo_fx.fx", LAYERS),
    "pipeline to cli": ("pipeline.py", "import photo_fx.cli", "photo_fx.pipeline", LAYERS),
    "loader to runner": ("core/loader.py", "import photo_fx.core.runner", "photo_fx.core.loader", CORE_ORDER),
    "steps to loader": ("core/steps.py", "import photo_fx.core.loader", "photo_fx.core.steps", CORE_ORDER),
    "registry to steps": ("core/registry.py", "import photo_fx.core.steps", "photo_fx.core.registry", CORE_ORDER),
    "loader to registry": ("core/loader.py", "import photo_fx.core.registry", "photo_fx.core.loader", LOADER_DIRECT),
    "timing to registry": ("core/timing.py", "import photo_fx.core.registry", "photo_fx.core.timing", CORE_ORDER),
    "timing to ops": ("core/timing.py", "import photo_fx.ops.warp", "photo_fx.core.timing", TIMING_ALONE),
}


@pytest.mark.parametrize(("module", "line", "importer", "contract"), FORBIDDEN.values(), ids=FORBIDDEN.keys())
def test_a_forbidden_import_breaks_its_contract(module, line, importer, contract, tmp_path):
    result = lint_a_copy_with(tmp_path, module, line)
    assert result.returncode != 0
    assert re.search(rf"{re.escape(contract)} BROKEN", result.stdout), result.stdout + result.stderr
    assert importer in result.stdout, result.stdout


def test_core_may_import_ops(tmp_path):
    """Owner ruling: the layers are linear, app > adapters > effects > core > ops, so core may use ops."""
    result = lint_a_copy_with(tmp_path, "core/steps.py", "import photo_fx.ops.warp")
    assert result.returncode == 0, result.stdout + result.stderr
    for name in CONTRACTS:
        assert re.search(rf"{re.escape(name)} KEPT", result.stdout), f"{name!r} not reported kept:\n{result.stdout}"


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
    result = subprocess.run([sys.executable, "-I", "-c", code], capture_output=True, text=True, check=False)
    assert result.returncode == 0, result.stderr
