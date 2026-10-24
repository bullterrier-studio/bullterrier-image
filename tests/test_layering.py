"""The package's layering: which way imports may point (import-linter, configured in pyproject.toml), and what a fresh
import sees. Each layer may import only the layers below it:

    app        cli > pipeline | gallery > sheet | checks | fx > recipe
    adapters   imageio, matte, loader (recipe files)
    effects    one folder per effect; never import each other
    core       runner > steps > framing | registry | regions > presets | outlines > params | vocabulary > types
    ops        may import errors
    errors     the exceptions; import nothing from bullterrier_image, so every layer may raise them

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

import bullterrier_image

PACKAGE = Path(bullterrier_image.__file__).parent
ROOT = PACKAGE.parent.parent  # the repository: pyproject.toml holds the contracts
LINT_IMPORTS = Path(sys.executable).with_name("lint-imports")  # `uv run lint-imports` from the repository root

LAYERS = "Layers: app > adapters > effects > core > ops > errors"
EFFECTS_APART = "Effects never import each other"
CORE_ORDER = "Core: runner > steps > framing|registry|regions > presets|outlines > params|vocabulary > types"
CONTRACTS = [LAYERS, EFFECTS_APART, CORE_ORDER]


def fresh_python(code: str) -> subprocess.CompletedProcess:
    """Run `code` in a fresh interpreter that imports this very package (the one these tests read the source of), with
    neither the current directory nor the user's site on its path."""
    environment = {**os.environ, "PYTHONPATH": str(PACKAGE.parent)}
    return subprocess.run(
        [sys.executable, "-P", "-s", "-c", code], env=environment, capture_output=True, encoding="utf-8", check=False
    )


def lint(cwd: Path, pythonpath: Path | None = None) -> subprocess.CompletedProcess:
    # a wide terminal, so that a contract's name is on one line however long it is
    environment = {**os.environ, "COLUMNS": "200", **({"PYTHONPATH": str(pythonpath)} if pythonpath else {})}
    return subprocess.run(
        [str(LINT_IMPORTS), "--no-cache"], cwd=cwd, env=environment, capture_output=True, encoding="utf-8", check=False
    )


def lint_a_copy_with(tmp_path: Path, module: str, line: str) -> subprocess.CompletedProcess:
    """Lint a copy of the package (and its contracts) in which `module` also has the import `line`."""
    shutil.copytree(PACKAGE, tmp_path / "bullterrier_image", ignore=shutil.ignore_patterns("__pycache__"))
    shutil.copy(ROOT / "pyproject.toml", tmp_path)
    target = tmp_path / "bullterrier_image" / module
    target.write_text(target.read_text(encoding="utf-8") + f"\n{line}  # injected by the test\n", encoding="utf-8")
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
        "import bullterrier_image.effects.ascii",
        "bullterrier_image.effects.glitch.effect",
        EFFECTS_APART,
    ),
    "ops to core": ("ops/cells.py", "import bullterrier_image.core.params", "bullterrier_image.ops.cells", LAYERS),
    "errors to ops": ("errors.py", "import bullterrier_image.ops.cells", "bullterrier_image.errors", LAYERS),
    "errors to core": ("errors.py", "import bullterrier_image.core.params", "bullterrier_image.errors", LAYERS),
    "core to effects": ("core/steps.py", "import bullterrier_image.effects", "bullterrier_image.core.steps", LAYERS),
    "core to imageio": ("core/runner.py", "import bullterrier_image.imageio", "bullterrier_image.core.runner", LAYERS),
    "effects to imageio": (
        "effects/grade/effect.py",
        "import bullterrier_image.imageio",
        "bullterrier_image.effects.grade.effect",
        LAYERS,
    ),
    "effect folder to effect folder": (
        "effects/grade/effect.py",
        "import bullterrier_image.effects.clarity.effect",
        "bullterrier_image.effects.grade.effect",
        EFFECTS_APART,
    ),
    "recipe to pipeline": ("recipe.py", "import bullterrier_image.pipeline", "bullterrier_image.recipe", LAYERS),
    "fx to gallery": ("fx.py", "import bullterrier_image.gallery", "bullterrier_image.fx", LAYERS),
    "pipeline to cli": ("pipeline.py", "import bullterrier_image.cli", "bullterrier_image.pipeline", LAYERS),
    "core to loader": ("core/steps.py", "import bullterrier_image.loader", "bullterrier_image.core.steps", LAYERS),
    "loader to recipe": ("loader.py", "import bullterrier_image.recipe", "bullterrier_image.loader", LAYERS),
    "registry to steps": (
        "core/registry.py",
        "import bullterrier_image.core.steps",
        "bullterrier_image.core.registry",
        CORE_ORDER,
    ),
    "presets to registry": (
        "core/presets.py",
        "import bullterrier_image.core.registry",
        "bullterrier_image.core.presets",
        CORE_ORDER,
    ),
    "params to presets": (
        "core/params.py",
        "import bullterrier_image.core.presets",
        "bullterrier_image.core.params",
        CORE_ORDER,
    ),
    "params to registry": (
        "core/params.py",
        "import bullterrier_image.core.registry",
        "bullterrier_image.core.params",
        CORE_ORDER,
    ),
}


