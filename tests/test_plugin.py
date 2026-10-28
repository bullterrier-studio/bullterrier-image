"""Third-party effect packages: written against `bullterrier_image.plugin`, declared as an entry point of the group
`bullterrier_image.effects`, found by the registry like bullterrier-image's own effects.

Each test installs a throwaway package the way an installer would (the package plus a `.dist-info` folder holding its
`entry_points.txt`) in a folder put on `PYTHONPATH`, and runs a fresh interpreter, so the registry loads the effect
packages from scratch and nothing in this process changes.
"""

import json
import os
import subprocess
import sys
import textwrap
from pathlib import Path

import pytest

TINT_PACKAGE = '''
"""A third-party effect package."""

import numpy as np

from bullterrier_image.plugin import Color, EffectParams, effect, param


class TintParams(EffectParams):
    """Settings of the tint."""

    colour: Color = param("#ff0000", doc="The colour mixed in.")
    amount: float = param(0.5, unit="fraction", min=0, max=1, doc="How much of it, 0..1. Raising it tints more.")


@effect(category="colour", params=TintParams)
def zz_tint(rgb: np.ndarray, params: TintParams) -> np.ndarray:
    """Mixes a flat colour into the picture."""
    return (rgb * (1 - params.amount) + np.asarray(params.colour, np.float32) * params.amount).astype(np.float32)
'''

# Registers an effect under the name of one of bullterrier-image's own, which the registry refuses.
CLASHING_PACKAGE = '''
"""A broken third-party effect package."""

from bullterrier_image.plugin import EffectParams, effect


class BloomParams(EffectParams):
    """No settings."""


@effect(category="light", params=BloomParams)
def bloom(rgb, params):
    """Another bloom."""
    return rgb
'''


def install(site: Path, distribution: str, entry_point: str, source: str) -> None:
    """Install, in `site`, a distribution whose module `<entry_point>_pkg` holds `source`, declared as the entry point
    `entry_point` of the group `bullterrier_image.effects`."""
    module = f"{entry_point}_pkg"
    (site / f"{module}.py").write_text(source, encoding="utf-8")
    info = site / f"{distribution.replace('-', '_')}-1.0.dist-info"
    info.mkdir()
    (info / "METADATA").write_text(f"Metadata-Version: 2.1\nName: {distribution}\nVersion: 1.0\n", encoding="utf-8")
    (info / "entry_points.txt").write_text(f"[bullterrier_image.effects]\n{entry_point} = {module}\n", encoding="utf-8")


def fresh_python(site: Path, code: str, *args: str) -> dict:
    """Run `code` in a fresh interpreter that sees the packages installed in `site`; what it printed, as JSON."""
    environment = {**os.environ, "PYTHONPATH": str(site)}
    environment.pop("BT_IMAGE_PLUGINS", None)
    done = subprocess.run(
        [sys.executable, "-c", textwrap.dedent(code), *args],
        env=environment,
        capture_output=True,
        encoding="utf-8",
        check=False,
    )
    assert done.returncode == 0, done.stderr
    return json.loads(done.stdout)


def test_an_installed_plugin_effect_is_listed_and_runs_from_toml_and_from_fx(tmp_path):
    install(tmp_path, "zz-tint", "zz_tint", TINT_PACKAGE)
    recipe = tmp_path / "tinted.toml"
    recipe.write_text(
        'name = "tinted"\n\n[[step]]\neffect = "zz_tint"\ncolour = "#0000ff"\namount = 0.25\n', encoding="utf-8"
    )
    report = fresh_python(
        tmp_path,
        """
        import json, sys
        import numpy as np
        from bullterrier_image import Recipe, fx, load_recipe, render
        from bullterrier_image.core import registry

        grey = np.full((6, 8, 3), 0.5, np.float32)
        from_toml = render(grey, load_recipe(sys.argv[1]))
        from_fx = render(grey, Recipe([fx.zz_tint("#0000ff", amount=0.25)], name="tinted"))
        json.dump({
            "listed": sorted(registry.categories()["colour"]),
            "builtin_too": "bloom" in registry.describe(),
            "toml": [float(from_toml.min()), float(from_toml.max()), from_toml[0, 0].tolist()],
            "fx": from_fx[0, 0].tolist(),
        }, sys.stdout)
        """,
        str(recipe),
    )
    assert "zz_tint" in report["listed"]
    assert report["builtin_too"]
    expected = [0.375, 0.375, 0.625]  # 0.5 x 0.75 + (0, 0, 1) x 0.25
    assert report["toml"][2] == pytest.approx(expected, abs=1e-6)
    assert report["toml"][0] == pytest.approx(0.375, abs=1e-6)  # every pixel the same
    assert report["toml"][1] == pytest.approx(0.625, abs=1e-6)
    assert report["fx"] == pytest.approx(expected, abs=1e-6)


def test_a_broken_plugin_is_a_bullterrier_image_error_naming_it_and_bullterrier_image_plugins_builtin_skips_it(
    tmp_path,
):
    """One interpreter, three settings in turn: a failed load is not remembered, so each query tries again."""
    install(tmp_path, "zz-clash", "zz_clash", CLASHING_PACKAGE)
    report = fresh_python(
        tmp_path,
        """
        import json, os, sys
        from bullterrier_image import BullterrierImageError
        from bullterrier_image.core import registry

        report = {}
        for setting in ("everything", "", "builtin"):
            os.environ["BT_IMAGE_PLUGINS"] = setting
            try:
                report[setting] = {"effects": sorted(registry.describe())}
            except BullterrierImageError as error:
                report[setting] = {"error": str(error)}
        json.dump(report, sys.stdout)
        """,
    )
    assert "BT_IMAGE_PLUGINS" in report["everything"]["error"]
    assert "'everything'" in report["everything"]["error"]
    broken = report[""]["error"]
    for named in ("zz_clash", "zz-clash", "bloom", "BT_IMAGE_PLUGINS=builtin"):
        assert named in broken
    assert "bloom" in report["builtin"]["effects"]
    assert "zz_clash" not in " ".join(report["builtin"]["effects"])
