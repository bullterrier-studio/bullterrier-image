"""Reading a recipe from a small TOML file: `load_recipe(path)` returns a validated `Recipe`.

Promises: every error is a `RecipeError` that starts with the file's name, then names the step and the offending key
(a TOML syntax error keeps its line and column); nothing is read except that one file. The format (UTF-8 TOML):

    format = 1                     # optional: the recipe format version; this release reads 1 (the default)
    name = "green-terminal"        # optional (default: the file name, made safe): letters, digits, . _ -
    size = 1024                    # longest side in px; effect sizes (cell, radius...) are relative to this

    [frame]                        # optional: frame the photo before the steps, cut it out after them (see below)
    aspect = 1                     # crop to this width over height...
    anchor = "subject"             # ...around the subject
    transparent = "circle"         # a circle cut-out: the render has an alpha channel (PNG or WebP, not JPEG)

    [[step]]
    effect = "ascii"               # any effect from bullterrier_image.effects; the other keys are its parameters
    cell = 9
    ink = "#00ff9c"

    [[step]]
    effect = "fill"
    color = "#000000"
    mask = "background"            # "subject" or "background": apply only there (needs a clear subject in the photo)
    opacity = 0.3                  # optional, 0..1: mix the effect in partway (default 1 = fully)

    [[step]]
    effect = "gradient"
    blend_mode = "screen"          # optional: how the result combines with the picture below (default "normal"):
                                   # normal, multiply, screen, overlay, soft_light, difference or color

    [[step]]
    effect = "gradient_map"
    preset = "sepia"               # optional: one of the effect's presets (`bt-image effects NAME` lists them); the
    opacity = 0.6                  # step's own keys win over the preset's

A parameter that is a picture (a texture, a logo; the effect's documentation says which, and an effect with one calls
it `image`) is written as a path, relative to the recipe file's folder, or as a colour (a string starting with `#`, or
three numbers):

    [[step]]
    effect = "blend"
    image = "../assets/grain.png"

The picture is read once, when the recipe is loaded; a file that is missing or not a picture fails there, naming the
file, the step, the parameter and the path.

The `[frame]` table holds the fields of `Framing` (`Framing.documentation()` lists them with their units and ranges):
`flip`, `angle` (with `fit` and `resample`), `aspect` (with `anchor` and `margin`), `pad_top`, `pad_right`, `pad_bottom`
and `pad_left` (with `fill`), applied to the photo and the subject matte in that order, before the first step and
before `size`; and `transparent` (with `corner_radius`), applied after the last step. The transparency lives in the
table with the geometry because it is the same model, `Recipe(..., frame=Framing(...))` in Python: one place says how a
recipe's picture is framed and cut. Any other top-level key is refused, so a misspelt `size` or `format` is not
silently ignored; so is any other key in `[frame]`.

Dependency direction: imports `core` (steps, framing, registry, params), `imageio` and `errors`; it does not know
`runner`.
"""

import tomllib
from collections.abc import Mapping
from os import PathLike
from pathlib import Path

import numpy as np

from bullterrier_image.errors import BullterrierImageError, ImageReadError, RecipeError

from . import imageio
from .core import registry
from .core.framing import Framing
from .core.params import Picture
from .core.steps import Recipe, Step, safe_name

__all__ = ["load_recipe", "with_pictures"]

# The recipe format versions this release reads; a file without `format` is the first. A release that changes the
# format adds its number here and keeps reading the old ones.
_FORMATS = (1,)
# the top-level keys of a recipe file, each as it is written (a table or a list of tables in brackets), in the order the
# error for an unknown one lists them
_KEYS = {"format": "format", "name": "name", "size": "size", "frame": "[frame]", "step": "[[step]]"}


def _located[E: BullterrierImageError](error: E, where: str) -> E:
    """`error` with `where` (the file, and the step) in front, keeping its class (an unknown effect stays an
    `UnknownEffectError`, a missing picture an `ImageReadError`)."""
    return type(error)(f"{where}: {error}")