@pytest.mark.parametrize(("module", "line", "importer", "contract"), FORBIDDEN.values(), ids=FORBIDDEN.keys())
def test_a_forbidden_import_breaks_its_contract(module, line, importer, contract, tmp_path):
    result = lint_a_copy_with(tmp_path, module, line)
    assert result.returncode != 0
    assert re.search(rf"{re.escape(contract)} BROKEN", result.stdout), result.stdout + result.stderr
    assert importer in result.stdout, result.stdout


ALLOWED = {
    "core to ops": ("core/steps.py", "import bullterrier_image.ops.cells"),
    "core to errors": ("core/vocabulary.py", "import bullterrier_image.errors"),
    "ops to errors": ("ops/cells.py", "import bullterrier_image.errors"),
}


@pytest.mark.parametrize(("module", "line"), ALLOWED.values(), ids=ALLOWED.keys())
def test_a_layer_may_import_the_layers_below_it(module, line, tmp_path):
    """Owner rulings: the layers are linear, app > adapters > effects > core > ops > errors, so core may use ops, and
    the exceptions, which import nothing, sit below everything so every layer may raise them."""
    result = lint_a_copy_with(tmp_path, module, line)
    assert result.returncode == 0, result.stdout + result.stderr
    for name in CONTRACTS:
        assert re.search(rf"{re.escape(name)} KEPT", result.stdout), f"{name!r} not reported kept:\n{result.stdout}"


def _effects_declared_in_the_source() -> set[str]:
    """Names of the functions decorated with `effect(...)` in the effect modules that discovery imports (the
    `effect.py` of every folder of `effects/` not starting with `_`), read from
    the source with `ast`: an oracle independent of the registry."""
    names = set()
    effects = PACKAGE / "effects"
    for path in sorted(effects.glob("*/effect.py")):
        if path.relative_to(effects).parts[0].startswith("_"):
            continue
        for node in ast.walk(ast.parse(path.read_text(encoding="utf-8"))):
            if isinstance(node, ast.FunctionDef) and any(
                isinstance(d, ast.Call) and getattr(d.func, "id", None) == "effect" for d in node.decorator_list
            ):
                names.add(node.name)
    return names


def test_importing_the_package_loads_no_effect_and_no_heavy_library():
    """`import bullterrier_image` stays fast: the effects, and the libraries they need, load when used."""
    heavy = ["bullterrier_image.effects", "scipy", "pydantic", "numpy", "PIL", "onnxruntime"]
    code = (
        "import json, sys\nimport bullterrier_image\n"
        f"json.dump([m for m in {heavy!r} if m in sys.modules], sys.stdout)\n"
    )
    result = fresh_python(code)
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout) == []


def test_importing_one_core_module_alone_finds_every_effect_on_the_first_query():
    """`core` never imports `effects`: the registry loads them through their entry point the first time it is asked."""
    code = (
        "import json, sys\n"
        "import bullterrier_image.core.runner\n"
        "from bullterrier_image.core import registry\n"
        "json.dump(sorted(registry.all_effects()), sys.stdout)\n"
    )
    result = fresh_python(code)
    assert result.returncode == 0, result.stderr
    declared = _effects_declared_in_the_source()
    assert len(declared) >= 10  # the oracle really found the effects (13 today)
    assert set(json.loads(result.stdout)) == declared


def test_the_recipe_namespace_is_reachable_from_a_fresh_import_of_the_package():
    """`import bullterrier_image; bullterrier_image.recipe` works, with every public name of the old module."""
    names = ["Recipe", "Stage", "Step", "load_recipe", "render", "render_all", "trace"]
    code = (
        "import bullterrier_image\n"
        f"names = {names!r}\n"
        "missing = [n for n in names if not hasattr(bullterrier_image.recipe, n)]\n"
        "assert not missing, missing\n"
        "recipe = bullterrier_image.recipe\n"
        "assert recipe.render is bullterrier_image.render and recipe.load_recipe is bullterrier_image.load_recipe\n"
    )
    result = fresh_python(code)
    assert result.returncode == 0, result.stderr
