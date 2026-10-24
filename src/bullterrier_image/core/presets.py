"""Presets: named parameter sets for one effect, shipped as data in `bullterrier_image/presets/<effect>.toml`.

    [sepia]                                  # one table per preset, named as the preset is
    description = "Warm brown monochrome."   # required, one sentence
    stops = ["#1a0f08", "#f4e4c1"]           # the other keys are the effect's parameters

`read` returns an effect's presets by name, each checked by the effect's own parameter model exactly as a recipe's
step is (a misspelt or out-of-range key, or `matte`, which a recipe supplies itself, is a `RecipeError` naming the
preset). A file is read and checked once, the first time its effect asks. An effect without a file has no presets.
The folder is a parameter only so that a test can read a throwaway one.

Dependency direction: imports `params` and `errors` only; the registry and `steps` build on it.
"""

import tomllib
from functools import cache
from importlib.resources import files
from importlib.resources.abc import Traversable
from typing import Any, NamedTuple

from bullterrier_image.errors import RecipeError

from .params import EffectParams

SHIPPED = files("bullterrier_image") / "presets"


class Preset(NamedTuple):
    """One preset: what it looks like in a sentence, and the parameters it sets."""

    description: str
    params: dict[str, Any]


@cache
def read(effect: str, model: type[EffectParams], folder: Traversable = SHIPPED) -> dict[str, Preset]:
    """The presets of the effect called `effect` (its parameter model is `model`) by name, in file order; empty when
    it has none. RecipeError when a preset lacks its `description` or `model` refuses its parameters."""
    file = folder / f"{effect}.toml"
    if not file.is_file():
        return {}
    presets = {}
    for name, table in tomllib.loads(file.read_text(encoding="utf-8")).items():
        owner = f"preset {name!r} of effect {effect!r}"
        params = dict(table)
        description = params.pop("description", "")
        if type(description) is not str or not description.strip():
            raise RecipeError(f"{owner} needs a `description`: one sentence saying what the look is")
        model.parse(params, owner)
        presets[name] = Preset(description, params)
    return presets