def with_pictures(effect: str, values: Mapping[str, object], folder: Path) -> dict[str, object]:
    """`values` (the parameters of `effect`) with each picture given as a path or an array turned into a `Picture`.

    A path (a `str` not starting with `#`, which is a colour, or a `PathLike`) is read with `imageio.load_picture`,
    relative to `folder` and labelled as written; a 3-d array is a `Picture` labelled "array". A `Picture`, a colour and
    every other parameter pass unchanged. ImageReadError naming the effect, the parameter and the path when a file
    cannot be read; an unknown effect is the registry's UnknownEffectError.
    """
    pictures = registry.parameters(effect).picture_fields()
    resolved = dict(values)
    for key in pictures & values.keys():
        value = values[key]
        if isinstance(value, PathLike) or (isinstance(value, str) and not value.startswith("#")):
            try:
                resolved[key] = Picture(imageio.load_picture(folder / value), str(value))
            except ImageReadError as error:
                raise ImageReadError(f"effect {effect!r}: `{key}`: {error}") from None
        elif isinstance(value, np.ndarray) and value.ndim == 3:
            resolved[key] = Picture(value)
    return resolved


def _read(path: Path) -> dict:
    """The file's top-level table, checked: UTF-8 TOML, a format this release reads, and only the known keys."""
    try:
        with path.open("rb") as f:
            raw = tomllib.load(f)
    except tomllib.TOMLDecodeError as error:  # its message keeps the line and column
        raise RecipeError(f"{path.name}: {error}") from None
    except UnicodeDecodeError as error:
        raise RecipeError(f"{path.name}: not UTF-8 text ({error.reason} at byte {error.start})") from None
    version = raw.get("format", _FORMATS[0])
    if type(version) is not int or version not in _FORMATS:  # not `in` alone: true == 1 and 1.0 == 1 in Python
        raise RecipeError(
            f"{path.name}: `format` {version!r} is not one this release reads; it reads format "
            f"{', '.join(map(str, _FORMATS))}"
        )
    unknown = sorted(set(raw) - set(_KEYS))
    if unknown:
        raise RecipeError(f"{path.name}: `{unknown[0]}` is not a recipe key; the keys are: {', '.join(_KEYS.values())}")
    return raw


def load_recipe(path: str | Path) -> Recipe:
    """Read a recipe file and return the validated `Recipe`.

    path: a TOML file in the format above. A `name` it sets is used as written (and refused if it is not a safe file
    name); without one the recipe is named after the file, made safe (see `steps.safe_name`: "my look.toml" gives
    "my-look"), and a file name that leaves nothing usable ("日本.toml", "con.toml") is refused, asking for a `name`.
    Raises RecipeError (a ValueError; an unknown effect is an UnknownEffectError) for anything invalid, starting with
    the file's name, then the step and the offending key; OSError (FileNotFoundError...) if the file cannot be opened.
    """
    path = Path(path)
    raw = _read(path)
    recipe_name = raw["name"] if "name" in raw else safe_name(path.stem)
    if not recipe_name:
        raise RecipeError(
            f'{path.name}: the file name gives no usable recipe name: add name = "..." (letters, digits, . _ -) or '
            "rename the file"
        )
    table = raw.get("frame", {})
    if not isinstance(table, dict):
        raise RecipeError(
            f"{path.name}: write the framing as a [frame] table (one: its keys are the fields of Framing)"
        )
    try:
        frame = Framing(**table)
    except RecipeError as error:
        raise _located(error, f"{path.name}: [frame]") from None
    entries = raw.get("step", [])
    if not isinstance(entries, list) or not all(isinstance(entry, dict) for entry in entries):
        raise RecipeError(f"{path.name}: write each step as a [[step]] table (two brackets: there may be several)")
    steps = []
    for i, entry in enumerate(entries, start=1):
        where = f"{path.name}: step {i}"
        entry = dict(entry)
        keys = {key: entry.pop(key) for key in registry.STEP_KEYS if key in entry}  # the rest are the parameters
        if "effect" not in keys:
            raise RecipeError(f"{where}: missing `effect`")
        try:
            steps.append(Step(**keys, params=with_pictures(keys["effect"], entry, path.parent)))
        except (RecipeError, ImageReadError) as error:
            raise _located(error, where) from None
    try:
        return Recipe(recipe_name, tuple(steps), raw.get("size", 1024), frame)
    except RecipeError as error:
        raise _located(error, path.name) from None
