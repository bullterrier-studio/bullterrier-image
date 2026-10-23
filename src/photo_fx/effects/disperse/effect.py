"""The `disperse` effect: over time, pieces of the picture detach one after another, drift on the wind, fade and go.

Written to the motion-design skill's three layers. Primary: a wind front crosses the picture and pieces drift away on
it. Secondary: the pieces near the outline float up slowly first, each piece pulls back slightly before it goes
(anticipation) and glows briefly as it leaves. Ambient: the pieces that stay shimmer in a slow wave, all the time,
travelling against the wind. Times are shares of the moving part of the clip (`progress` 0..1), so they scale with the
clip's length.
"""

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops import cells, distance
from photo_fx.ops.glyphs import cell_size


class DisperseParams(EffectParams):
    """The settings of `disperse`: the order pieces leave in, where they go, and the timing of each piece's exit."""

    cell: int = param(
        5,
        unit="px",
        min=1,
        max=64,
        doc="Character width in px; a piece is one character (its height comes from `font`). Use the same value as "
        "the ascii step before this one, so whole characters move.",
    )
    font: str | None = param(
        None,
        doc="Path to the monospaced .ttf/.ttc whose character height sizes the pieces, or None for the system font; "
        "use the same as the ascii step. A path that cannot be opened fails when the effect runs.",
    )
    progress: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The moment shown: 0 = untouched, 1 = finished (every piece that goes has gone). Animate it.",
    )
    top: float = param(
        1.0,
        unit="ratio",
        min=0,
        max=10,
        doc="Weight of height in the order pieces leave: higher pieces go first. 0 = height does not matter.",
    )
    edge: float = param(
        1.0,
        unit="ratio",
        min=0,
        max=10,
        doc="Weight of closeness to the outline (and the left, right and top borders) in the order: pieces near the "
        "edge go before the middle. 0 = closeness does not matter.",
    )
    sweep: float = param(
        0.0,
        unit="ratio",
        min=-1,
        max=1,
        doc="Weight of side in the order: positive starts at the left, negative at the right; its size is how much it "
        "matters next to `top` and `edge`.",
    )
    reach: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of the figure ever goes, in leaving order: 0.4 = the first 40% disperses and the rest stays, "
        "1 = everything goes.",
    )
    wind: float = param(
        0.2,
        unit="fraction",
        min=-1,
        max=1,
        doc="The farthest any piece is carried by the time it vanishes, as a fraction of the picture width: positive "
        "blows right, negative left. Keep it under a third, or pieces cross too much of the picture in one go.",
    )
    angle: float = param(
        10.0,
        unit="degrees",
        min=-180,
        max=180,
        doc="Direction of the wind above the horizontal (on the side `wind` blows to); negative blows downward.",
    )
    hover: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="Share of its journey a piece floats slowly along the wind before it goes (about 0..0.3 looks right). "
        "The outline starts at once, deeper pieces later (see `hover_spread`). 0 = off.",
    )
    outward: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How much a piece is also thrown away from the edge it came from (by up to `outward_distance`); pieces "
        "then fly in different directions, so leave it at 0 when everything should travel together on the wind.",
    )
    ambient: float = param(
        0.0,
        unit="ratio",
        min=0,
        max=1,
        doc="Depth of the slow shimmer of the pieces that stay, as a share of their brightness (about 0..0.1 looks "
        "right; brightening stops at white). 0 = off.",
    )
    clock: float = param(
        0.0,
        unit="seconds",
        min=0,
        max=3600,
        doc='The time the shimmer runs on: animate it over the whole clip (`over = "clip"`) so the picture is alive '
        "even while nothing detaches.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**31 - 1,
        doc="Random seed: each piece's fate is fixed by it, so every frame of one seed fits together.",
    )
    edge_depth: float = param(
        0.15,
        unit="fraction",
        min=0.01,
        max=1,
        doc="Distance from the outline (fraction of the picture width) beyond which a piece counts as fully inside, "
        "for `edge` and `hover_spread`. Raising it makes closeness to the edge matter deeper in.",
    )
    jitter: float = param(
        0.2,
        unit="fraction",
        min=0,
        max=1,
        doc="Random share of each piece's place in the leaving order: 0 = a clean front, raising it makes it ragged.",
    )
    anticipation: float = param(
        0.06,
        unit="fraction",
        min=0.01,
        max=0.2,
        doc="Wind-up time before a piece goes, as a share of the clip: it eases back by `pullback` over this time. "
        "The first piece goes only after it, so every piece has one.",
    )
    pullback: float = param(
        0.15,
        unit="fraction",
        min=0,
        max=1,
        doc="How far a piece eases back against its journey before it goes (anticipation), as a share of the "
        "journey. 0 = no wind-up.",
    )
    flight: float = param(
        0.2,
        unit="fraction",
        min=0.01,
        max=0.5,
        doc="Average time from detaching to vanishing, as a share of the clip. Shorter flights keep fewer pieces in "
        "the air at once (about 0.1 keeps it to a third on a side-to-side sweep).",
    )
    flight_variation: float = param(
        0.4,
        unit="fraction",
        min=0,
        max=0.9,
        doc="How much each piece's flight differs from `flight`: 0.4 = from 0.6x to 1.4x. 0 = all the same.",
    )
    finish: float = param(
        0.98,
        unit="fraction",
        min=0.5,
        max=1,
        doc="The progress by which the last piece has vanished (a little under 1 gives a clean last frame). If "
        "`anticipation` and the longest flight do not fit before it, all pieces go at once.",
    )
    hover_spread: float = param(
        0.45,
        unit="fraction",
        min=0,
        max=1,
        doc="With `hover`: how far into the clip a piece `edge_depth` inside starts to float (the outline starts at "
        "0). 0 = everything floats at once.",
    )
    bob: float = param(
        0.004,
        unit="fraction",
        min=0,
        max=0.05,
        doc="Height of the gentle up-and-down of a hovering piece, as a fraction of the picture height.",
    )
    bob_cycles: float = param(
        3.0,
        unit="cycles",
        min=0,
        max=20,
        doc="Number of ups and downs while a piece hovers.",
    )
    angle_spread: float = param(
        12.0,
        unit="degrees",
        min=0,
        max=180,
        doc="Range of each piece's own direction around the wind's `angle` (it differs by up to half of this).",
    )
    distance_minimum: float = param(
        0.35,
        unit="fraction",
        min=0,
        max=1,
        doc="The shortest journey as a share of the longest (`wind`), so speeds differ; 1 = all travel equally far.",
    )
    outward_distance: float = param(
        0.12,
        unit="fraction",
        min=0,
        max=1,
        doc="How far `outward` 1 throws a piece away from its edge, as a fraction of the picture's width "
        "(sideways) and height (up and down).",
    )
    acceleration: float = param(
        0.5,
        unit="ratio",
        min=0,
        max=10,
        doc="How a piece speeds up on its way (an exit accelerates): it ends 1 + 2 x acceleration times as fast as "
        "it starts. 0 = steady speed.",
    )
    sway: float = param(
        0.012,
        unit="fraction",
        min=0,
        max=0.2,
        doc="Largest sideways wobble of a flying piece, as a fraction of the picture width; it grows during the "
        "flight. 0 = none.",
    )
    sway_cycles: float = param(
        1.5,
        unit="cycles",
        min=0,
        max=20,
        doc="Number of wobbles during a piece's flight.",
    )
    falloff: float = param(
        1.5,
        unit="ratio",
        min=0,
        max=10,
        doc="How a flying piece fades: brightness is (1 - share of flight)^falloff. 0 = no fade (it just vanishes "
        "at the end), 1 = even, above 1 fades quickly at first.",
    )
    flare: float = param(
        0.4,
        unit="ratio",
        min=0,
        max=2,
        doc="Extra brightness at the peak of the glow as a piece leaves (0.4 = 1.4x); brightening stops at white. "
        "0 = no glow.",
    )
    flare_span: float = param(
        0.2,
        unit="fraction",
        min=0.01,
        max=1,
        doc="Share of a piece's flight that the glow lasts; it peaks halfway through that.",
    )
    ambient_period: float = param(
        3.0,
        unit="seconds",
        min=0.1,
        max=60,
        doc="Seconds per shimmer cycle (breathing is 2-4 s).",
    )
    ambient_wavelength: float = param(
        0.2,
        unit="fraction",
        min=0.01,
        max=2,
        doc="Length of one shimmer wave as a fraction of the picture width; with the period it sets how fast the "
        "wave travels (against the wind).",
    )
    open_bottom: bool = param(
        True,
        doc="True: the bottom border is not an edge for `edge` and `hover` (a portrait is cut off there). False: it "
        "counts like the other borders.",
    )
    normal_smoothing: float = param(
        1.0,
        unit="count",
        min=0,
        max=10,
        doc="Blur, in characters (a Gaussian sigma), of the outline before `outward` reads its direction: raising it "
        "makes neighbouring pieces fly in more similar directions; 0 uses the raw outline.",
    )
    matte_threshold: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="With a matte: how much of a piece, 0 to 1, the matte must cover on average for the piece to belong to "
        "the figure whose outline `edge` and `hover` start at; 0.5 = more than half. Lowering it takes in more of a "
        "soft edge (hair, fur), raising it less. Ignored without a matte.",
    )
    background_threshold: float = param(
        0.02,
        unit="fraction",
        min=0,
        max=1,
        doc="How bright, 0 to 1, some pixel of a piece must be for the piece to count as picture rather than black "
        "background: a piece no brighter never flies and is drawn black once the dispersal (or the shimmer) has "
        "begun. Raising it drops dim, noisy pieces; without a matte it also decides the figure's outline.",
    )


