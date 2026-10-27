"""The `tile` effect: the whole picture, scaled down, repeated in a grid of columns by rows, the grid slid around."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.shift import shift


class TileParams(EffectParams):
    """The settings of `tile`: the shape of the grid and how far it is slid."""

    columns: int = param(
        3,
        unit="count",
        min=1,
        max=100,
        doc="How many copies of the picture sit side by side, 1 to 100; 1 keeps the picture whole across. Each copy is "
        "the width of the picture divided by this, rounded down (at least 1 px).",
    )
    rows: int = param(
        3,
        unit="count",
        min=1,
        max=100,
        doc="How many copies sit one above another, 1 to 100; 1 keeps the picture whole down. Each copy is the height "
        "of the picture divided by this, rounded down (at least 1 px).",
    )
    offset_x: float = param(
        0.0,
        unit="fraction",
        min=-1,
        max=1,
        doc="How far the copies slide sideways, as a fraction of one copy's width, -1 to 1: positive moves them to "
        "the right, negative to the left, and what leaves one side of a copy comes back on the other; 0 leaves them "
        "where they are, and so does a whole copy (1 or -1). A distance that ends between two pixels blends the two.",
    )
    offset_y: float = param(
        0.0,
        unit="fraction",
        min=-1,
        max=1,
        doc="How far the copies slide vertically, as a fraction of one copy's height, -1 to 1: positive moves them "
        "down, negative up, and what leaves the bottom of a copy comes back at its top; 0 leaves them where they are, "
        "and so does a whole copy (1 or -1). A distance that ends between two pixels blends the two.",
    )


def _box_weights(source: int, target: int) -> np.ndarray:
    """The (target, source) matrix that averages `source` pixels down to `target` by area: row i holds, for each
    source pixel, the share of the i-th of `target` equal stretches of the line that the pixel covers."""
    edges = np.arange(target + 1) * (source / target)  # where each target pixel starts and ends, in source pixels
    pixels = np.arange(source)
    covered = np.minimum(edges[1:, None], pixels + 1) - np.maximum(edges[:-1, None], pixels)
    return (np.clip(covered, 0, None) / (source / target)).astype(np.float32)


@effect(category="distortion", params=TileParams)
def tile(rgb: np.ndarray, params: TileParams) -> np.ndarray:
    """Repeat the picture scaled down in a grid, or slide it around: a rolling scan, a wrap-around shift.

    The picture is averaged down by area (each new pixel is the mean of the source pixels it covers, fractions of a
    pixel counted in part) to a cell of whole pixels, width // columns by height // rows and at least 1 px each. The
    cell is slid by `offset_x` and `offset_y` of its size, wrapping around within the cell (a fraction of a pixel
    blends the two pixels it lies between), and repeated across the picture. When the grid does not divide the
    picture, the leftover pixels (width mod columns on the right, height mod rows at the bottom) show the start of the
    next copy, cut off. One column and one row give the picture itself, or with an offset the picture rolled around, as
    on a television whose sync slipped (the `scan_roll` preset).
    """
    height, width = rgb.shape[:2]
    cell_height, cell_width = max(1, height // params.rows), max(1, width // params.columns)
    down = _box_weights(height, cell_height) @ rgb.reshape(height, width * rgb.shape[2])
    cell = np.tensordot(_box_weights(width, cell_width), down.reshape(cell_height, width, rgb.shape[2]), axes=(1, 1))
    cell = cell.transpose(1, 0, 2)  # (cell_height, cell_width, 3)
    cell = shift(cell, params.offset_y * cell_height, params.offset_x * cell_width, edge="wrap")
    rows = np.arange(height) % cell_height
    columns = np.arange(width) % cell_width
    return np.clip(cell[rows[:, None], columns[None, :]], 0, 1).astype(np.float32)
