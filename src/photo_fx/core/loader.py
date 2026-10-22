"""Reading a recipe from a small TOML file: `load_recipe(path)` returns a validated `Recipe`.

Promises: every error names the file, the step and the offending key; nothing is read except that one file. The format:

    name = "green-terminal"
    size = 1024                    # longest side in px; effect sizes (cell, radius...) are relative to this

    [[step]]
    effect = "ascii"               # any effect from photo_fx.effects; the other keys are its parameters
    cell = 9
    fg = "#00ff9c"

    [[step]]
    effect = "fill"
    color = "#000000"
    mask = "background"            # "subject" or "background": apply only there (needs a clear subject in the photo)
    opacity = 0.3                  # optional, 0..1: blend the effect in partway (default 1 = fully)

Removing the background is not a feature of its own: it is a `fill` step masked to the background.

A recipe becomes an animation with an [animation] table, and any numeric parameter can then change over the clip (a
whole-number parameter moves in whole steps, rounded to the nearest; every frame is checked against the range when
the recipe loads):

    [animation]
    seconds = 5
    fps = 30
    hold_start = 0.5               # optional: seconds the first frame is held
    hold_end = 0.5                 # optional: seconds the last frame is held
    loop = "pingpong"              # optional: "once" (default), or play forward then backward for a seamless loop

    [[step]]
    effect = "disperse"
    progress = { from = 0.0, to = 1.0, ease = "gentle" }

`ease` is a name (linear, the default; in, out, in-out; or the cubic-bezier presets gentle, standard, accelerate,
decelerate, emphasized, sine) or your own cubic-bezier as four numbers, `ease = [0.4, 0.0, 0.2, 1.0]`. Spatial motion
should not stay linear. A value normally moves only while the animation itself runs (after `hold_start`, before
`hold_end`); add `over = "clip"` to make it run across the whole clip including the holds, e.g. a clock
for ambient motion.

Dependency direction: imports `steps` and `timing` only; it does not know `runner`.
"""

import tomllib
from pathlib import Path

from .errors import RecipeError
from .steps import Recipe, Step
from .timing import EASE_NAMES, Animated, Animation


def _located(error: ValueError, where: str) -> RecipeError:
    """`error` with `where` (the file, and the step) in front; a `RecipeError` keeps its class (an unknown effect stays
    an `UnknownEffectError`), anything else (timing's plain `ValueError`) becomes a `RecipeError`."""
    kind = type(error) if isinstance(error, RecipeError) else RecipeError
    return kind(f"{where}: {error}")


def _animated_value(value: object, where: str, key: str) -> object:
    """Turn `{ from, to, ease, over }` into an Animated; any other value is returned unchanged."""
    if not isinstance(value, dict):
        return value
    if set(value) - {"from", "to", "ease", "over"} or not {"from", "to"} <= set(value):
        raise RecipeError(
            f"{where}: `{key}` must be {{ from = number, to = number, ease = one of {', '.join(EASE_NAMES)} or "
            f'[x1, y1, x2, y2] with x1, x2 in 0..1, over = "motion" or "clip" }}'
        )
    try:
        return Animated(value["from"], value["to"], value.get("ease", "linear"), value.get("over", "motion"))
    except ValueError as error:
        raise _located(error, f"{where}: `{key}`") from None


def _load_animation(table: dict, filename: str) -> Animation:
    unknown = set(table) - {"seconds", "fps", "hold_start", "hold_end", "loop"}
    if unknown or "seconds" not in table:
        raise RecipeError(
            f"{filename}: [animation] needs `seconds`; allowed keys: seconds, fps, hold_start, hold_end, loop"
        )
    try:
        return Animation(**table)
    except ValueError as error:
        raise _located(error, f"{filename}: [animation]") from None


def load_recipe(path: str | Path) -> Recipe:
    """Read a recipe file and return the validated `Recipe`.

    path: a TOML file in the format above. The recipe is named after the file (without `.toml`) unless it sets `name`.
    Raises ValueError for anything invalid, naming the file, the step and the offending key.
    """
    path = Path(path)
    with open(path, "rb") as f:
        raw = tomllib.load(f)
    animation = _load_animation(raw["animation"], path.name) if "animation" in raw else None
    steps = []
    for i, entry in enumerate(raw.get("step", []), start=1):
        where = f"{path.name}: step {i}"
        entry = dict(entry)
        name = entry.pop("effect", None)
        if name is None:
            raise RecipeError(f"{where}: missing `effect`")
        mask, opacity = entry.pop("mask", None), entry.pop("opacity", 1.0)
        params = {key: _animated_value(value, where, key) for key, value in entry.items()}
        try:
            steps.append(Step(name, params, mask, opacity))
        except ValueError as error:
            raise _located(error, where) from None
    try:
        return Recipe(raw.get("name", path.stem), tuple(steps), raw.get("size", 1024), animation)
    except ValueError as error:
        raise _located(error, path.name) from None
