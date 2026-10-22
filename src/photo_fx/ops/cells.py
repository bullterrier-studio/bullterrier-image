"""Cut a picture into a grid of equal pieces (for example character-sized ones) and put it back together.

Pieces are sized in pixels, so this module knows nothing about fonts or characters: callers pass the cell size in px.
A piece grid is an array of shape (rows, cols, cell_h, cell_w, 3). `cut` and `join` are exact inverses; `lit` and
`figure` summarise a grid as one boolean per piece.
"""

import numpy as np
from scipy import ndimage


def cut(rgb: np.ndarray, cell_w: int, cell_h: int) -> np.ndarray:
    """Cut `rgb` into a grid of pieces.

    `rgb` is a float32 (height, width, 3) picture. `cell_w` and `cell_h` are the size of one piece in px (both >= 1,
    else ValueError). Returns an array of shape (rows, cols, cell_h, cell_w, 3) where rows = ceil(height / cell_h) and
    cols = ceil(width / cell_w); the picture is padded with black on the right and bottom to a whole number of pieces.
    """
    if cell_w < 1 or cell_h < 1:
        raise ValueError(f"cell sizes must be positive, got cell_w={cell_w}, cell_h={cell_h}")
    height, width = rgb.shape[:2]
    rows, cols = -(-height // cell_h), -(-width // cell_w)
    padded = np.pad(rgb, ((0, rows * cell_h - height), (0, cols * cell_w - width), (0, 0)))
    return padded.reshape(rows, cell_h, cols, cell_w, 3).transpose(0, 2, 1, 3, 4)


def join(pieces: np.ndarray, height: int, width: int) -> np.ndarray:
    """Put a grid of pieces (as made by `cut`, possibly rearranged) back together as a picture.

    `height` and `width` (px) are the size to crop the result to, normally the size of the picture given to `cut`; they
    must not exceed rows * cell_h and cols * cell_w. Returns a float32 (height, width, 3) picture.
    """
    rows, cols, cell_h, cell_w, _ = pieces.shape
    return pieces.transpose(0, 2, 1, 3, 4).reshape(rows * cell_h, cols * cell_w, 3)[:height, :width].astype(np.float32)


def lit(pieces: np.ndarray) -> np.ndarray:
    """(rows, cols) bool: the pieces that have any content, meaning some value above 0.02 (on a 0..1 scale).

    `pieces` is a grid from `cut`. Pieces of black background are not worth moving.
    """
    return pieces.max(axis=(2, 3, 4)) > 0.02


def figure(
    lit_pieces: np.ndarray, matte: np.ndarray | None, cell_w: int, cell_h: int, height: int, width: int
) -> np.ndarray:
    """(rows, cols) bool: the pieces that belong to the subject.

    `lit_pieces` is the (rows, cols) result of `lit` for the grid. `matte` is the subject's (height, width) silhouette,
    0..1, or None. `cell_w`, `cell_h` (px) are the piece size and `height`, `width` (px) the picture size that the grid
    was cut from.

    With a matte, a piece belongs to the subject when more than half of its pixels are inside the matte (above 0.5 on
    average; padding beyond the picture counts as outside), so dark parts of the figure (eyes, shadows) still count.
    Without one, it is the lit pieces with enclosed gaps filled in.
    """
    if matte is None:
        return ndimage.binary_fill_holes(lit_pieces)
    rows, cols = lit_pieces.shape
    padded = np.pad(matte, ((0, rows * cell_h - height), (0, cols * cell_w - width)))
    return padded.reshape(rows, cell_h, cols, cell_w).mean(axis=(1, 3)) > 0.5
