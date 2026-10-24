"""The `flow_lines` effect: thin lines that follow a smooth noise flow field, in place of the picture."""

import numpy as np

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.noise import fractal


class FlowLinesParams(EffectParams):
    """Streamlines of a flow field made of fractal noise: lines that start at random points and follow the flow."""

    density: float = param(
        0.002,
        unit="fraction",
        min=0,
        max=0.05,
        doc="How many lines: the share of the picture's pixels that start one, so a 480 x 384 picture at 0.002 has "
        "about 370. 0 draws none. More = a denser weave; lines cross freely.",
    )
    length: int = param(
        120,
        unit="count",
        min=1,
        max=2000,
        doc="How many steps each line follows the flow; with `step` it sets the line's length in px. A line also "
        "ends where it reaches the edge of the picture.",
    )
    step: float = param(
        1.0,
        unit="px",
        min=0.5,
        max=16,
        doc="The distance in px a line moves each step. Keep it at or below the line's thickness for a solid line; "
        "larger steps leave dotted lines.",
    )
    thickness: float = param(
        1.5,
        unit="px",
        min=0.5,
        max=16,
        doc="The width of the lines in px (a soft disc of this diameter is stamped at every step). Tuned for a "
        "picture about 480 px high.",
    )
    scale: float = param(
        120.0,
        unit="px",
        min=4,
        max=1024,
        doc="The size of the flow's coarsest swirls in px, the spacing of the first noise lattice. Larger = broader, "
        "gentler bends. Tuned for a picture about 480 px high.",
    )
    octaves: int = param(
        2,
        unit="count",
        min=1,
        max=8,
        doc="How many layers of noise are summed for the flow, each finer than the last (by `lacunarity`) and weaker "
        "(by `persistence`). 1 = smooth sweeping lines, more = wigglier ones.",
    )
    persistence: float = param(
        0.5,
        unit="ratio",
        min=0,
        max=1,
        doc="How much weaker each finer layer of the flow is than the one before. 0 = only the coarsest layer.",
    )
    lacunarity: float = param(
        2.0,
        unit="ratio",
        min=1,
        max=4,
        doc="How much finer each layer of the flow is than the one before: the lattice spacing is divided by this.",
    )
    twist: float = param(
        180.0,
        unit="degrees",
        min=0,
        max=720,
        doc="How far the flow turns for each standard deviation of the noise: a line's direction is the noise times "
        "this, measured clockwise on screen from the right. 0 = every line runs straight to the right; larger = "
        "tighter, more varied swirls.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**31 - 1,
        doc="Seeds the flow and the line starts: the same seed gives the same picture. The starts and the flow "
        "are independent random streams of it.",
    )
    line: Color = param("#ffd84a", doc="The colour of the lines.")
    background: Color = param("#12002b", doc="The colour behind the lines.")


@effect(category="backgrounds", params=FlowLinesParams)
def flow_lines(rgb: np.ndarray, params: FlowLinesParams) -> np.ndarray:
    """Replace the picture with streamlines of a noise flow: lines that start at random points and follow the flow.

    A generated background: the input is used only for its height and width, its content is ignored. The flow's
    direction at each pixel is `twist` times a fractal noise field. Each of the `density` x pixels lines starts at a
    random point and takes `length` steps of `step` px along the flow (the direction read at the pixel the line is
    in), ending early where it would leave the picture. A soft disc `thickness` px across is stamped at every
    point; where discs overlap the strongest counts, so lines stay as bold as one.
    """
    height, width = rgb.shape[:2]
    rng = np.random.default_rng(params.seed)
    lines = round(params.density * height * width)
    extent = np.array([width, height])  # x then y
    where = rng.random((lines, len(extent))) * extent  # px
    # constant: stream 1 of the seed, independent of the starts' stream 0 (the seed itself)
    heading = np.radians(params.twist) * fractal(
        (height, width),
        scale=params.scale,
        octaves=params.octaves,
        persistence=params.persistence,
        lacunarity=params.lacunarity,
        seed=[params.seed, 1],
    )
    # follow all the lines together; `running` marks those that have not left the picture
    trail = np.empty((params.length, *where.shape))
    running = np.ones((params.length, lines), bool)
    on_picture = np.ones(lines, bool)
    for k in range(params.length):
        on_picture &= (where[:, 0] >= 0) & (where[:, 0] < width) & (where[:, 1] >= 0) & (where[:, 1] < height)
        trail[k], running[k] = where, on_picture
        moving = np.flatnonzero(on_picture)
        pixel = where[moving].astype(int)
        angle = heading[pixel[:, 1], pixel[:, 0]]
        where[moving] += params.step * np.stack([np.cos(angle), np.sin(angle)], axis=1)
    points = trail[running]
    # stamp a soft disc at every point: each pixel takes the strongest one over it
    radius = params.thickness / 2
    reach = int(np.ceil(radius))
    corner = np.floor(points).astype(int)
    ink = np.zeros(height * width, np.float32)
    for dy in range(-reach, reach + 1):
        for dx in range(-reach, reach + 1):
            col, row = corner[:, 0] + dx, corner[:, 1] + dy
            covers = np.clip(radius - np.hypot(col + 0.5 - points[:, 0], row + 0.5 - points[:, 1]) + 0.5, 0.0, 1.0)
            visible = (col >= 0) & (col < width) & (row >= 0) & (row < height)
            np.maximum.at(ink, row[visible] * width + col[visible], covers[visible])
    ink = ink.reshape(height, width)[..., None]
    background = np.array(params.background, np.float32)
    return (background + (np.array(params.line, np.float32) - background) * ink).astype(np.float32)
