"""Dispersal over time: pieces of the picture detach one after another, drift on the wind, fade and are gone.

Written to the motion-design skill's three layers. Primary: a wind front crosses the picture and pieces drift away on
it. Secondary: the pieces near the outline float up slowly first, each piece pulls back slightly before it goes
(anticipation) and glows briefly as it leaves. Ambient: the pieces that stay shimmer in a slow wave, all the time,
travelling against the wind.
"""

import numpy as np

from photo_fx.core.registry import effect
from photo_fx.ops import cells, distance
from photo_fx.ops.glyphs import cell_size

# All times are shares of the moving part of the clip (progress 0..1), so they scale with the clip's length.
_ANTICIPATE = 0.06  # wind-up before a piece goes; the first piece detaches only after this, so every piece has one
_FLIGHT = 0.2  # time from detaching to vanishing, on average; each piece varies by 0.6x to 1.4x
_END = 0.98  # the last piece has vanished by here, so progress 1 is a clean finish
_JITTER = 0.2  # share of each piece's detach time that is random, so the front is ragged and not a clean line
_HOVER_SPREAD = 0.45  # a piece on the outline starts to hover at once; one far inside, this far into the clip
_HOVER_BOB = 0.004  # of the picture height: the gentle up-and-down of a hovering piece
_ANGLE_SPREAD = 12.0  # degrees: each piece's own direction differs from the wind's by up to half of this
_PULL = 0.15  # wind-up distance, as a share of the piece's whole journey, in the direction opposite to it
_FLARE = 0.4  # extra brightness at the brightest moment of the glow (kept well short of a white flash)
_FLARE_SPAN = 0.2  # the glow lasts this share of a piece's flight, peaking early in it
_AMBIENT_PERIOD = 3.0  # seconds per shimmer cycle (breathing: 2-4 s)
_AMBIENT_WAVELENGTH = 0.2  # of the picture width: with the period, the wave moves at about a quarter of a piece's speed


