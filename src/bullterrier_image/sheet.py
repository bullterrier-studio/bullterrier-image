"""Contact sheets: many results side by side with labels, for choosing between experiments."""

import re
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

from . import imageio
from .core import registry
from .errors import RecipeError
from .ops.resample import resize
from .recipe import Recipe, Stage, Step


def contact_sheet(tiles: list[tuple[str, np.ndarray]], columns: int = 3, tile: int = 480) -> np.ndarray:
    """Lay out `(label, image)` pairs in a grid, each scaled to fit a `tile` px square, label underneath."""
    gap, label_h = 14, 30
    rows = -(-len(tiles) // columns)
    cols = min(columns, len(tiles))
    sheet = Image.new("RGB", (gap + cols * (tile + gap), gap + rows * (tile + label_h + gap)), "#242424")
    draw = ImageDraw.Draw(sheet)
    font = ImageFont.load_default(size=16)
    for i, (label, image) in enumerate(tiles):
        x = gap + (i % columns) * (tile + gap)
        y = gap + (i // columns) * (tile + label_h + gap)
        small = imageio.to_pil(resize(image, tile))
        sheet.paste(small, (x + (tile - small.width) // 2, y + (tile - small.height) // 2))
        draw.text((x, y + tile + 6), label, fill="#d8d8d8", font=font)
    return imageio.from_pil(sheet)


def sweep(recipe: Recipe, target: str, start: float, end: float, count: int = 5) -> list[tuple[str, Recipe]]:
    """`recipe` once for each of `count` values of one parameter, evenly spaced from `start` to `end`, each with the
    label `parameter=value`: the tiles of a sheet that shows what the parameter does.

    target: `effect.parameter`, e.g. `glitch.split`; `effect#2.parameter` picks the second step of an effect the recipe
    has twice (the number is required then). The parameter must be a number. The value replaces whatever the recipe
    has there. A whole-number parameter takes the values rounded to the nearest whole number, each once. count: 2 to
    24. A value outside the parameter's range is refused as in any recipe, naming it, never clamped.

    Raises RecipeError for a target the recipe does not have (naming what it does have), a parameter the effect does
    not have or that is not a number (naming the valid ones), a count outside 2 to 24, a start equal to the end, or a
    value outside the range.
    """
    if not 2 <= count <= 24:
        raise RecipeError(f"count must be 2 to 24 tiles, got {count}")
    if start == end:
        raise RecipeError(f"a sweep needs a range: start and end are both {start:g}")
    index, name = _locate(recipe, target)
    step = recipe.steps[index]
    values = np.linspace(start, end, count).tolist()
    if name in registry.parameters(step.effect).whole_number_fields():
        values = list(dict.fromkeys(round(value) for value in values))
    tiles = []
    for value in values:
        swept = Step(step.effect, {**step.params, name: value}, step.mask, step.opacity)
        steps = [*recipe.steps[:index], swept, *recipe.steps[index + 1 :]]
        tiles.append((f"{name}={value:g}", recipe.model_copy(update={"steps": tuple(steps)})))  # keeps the rest
    return tiles


def _locate(recipe: Recipe, target: str) -> tuple[int, str]:
    """The index of the step and the name of the parameter that `target` (see `sweep`) points at. The parameter is
    checked by the effect's model when the tile is made, which names it and the valid ones."""
    found = re.fullmatch(r"(\w+)(?:#(\d+))?\.(\w+)", target)
    if not found:
        raise RecipeError(f"sweep target {target!r} is not `effect.parameter` (or `effect#2.parameter`)")
    effect, number, name = found.groups()
    steps = [i for i, step in enumerate(recipe.steps) if step.effect == effect]
    if not steps:
        have = ", ".join(dict.fromkeys(step.effect for step in recipe.steps))
        raise RecipeError(f"sweep target {target!r}: recipe {recipe.name!r} has no {effect!r} step; it has: {have}")
    if number is None and len(steps) > 1:
        raise RecipeError(
            f"sweep target {target!r}: recipe {recipe.name!r} has {len(steps)} {effect!r} steps; "
            f"choose one, from {effect}#1.{name} to {effect}#{len(steps)}.{name}"
        )
    pick = int(number or 1)
    if not 1 <= pick <= len(steps):
        raise RecipeError(
            f"sweep target {target!r}: recipe {recipe.name!r} has {len(steps)} {effect!r} steps, no #{pick}"
        )
    return steps[pick - 1], name


def write_stages(stages: list[Stage], folder: str | Path, quality: int = 92) -> Path:
    """Save every stage of a `trace` as `NN-name.jpg` in `folder`, plus `_sheet.jpg` showing them all in order.

    Returns the path of the sheet, the one picture to look at to see what each step did.
    """
    folder = Path(folder)
    folder.mkdir(parents=True, exist_ok=True)
    for number, stage in enumerate(stages):
        imageio.save(stage.image, folder / f"{number:02d}-{_stage_name(stage.label)}.jpg", quality=quality)
    sheet = folder / "_sheet.jpg"
    columns = min(4, len(stages))
    imageio.save(contact_sheet([(s.label, s.image) for s in stages], columns=columns, tile=420), sheet, quality=quality)
    return sheet


def _stage_name(label: str) -> str:
    """A file-name-friendly name for a stage label: 'input', 'matte', or the step's effect."""
    if label.startswith("subject matte"):
        return "matte"
    found = re.match(r"\d+\. (\w+)", label)
    return found.group(1) if found else re.sub(r"[^a-z0-9]+", "-", label.lower()).strip("-")
