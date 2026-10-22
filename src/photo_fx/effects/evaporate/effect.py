"""The `evaporate` effect: the picture dissolves into character-sized pieces that drift away and fade, like smoke."""

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops import cells, distance
from photo_fx.ops.glyphs import cell_size


class EvaporateParams(EffectParams):
    """The settings of `evaporate`: where pieces detach, how far and which way they drift, and how they fade."""

    cell: int = param(
        5,
        unit="px",
        min=1,
        max=64,
        doc="Character width in px; a piece is one character (its height comes from `font`). Use the same value as "
        "the ascii step before this one, so whole characters fly away.",
    )
    font: str | None = param(
        None,
        doc="Path to the monospaced .ttf/.ttc whose character height sizes the pieces, or None for the system font; "
        "use the same as the ascii step. A path that cannot be opened fails when the effect runs.",
    )
    start: float = param(
        0.6,
        unit="fraction",
        min=0,
        max=1,
        doc="Height (fraction of the picture, 0 = top) above which pieces begin to detach from the top down. "
        "0 switches the top-down dissolve off.",
    )
    end: float = param(
        0.3,
        unit="fraction",
        min=0,
        max=1,
        doc="Height (fraction of the picture, 0 = top) above which every piece has detached. Between `end` and "
        "`start` the chance rises smoothly.",
    )
    edge: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="Width of the band (fraction of the picture width) along the figure's outline, and the left, right and "
        "top borders, where pieces detach; stronger nearer the edge. 0 switches it off. Raising it eats further in.",
    )
    outward: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How much a piece turns away from the edge it came from (the left of the head sheds to the left): 0 = "
        "straight up, 1 = as much sideways as up.",
    )
    rise: float = param(
        0.3,
        unit="fraction",
        min=0,
        max=1,
        doc="Farthest a piece climbs, as a fraction of the picture height (at least one character row). Raising it "
        "spreads the smoke higher.",
    )
    wind: float = param(
        0.0,
        unit="ratio",
        min=-5,
        max=5,
        doc="Sideways drift in characters per row climbed: positive to the right, negative to the left.",
    )
    vanish: float = param(
        0.3,
        unit="fraction",
        min=0,
        max=1,
        doc="Share of detached pieces already gone before they climb (0..1); `thinning` removes more with height.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**31 - 1,
        doc="Random seed: the same seed always gives the same dissolve, another seed another one.",
    )
    rise_gamma: float = param(
        0.6,
        unit="ratio",
        min=0,
        max=5,
        doc="Gamma of the climb against how strongly a piece is pulled away (0..1): a piece climbs up to `rise` x "
        "pull^rise_gamma. Below 1 even weakly pulled pieces climb far, above 1 only the strongly pulled ones.",
    )
    rise_minimum: float = param(
        0.1,
        unit="fraction",
        min=0,
        max=1,
        doc="Shortest climb as a share of a piece's longest one; 1 makes every piece climb the full way.",
    )
    rise_skew: float = param(
        1.5,
        unit="ratio",
        min=0,
        max=5,
        doc="Exponent on each piece's random share of its climb: above 1 most pieces stay low and a few go high, "
        "1 spreads them evenly, below 1 most go high.",
    )
    thinning: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="Extra share of pieces gone by the time they climb the full `rise` (on top of `vanish`): raising it "
        "makes the smoke sparser higher up.",
    )
    falloff: float = param(
        1.5,
        unit="ratio",
        min=0,
        max=5,
        doc="How fast a piece dims as it climbs: brightness is (1 - climbed share of `rise`)^falloff. 0 keeps full "
        "brightness, 1 dims evenly, above 1 dims quickly at first.",
    )
    sway: float = param(
        0.12,
        unit="ratio",
        min=0,
        max=1,
        doc="Random sideways sway in characters per row climbed (a standard deviation): raising it makes the smoke "
        "spread wider the higher it goes.",
    )
    sway_minimum: float = param(
        0.3,
        unit="count",
        min=0,
        max=5,
        doc="Sideways sway every detached piece has, however little it climbs, in characters (a standard deviation).",
    )
    open_bottom: bool = param(
        True,
        doc="True: the bottom border is not an edge for `edge` (a portrait is simply cut off there). False: it sheds "
        "like the other borders.",
    )
    normal_smoothing: float = param(
        1.0,
        unit="count",
        min=0,
        max=10,
        doc="Blur, in characters (a Gaussian sigma), of the outline before `outward` reads its direction: raising it "
        "makes neighbouring pieces fly in more similar directions; 0 uses the raw outline.",
    )


