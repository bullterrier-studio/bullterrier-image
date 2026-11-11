"""Presets: named parameter sets for one effect, shipped in the core (`crates/core/data/presets/<name>.toml`).

    [sepia]                                  # one table per preset, named as the preset is
    description = "Warm brown monochrome."   # required, one sentence
    stops = ["#1a0f08", "#f4e4c1"]           # the other keys are the effect's parameters

`read` returns the presets of an effect by name, each checked by the core against the effect's parameters exactly as a
recipe's step is (a misspelt or out-of-range key, or `subject`, which a recipe supplies itself, is a `RecipeError`
naming the preset). A file is read and checked once, the first time it is asked for. Without a file there are no
presets. The folder is a parameter only so that a test can read a throwaway one; none reads the files the core ships.

Dependency direction: imports `native` and `errors` only; the registry and `steps` build on it.
"""

import tomllib
from functools import cache
from pathlib import Path
from typing import Any, NamedTuple

from bullterrier_image.errors import RecipeError

from . import native


class Preset(NamedTuple):
    """One preset: what it looks like in a sentence, and the parameters it sets."""

    description: str
    params: dict[str, Any]


@cache
def read(name: str, folder: Path | None = None) -> dict[str, Preset]:
    """The presets of the effect called `name` (registered) by name, in file order;
    empty when it has none. RecipeError when a preset lacks its `description` or the effect refuses its parameters."""
    if folder is None:
        text = native.shipped_presets().get(name)
    else:
        file = folder / f"{name}.toml"
        text = file.read_text(encoding="utf-8") if file.is_file() else None
    if text is None:
        return {}
    presets = {}
    for preset, table in tomllib.loads(text).items():
        owner = f"preset {preset!r} of {name!r}"
        params = dict(table)
        description = params.pop("description", "")
        if type(description) is not str or not description.strip():
            raise RecipeError(f"{owner} needs a `description`: one sentence saying what the look is")
        try:
            native.check(name, params)
        except RecipeError as error:
            raise RecipeError(f"{owner}: {error}") from None
        presets[preset] = Preset(description, params)
    return presets
