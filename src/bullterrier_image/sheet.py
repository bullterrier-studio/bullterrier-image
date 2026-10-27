"""Contact sheets: many results side by side with labels, for choosing between experiments. A result with
transparency (RGBA) is shown over a checkerboard, and a stage saved with transparency is a PNG."""

import re
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

from . import imageio
from .errors import RecipeError
from .ops import glyphs
from .ops.resample import resize
from .recipe import Recipe, Stage

_RGBA = 4  # the channels of a picture with transparency
_CHECKER = 8  # px: the side of a square of the checkerboard that shows through transparency
_LIGHT, _DARK = 0.8, 0.6  # the greys of the checkerboard, as in the boards image editors show
_LABEL_SIZE, _LABEL_SMALLEST = 16, 10  # px: the label's type size, and the smallest it shrinks to before it is cut


def _over_checkerboard(image: np.ndarray) -> np.ndarray:
    """`image` as RGB: a picture with transparency (four channels, straight alpha last) laid over a checkerboard, so
    that what is clear shows as the board and not as the colour hidden under it; an RGB picture as it is."""
    if image.shape[2] != _RGBA:
        return image
    rows, columns = np.indices(image.shape[:2]) // _CHECKER
    board = np.where((rows + columns) % 2 == 0, _LIGHT, _DARK).astype(np.float32)[..., None]
    alpha = image[..., 3:]
    return image[..., :3] * alpha + board * (1.0 - alpha)


def _fitted_label(label: str, width: int) -> tuple[ImageFont.FreeTypeFont, str]:
    """The font and the text that draw `label` inside `width` px: the label at the type size when it fits, else at the
    largest smaller whole size that fits (down to the smallest), else cut at the smallest size and ended with `…`."""
    for size in range(_LABEL_SIZE, _LABEL_SMALLEST - 1, -1):
        font = ImageFont.truetype(glyphs.FONT, size)
        if font.getlength(label) <= width:
            return font, label
    while label and font.getlength(f"{label}…") > width:
        label = label[:-1]
    return font, f"{label}…"


def contact_sheet(tiles: list[tuple[str, np.ndarray]], columns: int = 3, tile: int = 480) -> np.ndarray:
    """Lay out `(label, image)` pairs in a grid, each scaled to fit a `tile` px square, label underneath. A label wider
    than the tile is set smaller, and cut with `…` if it still does not fit. An image with transparency is laid over a
    checkerboard."""
    gap, label_h = 14, 30
    rows = -(-len(tiles) // columns)
    cols = min(columns, len(tiles))
    sheet = Image.new("RGB", (gap + cols * (tile + gap), gap + rows * (tile + label_h + gap)), "#242424")
    draw = ImageDraw.Draw(sheet)
    for i, (label, image) in enumerate(tiles):
        x = gap + (i % columns) * (tile + gap)
        y = gap + (i // columns) * (tile + label_h + gap)
        small = imageio.to_pil(_over_checkerboard(resize(image, tile)))
        sheet.paste(small, (x + (tile - small.width) // 2, y + (tile - small.height) // 2))
        font, text = _fitted_label(label, tile)
        draw.text((x, y + tile + 6), text, fill="#d8d8d8", font=font)
    return imageio.from_pil(sheet)


def sweep(recipe: Recipe, target: str, start: float, end: float, count: int = 5) -> list[tuple[str, Recipe]]:
    """`recipe` once for each of `count` values of one number, evenly spaced from `start` to `end`, each with the label
    `number=value`: the tiles of a sheet that shows what the number does.

    target: the number's address (`Recipe.parameters` lists them): `rgb_split.amount`, `grain#2.amount` for the second
    of an effect the recipe has twice, `pulse.gain` for a step called `pulse`, `pulse.gain.end` for the end of a drive,
    `region.near.low`, `frame.angle`. The value replaces whatever the recipe has there. A whole-number parameter takes
    the values rounded to the nearest whole number, each once. count: 2 to 24. A value outside the number's range is
    refused as in any recipe, naming it, never clamped; a cyclic angle takes any value and wraps it into its turn.

    Raises RecipeError for a target the recipe does not have (naming what it does have), a count outside 2 to 24, a
    start equal to the end, or a value outside the range.
    """
    if not 2 <= count <= 24:
        raise RecipeError(f"count must be 2 to 24 tiles, got {count}")
    if start == end:
        raise RecipeError(f"a sweep needs a range: start and end are both {start:g}")
    number = recipe.parameter(target)
    values = np.linspace(start, end, count).tolist()
    if number.whole:
        values = list(dict.fromkeys(round(value) for value in values))
    label = target.partition(".")[2]
    return [(f"{label}={value:g}", recipe.with_values({target: value})) for value in values]


def write_stages(stages: list[Stage], folder: str | Path, quality: int = 92) -> Path:
    """Save every stage of a `trace` as `NN-name.jpg` in `folder` (`.png` for a stage with transparency, which a JPEG
    cannot hold), plus `_sheet.jpg` showing them all in order, a stage with transparency over a checkerboard.

    Returns the path of the sheet, the one picture to look at to see what each step did.
    """
    folder = Path(folder)
    folder.mkdir(parents=True, exist_ok=True)
    for number, stage in enumerate(stages):
        extension = "png" if stage.image.shape[2] == _RGBA else "jpg"
        imageio.save(stage.image, folder / f"{number:02d}-{_stage_name(stage.label)}.{extension}", quality=quality)
    sheet = folder / "_sheet.jpg"
    columns = min(4, len(stages))
    imageio.save(contact_sheet([(s.label, s.image) for s in stages], columns=columns, tile=420), sheet, quality=quality)
    return sheet


def _stage_name(label: str) -> str:
    """A file-name-friendly name for a stage label: 'input', 'matte', 'depth', or the step's effect."""
    if label.startswith("subject matte"):
        return "matte"
    if label.startswith("depth ("):
        return "depth"
    found = re.match(r"\d+\. (\w+)", label)
    return found.group(1) if found else re.sub(r"[^a-z0-9]+", "-", label.lower()).strip("-")
