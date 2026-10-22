"""Contact sheets: many results side by side with labels, for choosing between experiments."""

import numpy as np
from PIL import Image, ImageDraw, ImageFont

from . import imageio


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