@effect(category="animation", params=DisperseParams)
def disperse(rgb: np.ndarray, params: DisperseParams, *, matte: np.ndarray | None = None) -> np.ndarray:
    """The picture at one moment of its dispersal: `progress` 0 = untouched, 1 = finished. Animate `progress`.

    The picture is cut into character-sized pieces. Black is background, so use it on a cut-out on black. Each piece
    has a fixed, random-ish fate, so the same `seed` gives the same motion in every frame: it winds up, detaches,
    drifts, wobbles, glows, fades and is gone, and frames fit together as a continuous animation.

    When a piece detaches is set by where it is (`top`, `edge`, `sweep`, with `jitter`); `reach` is how much of the
    figure ever goes. Where it goes: on the wind (`wind`, `angle`), and away from its edge with `outward`. `hover`
    makes the edges loosen all round first; `ambient` with `clock` keeps the pieces that stay alive.

    `matte` is filled in by the recipe (never set it in a recipe file): the subject's silhouette, used for `edge`.
    """
    p = params
    height, width = rgb.shape[:2]
    if p.progress <= 0.0 and p.ambient <= 0.0:
        return rgb.copy()
    cell_w, cell_h = cell_size(p.cell, p.font)
    pieces = cells.cut(rgb, cell_w, cell_h)
    rows, cols = pieces.shape[:2]
    present = cells.lit(pieces, threshold=p.background_threshold)
    figure = cells.figure(present, matte, cell_w, cell_h, height, width, threshold=p.matte_threshold)
    edges = distance.edge_distance(
        figure, sampling=(cell_h, cell_w), open_bottom=p.open_bottom, smoothing=p.normal_smoothing
    )
    inside, normal_x, normal_y = edges.distance, edges.normal_x, edges.normal_y
    rng = np.random.default_rng(p.seed)
    # constant: six random fields, one per use below (a fixed number, so a seed keeps its look)
    order_noise, flight_noise, speed_noise, angle_noise, sway_noise, phase_noise = rng.random((6, rows, cols))

    # Order of detaching: 0 = first, 1 = last.
    y = np.broadcast_to(((np.arange(rows) + 0.5) * cell_h / height)[:, None], (rows, cols))
    x = np.broadcast_to(((np.arange(cols) + 0.5) * cell_w / width)[None, :], (rows, cols))
    depth = np.clip(inside / (p.edge_depth * width), 0.0, 1.0)  # 0 on the outline, 1 well inside
    factors = [(p.top, y), (p.edge, depth), (abs(p.sweep), x if p.sweep >= 0 else 1.0 - x)]
    total_weight = sum(weight for weight, _ in factors)
    order = sum(weight * value for weight, value in factors) / total_weight if total_weight > 0 else np.zeros_like(y)
    if present.any():
        low, high = order[present].min(), order[present].max()
        order = (order - low) / max(high - low, 1e-6)  # constant: guards a flat order
    order = np.clip(order * (1.0 - p.jitter) + order_noise * p.jitter, 0.0, 1.0)

    # When each piece detaches and how long it flies, in progress units; the last one has vanished by `finish`.
    lifetime = p.flight * ((1 - p.flight_variation) + 2 * p.flight_variation * flight_noise)
    span = max(p.finish - p.anticipation - p.flight * (1 + p.flight_variation), 0.0)
    detach_at = p.anticipation + order / max(p.reach, 1e-6) * span  # constant: guards reach == 0
    age = (p.progress - detach_at) / lifetime  # <= 0: not yet, 0..1: in flight, >= 1: gone
    goes = present & (order <= p.reach)
    closeness = 1.0 - depth  # 1 on the outline, 0 well inside
    hover_start = detach_at - p.anticipation  # with no hovering a piece only winds up just before it goes
    if p.hover > 0.0:  # the outline starts to float at once, deeper pieces later (but never later than that wind-up)
        hover_start = np.minimum(hover_start, (1.0 - closeness) * p.hover_spread)
    hover_start = np.maximum(hover_start, 0.0)
    winding = goes & (age <= 0.0) & (p.progress > hover_start)
    flying = goes & (age > 0.0) & (age < 1.0)
    staying = present & ~(goes & (age > 0.0)) & ~winding

    # Ambient layer: a slow wave through the pieces that stay, moving against the wind.
    shimmer = np.ones((rows, cols))
    if p.ambient > 0.0:
        against = 1.0 if p.wind >= 0 else -1.0  # constant: the sign of a direction
        phase = p.clock / p.ambient_period + against * x / p.ambient_wavelength
        shimmer = 1.0 + p.ambient * np.sin(2.0 * np.pi * phase)
    out = np.where(staying[..., None, None, None], pieces * shimmer[..., None, None, None], 0.0)

    # Primary layer: pieces that are winding up or in flight.
    row, col = np.nonzero(winding | flying)
    a = age[row, col]
    t = np.clip(a, 0.0, 1.0)  # how far through its flight
    speed = p.distance_minimum + (1 - p.distance_minimum) * speed_noise[row, col]
    direction = np.radians(p.angle + (angle_noise[row, col] - 0.5) * p.angle_spread)
    reach_px = abs(p.wind) * width * speed  # this piece's whole journey
    journey_x = (
        np.sign(p.wind) * reach_px * np.cos(direction) + p.outward * p.outward_distance * width * normal_x[row, col]
    )
    journey_y = -reach_px * np.sin(direction) + p.outward * p.outward_distance * height * normal_y[row, col]
    path = (t + p.acceleration * t * t) / (1 + p.acceleration)  # an exit: starts gently, speeds up, ends at 1

    # Before it goes: float slowly along the journey (hover), then ease back a little against it (anticipation).
    hover_time = np.maximum(detach_at[row, col] - hover_start[row, col], 1e-9)  # constant: guards a zero hover
    floating = np.clip((p.progress - hover_start[row, col]) / hover_time, 0, 1)
    windup = np.clip((p.progress - (detach_at[row, col] - p.anticipation)) / p.anticipation, 0.0, 1.0)
    settled = p.hover * floating * floating * (3.0 - 2.0 * floating)  # constant: smoothstep polynomial 3t^2 - 2t^3
    start = p.hover - p.pullback  # where flight begins, as a share of the journey
    along = np.where(a > 0.0, start + (1.0 - start) * path, settled - p.pullback * (1.0 - (1.0 - windup) ** 2))
    bob = (
        p.bob
        * height
        * (1.0 if p.hover > 0.0 else 0.0)  # the bob only exists while pieces hover
        * np.sin(np.pi * floating)
        * np.sin(2.0 * np.pi * (p.bob_cycles * floating + phase_noise[row, col]))
    )
    sway = p.sway * width * sway_noise[row, col] * t * np.sin(2.0 * np.pi * (p.sway_cycles * t + phase_noise[row, col]))
    dx = along * journey_x + sway
    dy = along * journey_y + np.where(a > 0.0, 0.0, bob)

    # Secondary layer: a brief glow just after leaving, then the fade.
    glow = 1.0 + p.flare * np.sin(np.pi * np.clip(t / p.flare_span, 0.0, 1.0))
    brightness = np.where(a > 0.0, (1.0 - t) ** p.falloff, 1.0) * glow

    yy = row[:, None, None] * cell_h + np.rint(dy).astype(int)[:, None, None] + np.arange(cell_h)[None, :, None]
    xx = col[:, None, None] * cell_w + np.rint(dx).astype(int)[:, None, None] + np.arange(cell_w)[None, None, :]
    yy, xx = np.broadcast_arrays(yy, xx)  # both (pieces, cell_h, cell_w)
    canvas = np.zeros((rows * cell_h, cols * cell_w, 3), dtype=np.float32)  # constant: the three RGB channels
    on_canvas = (yy >= 0) & (yy < canvas.shape[0]) & (xx >= 0) & (xx < canvas.shape[1])
    shaded = pieces[row, col] * brightness[:, None, None, None]
    np.maximum.at(canvas, (yy[on_canvas], xx[on_canvas]), shaded[on_canvas])

    kept = cells.join(out, canvas.shape[0], canvas.shape[1])
    # The glow and the shimmer brighten; on what is already white they would leave the 0..1 range, so they stop there.
    return np.clip(np.maximum(kept, canvas)[:height, :width], 0.0, 1.0).astype(np.float32)