@effect(category="animation", params=EvaporateParams)
def evaporate(rgb: np.ndarray, params: EvaporateParams, *, matte: np.ndarray | None = None) -> np.ndarray:
    """Dissolve the picture into pieces that float away, thin out and fade.

    The picture is cut into character-sized pieces. Black is background, so use it on a cut-out on black. Two things
    make pieces detach, at random and more often the stronger they act (the stronger of the two wins at any spot): from
    the top down, between `start` and `end`; and from the outline, within `edge` of the edge of the figure and of the
    left, right and top borders of the picture (and the bottom unless `open_bottom`).

    A detached piece rises up to `rise`, the further the sparser (`thinning`) and dimmer (`falloff`), and sways
    sideways (`sway`). `outward` turns it away from the edge it came from; `wind` pushes everything sideways. Pieces
    leaving the picture are gone. With `edge` and `outward` at 0 pieces only move up, so everything more than one
    character row below `start` is untouched. The same `seed` always gives the same dissolve.

    `matte` is filled in by the recipe (never set it in a recipe file): the subject's silhouette, which defines
    "the edge of the figure". Dark parts of the picture (eyes, shadows) are then still part of the figure. Without
    a matte the lit pieces are used, with enclosed dark gaps filled in.
    """
    p = params
    height, width = rgb.shape[:2]
    cell_w, cell_h = cell_size(p.cell, p.font)
    pieces = cells.cut(rgb, cell_w, cell_h)
    rows, cols = pieces.shape[:2]
    lit = cells.lit(pieces)

    rng = np.random.default_rng(p.seed)
    u = rng.random((3, rows, cols))  # constant: one random field per use below (fixed, so a seed keeps its look)

    # How strongly each piece wants to detach, 0..1 (smoothstep: 3t^2 - 2t^3).
    centre = (np.arange(rows) + 0.5) * cell_h / height  # each row's height in the picture, 0 = top
    t = np.clip((p.start - centre) / max(p.start - p.end, 1e-6), 0.0, 1.0)  # constant: guards start == end
    strength = np.broadcast_to(
        (t * t * (3.0 - 2.0 * t))[:, None], (rows, cols)
    )  # constant: smoothstep polynomial 3t^2 - 2t^3
    figure = cells.figure(lit, matte, cell_w, cell_h, height, width)
    edges = distance.edge_distance(
        figure, sampling=(cell_h, cell_w), open_bottom=p.open_bottom, smoothing=p.normal_smoothing
    )  # px from the nearest edge; direction out
    inside, normal_x, normal_y = edges.distance, edges.normal_x, edges.normal_y
    if p.edge > 0:
        t = np.clip(1.0 - inside / max(p.edge * width, 1.0), 0.0, 1.0)
        strength = np.maximum(strength, t * t * (3.0 - 2.0 * t))  # constant: smoothstep polynomial 3t^2 - 2t^3

    detached = u[0] < strength
    out = np.where(detached[..., None, None, None], 0.0, pieces)

    row, col = np.nonzero(detached & lit)
    rise_rows = max(1.0, p.rise * height / cell_h)
    share = p.rise_minimum + (1 - p.rise_minimum) * u[1][row, col] ** p.rise_skew
    climb = np.maximum(1, np.rint(rise_rows * strength[row, col] ** p.rise_gamma * share))
    height_share = np.minimum(climb / rise_rows, 1.0)
    # Travel direction in pieces per row climbed: straight up, plus the wind, plus away from the nearest edge.
    # A piece is cell_h/cell_w times taller than wide, so sideways steps are scaled to equal distances on screen.
    toward_x = p.wind + 2.0 * p.outward * normal_x[row, col] * cell_h / cell_w
    toward_y = -1.0 + 2.0 * p.outward * normal_y[row, col]
    sway = rng.normal(size=len(row)) * (p.sway * climb + p.sway_minimum)
    new_row = row + np.rint(climb * toward_y).astype(int)
    new_col = col + np.rint(climb * toward_x + sway).astype(int)

    alive = (u[2][row, col] > p.vanish + p.thinning * height_share) & (new_row >= 0) & (new_row < rows)
    alive &= (new_col >= 0) & (new_col < cols)
    dim = (1.0 - height_share[alive]) ** p.falloff
    np.maximum.at(out, (new_row[alive], new_col[alive]), pieces[row[alive], col[alive]] * dim[:, None, None, None])

    return cells.join(out, height, width)
