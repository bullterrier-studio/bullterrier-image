"""Contact sheets: many results side by side with labels, for choosing between experiments. A result with
transparency (RGBA) is shown over a checkerboard."""

import numpy as np

from .core import native
from .ops.image import as_image

_RGBA = 4  # the channels of a picture with transparency


def contact_sheet(tiles: list[tuple[str, np.ndarray]], columns: int = 3, tile: int = 480) -> np.ndarray:
    """Lay out `(label, image)` pairs in a grid, each scaled to fit a `tile` px square, label underneath. A label wider
    than the tile is set smaller, and cut with `…` if it still does not fit. An image with transparency is laid over a
    checkerboard. The work is the core's."""
    return native.contact_sheet([(label, as_image(image, alpha=True)) for label, image in tiles], columns, tile)
