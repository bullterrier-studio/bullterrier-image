"""Reading a recipe from a small TOML file: `load_recipe(path)` returns a validated `Recipe`.

Promises: every error is a `RecipeError` that starts with the file's name, then names the step and the offending key
(a TOML syntax error keeps its line and column); nothing is read except that one file. The format (UTF-8 TOML):

    format = 1                     # optional: the recipe format version; this release reads 1 (the default)
    name = "green-terminal"        # optional (default: the file name, made safe): letters, digits, . _ -
    size = 1024                    # longest side in px; effect sizes (cell, radius...) are relative to this

    [[step]]
    effect = "ascii"               # any effect from bullterrier_image.effects; the other keys are its parameters
    cell = 9
    ink = "#00ff9c"

    [[step]]
    effect = "fill"
    color = "#000000"
    mask = "background"            # "subject" or "background": apply only there (needs a clear subject in the photo)
    opacity = 0.3                  # optional, 0..1: blend the effect in partway (default 1 = fully)

    [[step]]
    effect = "gradient_map"
    preset = "sepia"               # optional: one of the effect's presets (`bt-image effects NAME` lists them); the
    opacity = 0.6                  # step's own keys win over the preset's

Removing the background is not a feature of its own: it is a `fill` step masked to the background. Any other top-level
key is refused, so a misspelt `size` or `format` is not silently ignored.

Dependency direction: imports `steps` and `errors` only; it does not know `runner`.
"""

import tomllib
from pathlib import Path

from bullterrier_image.errors import RecipeError

from .steps import Recipe, Step, safe_name

# The recipe format versions this release reads; a file without `format` is the first. A release that changes the
# format adds its number here and keeps reading the old ones.
_FORMATS = (1,)
_KEYS = ("format", "name", "size", "step")


def _located(error: RecipeError, where: str) -> RecipeError:
    """`error` with `where` (the file, and the step) in front, keeping its class (an unknown effect stays an
    `UnknownEffectError`)."""
    return type(error)(f"{where}: {error}")


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
        raise RecipeError(
            f"{path.name}: `{unknown[0]}` is not a recipe key; the keys are: format, name, size, [[step]]"
        )
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
    entries = raw.get("step", [])
    if not isinstance(entries, list) or not all(isinstance(entry, dict) for entry in entries):
        raise RecipeError(f"{path.name}: write each step as a [[step]] table (two brackets: there may be several)")
    steps = []
    for i, entry in enumerate(entries, start=1):
        where = f"{path.name}: step {i}"
        entry = dict(entry)
        name = entry.pop("effect", None)
        if name is None:
            raise RecipeError(f"{where}: missing `effect`")
        mask, opacity, preset = entry.pop("mask", None), entry.pop("opacity", 1.0), entry.pop("preset", None)
        try:
            steps.append(Step(name, entry, mask, opacity, preset))
        except RecipeError as error:
            raise _located(error, where) from None
    try:
        return Recipe(recipe_name, tuple(steps), raw.get("size", 1024))
    except RecipeError as error:
        raise _located(error, path.name) from None