@effect(category="animation")
def disperse(
    rgb: np.ndarray,
    cell: int = 5,
    progress: float = 0.0,
    top: float = 1.0,
    edge: float = 1.0,
    sweep: float = 0.0,
    reach: float = 1.0,
    wind: float = 0.2,
    angle: float = 10.0,
    hover: float = 0.0,
    outward: float = 0.0,
    ambient: float = 0.0,
    clock: float = 0.0,
    seed: int = 0,
    matte: np.ndarray | None = None,
) -> np.ndarray:
    """The picture at one moment of its dispersal: `progress` 0 = untouched, 1 = finished. Animate `progress`.

    The picture is cut into character-sized pieces (`cell` = character width in px: use the same value as the
    `ascii` step before this one). Black is background, so use it on a cut-out on black. Each piece has a fixed,
    random-ish fate, so the same `seed` gives the same motion in every frame: it winds up, detaches, drifts,
    wobbles, glows, fades and is gone, and frames fit together as a continuous animation.

    When a piece detaches is set by where it is. `top` makes pieces higher up go first, `edge` makes pieces near
    the outline of the figure go before the middle, and `sweep` (-1..1) makes one side go before the other: positive
    starts at the left, negative at the right (how strongly it matters, relative to the other two). `reach` (0..1)
    is how much of the figure ever goes: at 0.4 the outer 40% disperses and the rest stays, at 1 everything goes.

    Where a piece goes: the wind blows at `angle` degrees above the horizontal, to the right when `wind` is positive
    and to the left when it is negative. `wind` is also the farthest any piece is carried by the time it vanishes, as
    a fraction of the picture width (others go a third to all of that, so speeds differ); keep it under a third, or
    pieces cross too much of the picture in one go. `outward` (0..1) additionally throws a piece away from the edge
    it came from; it makes pieces fly in different directions, so leave it at 0 when everything should travel
    together on the wind.

    `hover` (0..~0.3) makes pieces float slowly along the wind's direction before they go, by that share of their
    journey. The pieces on the outline (and along the picture's left, right and top borders) start at once, deeper
    ones later, so the edges loosen all round first and each piece lets go when the front reaches it. 0 = off.

    `ambient` (0..~0.1) is the strength of the slow shimmer of the pieces that stay, and `clock` the time in seconds
    it runs on: animate `clock` over the whole clip (`over = "clip"`) so the picture is alive even while nothing
    detaches. The shimmer travels against the wind, which balances the motion of the pieces.

    `matte` is filled in by the recipe (never set it in a recipe file): the subject's silhouette, used for `edge`.
    """
    height, width = rgb.shape[:2]
    if progress <= 0.0 and ambient <= 0.0:
        return rgb.copy()
    cell_w, cell_h = cell_size(cell)
    pieces = cells.cut(rgb, cell_w, cell_h)
    rows, cols = pieces.shape[:2]
    present = cells.lit(pieces)
    figure = cells.figure(present, matte, cell_w, cell_h, height, width)
    edges = distance.edge_distance(figure, sampling=(cell_h, cell_w))
    inside, normal_x, normal_y = edges.distance, edges.normal_x, edges.normal_y
    rng = np.random.default_rng(seed)
    u = rng.random((6, rows, cols))

    # Order of detaching: 0 = first, 1 = last.
    y = np.broadcast_to(((np.arange(rows) + 0.5) * cell_h / height)[:, None], (rows, cols))
    x = np.broadcast_to(((np.arange(cols) + 0.5) * cell_w / width)[None, :], (rows, cols))
    factors = [(top, y), (edge, np.clip(inside / (0.15 * width), 0.0, 1.0)), (abs(sweep), x if sweep >= 0 else 1.0 - x)]
    total_weight = sum(weight for weight, _ in factors)
    order = sum(weight * value for weight, value in factors) / total_weight if total_weight > 0 else np.zeros_like(y)
    if present.any():
        low, high = order[present].min(), order[present].max()
        order = (order - low) / max(high - low, 1e-6)
    order = np.clip(order * (1.0 - _JITTER) + u[0] * _JITTER, 0.0, 1.0)

    detach_at, lifetime = _schedule(order, u[1], reach)
    age = (progress - detach_at) / lifetime  # <= 0: not yet, 0..1: in flight, >= 1: gone
    goes = present & (order <= reach)
    closeness = 1.0 - np.clip(inside / (0.15 * width), 0.0, 1.0)  # 1 on the outline, 0 well inside
    hover_start = detach_at - _ANTICIPATE  # with no hovering a piece only winds up just before it goes
    if hover > 0.0:  # the outline starts to float at once, deeper pieces later (but never later than that wind-up)
        hover_start = np.minimum(hover_start, (1.0 - closeness) * _HOVER_SPREAD)
    hover_start = np.maximum(hover_start, 0.0)
    winding = goes & (age <= 0.0) & (progress > hover_start)
    flying = goes & (age > 0.0) & (age < 1.0)
    staying = present & ~(goes & (age > 0.0)) & ~winding

    # Ambient layer: a slow wave through the pieces that stay, moving against the wind.
    shimmer = np.ones((rows, cols))
    if ambient > 0.0:
        against = 1.0 if wind >= 0 else -1.0
        shimmer = 1.0 + ambient * np.sin(2.0 * np.pi * (clock / _AMBIENT_PERIOD + against * x / _AMBIENT_WAVELENGTH))
    out = np.where(staying[..., None, None, None], pieces * shimmer[..., None, None, None], 0.0)

    # Primary layer: pieces that are winding up or in flight.
    row, col = np.nonzero(winding | flying)
    a = age[row, col]
    t = np.clip(a, 0.0, 1.0)  # how far through its flight
    speed = 0.35 + 0.65 * u[2][row, col]
    direction = np.radians(angle + (u[3][row, col] - 0.5) * _ANGLE_SPREAD)
    reach_px = abs(wind) * width * speed  # this piece's whole journey
    journey_x = np.sign(wind) * reach_px * np.cos(direction) + outward * 0.12 * width * normal_x[row, col]
    journey_y = -reach_px * np.sin(direction) + outward * 0.12 * height * normal_y[row, col]
    path = (t + 0.5 * t * t) / 1.5  # an exit: starts gently, speeds up, ends at 1

    # Before it goes: float slowly along the journey (hover), then ease back a little against it (anticipation).
    floating = np.clip(
        (progress - hover_start[row, col]) / np.maximum(detach_at[row, col] - hover_start[row, col], 1e-9), 0, 1
    )
    windup = np.clip((progress - (detach_at[row, col] - _ANTICIPATE)) / _ANTICIPATE, 0.0, 1.0)
    settled = hover * floating * floating * (3.0 - 2.0 * floating)  # share of the journey floated so far
    start = hover - _PULL  # where flight begins, as a share of the journey
    along = np.where(a > 0.0, start + (1.0 - start) * path, settled - _PULL * (1.0 - (1.0 - windup) ** 2))
    bob = (
        _HOVER_BOB
        * height
        * hover_gate(hover)
        * np.sin(np.pi * floating)
        * np.sin(2.0 * np.pi * (3.0 * floating + u[5][row, col]))
    )
    sway = 0.012 * width * u[4][row, col] * t * np.sin(2.0 * np.pi * (1.5 * t + u[5][row, col]))
    dx = along * journey_x + sway
    dy = along * journey_y + np.where(a > 0.0, 0.0, bob)

    # Secondary layer: a brief glow just after leaving, then the fade.
    glow = 1.0 + _FLARE * np.sin(np.pi * np.clip(t / _FLARE_SPAN, 0.0, 1.0))
    brightness = np.where(a > 0.0, (1.0 - t) ** 1.5, 1.0) * glow

    yy = row[:, None, None] * cell_h + np.rint(dy).astype(int)[:, None, None] + np.arange(cell_h)[None, :, None]
    xx = col[:, None, None] * cell_w + np.rint(dx).astype(int)[:, None, None] + np.arange(cell_w)[None, None, :]
    yy, xx = np.broadcast_arrays(yy, xx)  # both (pieces, cell_h, cell_w)
    canvas = np.zeros((rows * cell_h, cols * cell_w, 3), dtype=np.float32)
    on_canvas = (yy >= 0) & (yy < canvas.shape[0]) & (xx >= 0) & (xx < canvas.shape[1])
    shaded = pieces[row, col] * brightness[:, None, None, None]
    np.maximum.at(canvas, (yy[on_canvas], xx[on_canvas]), shaded[on_canvas])

    kept = cells.join(out, canvas.shape[0], canvas.shape[1])
    return np.maximum(kept, canvas)[:height, :width].astype(np.float32)


def hover_gate(hover: float) -> float:
    """1 when hovering is on, 0 when it is off (so the bob only exists when pieces hover)."""
    return 1.0 if hover > 0.0 else 0.0


def _schedule(order: np.ndarray, spread: np.ndarray, reach: float) -> tuple[np.ndarray, np.ndarray]:
    """When each piece detaches and how long it flies, in progress units, from its `order` (0 first .. 1 last)."""
    lifetime = _FLIGHT * (0.6 + 0.8 * spread)
    detach_at = _ANTICIPATE + order / max(reach, 1e-6) * (_END - _ANTICIPATE - _FLIGHT * 1.4)
    return detach_at, lifetime
