"""Presets: named parameter sets for one effect or region kind, shipped in `presets/<name>.toml`.

    [sepia]                                  # one table per preset, named as the preset is
    description = "Warm brown monochrome."   # required, one sentence
    stops = ["#1a0f08", "#f4e4c1"]           # the other keys are the effect's parameters

`read` returns the presets of an effect or region kind by name, each checked by its own parameter model exactly as a
recipe's step is (a misspelt or out-of-range key, or `matte`, which a recipe supplies itself, is a `RecipeError` naming
the preset). A file is read and checked once, the first time it is asked for. Without a file there are no presets.
The folder is a parameter only so that a test can read a throwaway one.

Dependency direction: imports `params` and `errors` only; the registry and `steps` build on it.
"""

import tomllib
from functools import cache
from pathlib import Path
from typing import Any, NamedTuple

from bullterrier_image.errors import RecipeError

from .params import EffectParams

SHIPPED = Path(__file__).parent.parent / "presets"


class Preset(NamedTuple):
    """One preset: what it looks like in a sentence, and the parameters it sets."""

    description: str
    params: dict[str, Any]


@cache
def read(name: str, model: type[EffectParams], folder: Path = SHIPPED) -> dict[str, Preset]:
    """The presets of the effect or region kind called `name` (its parameter model is `model`) by name, in file order;
    empty when it has none. RecipeError when a preset lacks its `description` or `model` refuses its parameters."""
    file = folder / f"{name}.toml"
    if not file.is_file():
        return {}
    presets = {}
    for preset, table in tomllib.loads(file.read_text(encoding="utf-8")).items():
        owner = f"preset {preset!r} of {name!r}"
        params = dict(table)
        description = params.pop("description", "")
        if type(description) is not str or not description.strip():
            raise RecipeError(f"{owner} needs a `description`: one sentence saying what the look is")
        model.parse(params, owner)
        presets[preset] = Preset(description, params)
    return presets
