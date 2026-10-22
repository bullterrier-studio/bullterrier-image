"""Shared by effects that break the picture into character-sized pieces (evaporate, disperse)."""

import numpy as np
from scipy import ndimage

from .ascii import cell_size


def cut(rgb: np.ndarray, cell: int) -> tuple[np.ndarray, int, int]:
    """Cut `rgb` into pieces the size of one `ascii(cell=cell)` character.

    Returns (pieces, cell_w, cell_h); pieces has shape (rows, cols, cell_h, cell_w, 3). The picture is padded with
    black on the right and bottom to a whole number of pieces.
    """
    height, width = rgb.shape[:2]
    cell_w, cell_h = cell_size(cell)
    rows, cols = -(-height // cell_h), -(-width // cell_w)
    padded = np.pad(rgb, ((0, rows * cell_h - height), (0, cols * cell_w - width), (0, 0)))
    return padded.reshape(rows, cell_h, cols, cell_w, 3).transpose(0, 2, 1, 3, 4), cell_w, cell_h


def join(pieces: np.ndarray, height: int, width: int) -> np.ndarray:
    """Put pieces from `cut` back together and crop to the original size."""
    rows, cols, cell_h, cell_w, _ = pieces.shape
    return pieces.transpose(0, 2, 1, 3, 4).reshape(rows * cell_h, cols * cell_w, 3)[:height, :width].astype(np.float32)


def lit(pieces: np.ndarray) -> np.ndarray:
    """(rows, cols) bool: pieces with any content. Pieces of black background are not worth moving."""
    return pieces.max(axis=(2, 3, 4)) > 0.02


def figure(
    lit_pieces: np.ndarray, matte: np.ndarray | None, cell_w: int, cell_h: int, height: int, width: int
) -> np.ndarray:
    """(rows, cols) bool: the pieces that belong to the person.

    From the person `matte` when there is one, so dark parts of the figure (eyes, shadows) still count. Without
    one, the lit pieces with enclosed gaps filled in.
    """
    if matte is None:
        return ndimage.binary_fill_holes(lit_pieces)
    rows, cols = lit_pieces.shape
    padded = np.pad(matte, ((0, rows * cell_h - height), (0, cols * cell_w - width)))
    return padded.reshape(rows, cell_h, cols, cell_w).mean(axis=(1, 3)) > 0.5


def outline(shape: np.ndarray, cell_w: int, cell_h: int) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """For each piece: its distance in px from the nearest edge of the figure, and the unit vector pointing out of it.

    `shape` marks the pieces that belong to the figure. Edges are its boundary, plus the left, right and top borders
    of the picture; the bottom border is not an edge. The direction is (x, y) with y pointing down; it points up
    where the distance is flat (the middle of a shape), because up is where evaporation goes by default.
    """
    grid = np.pad(shape, ((1, 0), (1, 1)), constant_values=False)
    grid = np.pad(grid, ((0, 1), (0, 0)), mode="edge")  # the figure carries on past the bottom
    inside = ndimage.distance_transform_edt(grid, sampling=(cell_h, cell_w))[1:-1, 1:-1]
    smooth = ndimage.gaussian_filter(inside, 1.0)
    grad_y, grad_x = np.gradient(smooth, cell_h, cell_w)
    length = np.hypot(grad_x, grad_y)
    flat = length < 1e-6
    normal_x = np.where(flat, 0.0, -grad_x / np.where(flat, 1.0, length))
    normal_y = np.where(flat, -1.0, -grad_y / np.where(flat, 1.0, length))
    return inside, normal_x, normal_y
