"""Evaporation: the picture dissolves into character-sized pieces that drift away and fade, like smoke."""

import numpy as np

from photo_fx.core.registry import effect

from ..ops import cells, distance
from .ascii import cell_size


@effect(category="animation")
def evaporate(
    rgb: np.ndarray,
    cell: int = 5,
    start: float = 0.6,
    end: float = 0.3,
    edge: float = 0.0,
    outward: float = 0.0,
    rise: float = 0.3,
    wind: float = 0.0,
    vanish: float = 0.3,
    seed: int = 0,
    matte: np.ndarray | None = None,
) -> np.ndarray:
    """Dissolve the picture into pieces that float away, thin out and fade.

    The picture is cut into character-sized pieces (`cell` = character width in px: use the same value as the
    `ascii` step before this one, so whole characters fly away). Black is background, so use it on a cut-out on
    black. Two things make pieces detach, at random and more often the stronger they act (the stronger of the two
    wins at any spot):

    - from the top down: above `start` (a fraction of the height, 0 = top) pieces begin to detach and by `end`
      every piece is gone from its place. `start` = 0 switches this off.
    - from the outline: within `edge` (a fraction of the width) of the edge of the figure, and of the left, right
      and top borders of the picture (not the bottom, where a portrait is simply cut off). 0 switches this off.

    A detached piece rises up to `rise` (fraction of the height), the further the sparser and dimmer, and sways
    sideways. `outward` (0..1) turns it away from the edge it came from, so the left side of the head sheds to the
    left and the right side to the right; 0 sends everything straight up. `wind` pushes everything right (positive)
    or left (negative), in characters per row risen. `vanish` (0..1) is the share of detached pieces already gone.
    Pieces leaving the picture are gone. With `edge` and `outward` at 0 pieces only move up, so everything more than
    one character row below `start` is untouched. The same `seed` always gives the same dissolve.

    `matte` is filled in by the recipe (never set it in a recipe file): the subject's silhouette, which defines
    "the edge of the figure". Dark parts of the picture (eyes, shadows) are then still part of the figure. Without
    a matte the lit pieces are used, with enclosed dark gaps filled in.
    """
    height, width = rgb.shape[:2]
    cell_w, cell_h = cell_size(cell)
    pieces = cells.cut(rgb, cell_w, cell_h)
    rows, cols = pieces.shape[:2]
    lit = cells.lit(pieces)

    rng = np.random.default_rng(seed)
    u = rng.random((3, rows, cols))

    # How strongly each piece wants to detach, 0..1.
    centre = (np.arange(rows) + 0.5) * cell_h / height  # each row's height in the picture, 0 = top
    t = np.clip((start - centre) / max(start - end, 1e-6), 0.0, 1.0)
    strength = np.broadcast_to((t * t * (3.0 - 2.0 * t))[:, None], (rows, cols))
    figure = cells.figure(lit, matte, cell_w, cell_h, height, width)
    edges = distance.edge_distance(figure, sampling=(cell_h, cell_w))  # px from the nearest edge; direction out
    inside, normal_x, normal_y = edges.distance, edges.normal_x, edges.normal_y
    if edge > 0:
        t = np.clip(1.0 - inside / max(edge * width, 1.0), 0.0, 1.0)
        strength = np.maximum(strength, t * t * (3.0 - 2.0 * t))

    detached = u[0] < strength
    out = np.where(detached[..., None, None, None], 0.0, pieces)

    row, col = np.nonzero(detached & lit)
    rise_rows = max(1.0, rise * height / cell_h)
    climb = np.maximum(1, np.rint(rise_rows * strength[row, col] ** 0.6 * (0.1 + 0.9 * u[1][row, col] ** 1.5)))
    height_share = np.minimum(climb / rise_rows, 1.0)
    # Travel direction in pieces per row climbed: straight up, plus the wind, plus away from the nearest edge.
    # A piece is cell_h/cell_w times taller than wide, so sideways steps are scaled to equal distances on screen.
    toward_x = wind + 2.0 * outward * normal_x[row, col] * cell_h / cell_w
    toward_y = -1.0 + 2.0 * outward * normal_y[row, col]
    sway = rng.normal(size=len(row)) * (0.12 * climb + 0.3)
    new_row = row + np.rint(climb * toward_y).astype(int)
    new_col = col + np.rint(climb * toward_x + sway).astype(int)

    alive = (u[2][row, col] > vanish + 0.5 * height_share) & (new_row >= 0) & (new_row < rows)
    alive &= (new_col >= 0) & (new_col < cols)
    dim = (1.0 - height_share[alive]) ** 1.5
    np.maximum.at(out, (new_row[alive], new_col[alive]), pieces[row[alive], col[alive]] * dim[:, None, None, None])

    return cells.join(out, height, width)
