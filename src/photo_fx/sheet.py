"""Contact sheets: many results side by side with labels, for choosing between experiments."""

import re
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

from . import imageio
from .recipe import Stage


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
        small = imageio.to_pil(imageio.resize(image, tile))
        sheet.paste(small, (x + (tile - small.width) // 2, y + (tile - small.height) // 2))
        draw.text((x, y + tile + 6), label, fill="#d8d8d8", font=font)
    return imageio.from_pil(sheet)


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
