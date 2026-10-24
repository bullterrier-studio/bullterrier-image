"""The `shatter` effect: the picture broken into Voronoi pieces, each moved and turned, with lines between them."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.voronoi import cells, line_coverage
from bullterrier_image.ops.warp import remap


class ShatterParams(EffectParams):
    """Broken glass: random pieces, each showing the picture a little moved and turned, divided by thin lines."""

    pieces: int = param(
        24,
        unit="count",
        min=4,
        max=500,
        doc="How many pieces (Voronoi cells round random points) the picture breaks into, 4 to 500. More = smaller "
        "pieces; the average piece is the picture's area divided by this.",
    )
    offset: float = param(
        10.0,
        unit="px",
        min=0,
        max=200,
        doc="The farthest a piece's picture is moved, in pixels, up or down and left or right: each piece draws its "
        "own random move up to this far in each direction. 0 = no piece moves. Raising it tears the picture further "
        "apart. Tuned for a picture about 480 px high.",
    )
    angle: float = param(
        5.0,
        unit="degrees",
        min=0,
        max=45,
        doc="The most a piece's picture is turned, in degrees, about the piece's own centre: each piece draws its own "
        "random turn up to this far clockwise or anticlockwise. 0 = no piece turns.",
    )
    gap: float = param(
        0.0,
        unit="px",
        min=0,
        max=32,
        doc="The width of the lines between the pieces, in pixels, with anti-aliased edges; 0 = no lines, the pieces "
        "simply meet (below 1 px the lines get fainter). Raising it draws the cracks in `color`. Tuned for a picture "
        "about 480 px high.",
    )
    color: Color = param("#000000", doc="The colour of the lines between the pieces.")
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed for where the pieces lie and how each moves and turns: the same seed always gives the same "
        "break, another seed a different one.",
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps "
        "every value exactly as it is but turned pieces turn jagged), 'bilinear' blends the four pixels around it "
        "(smooth, slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little "
        "slower, and may ring beside hard edges).",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What a moved piece shows where it reads from beyond the picture: 'reflect' (mirrored, edge pixel "
        "repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the "
        "opposite edge).",
        advanced=True,
    )


@effect(category="distortion", params=ShatterParams)
def shatter(rgb: np.ndarray, params: ShatterParams) -> np.ndarray:
    """Break the picture into `pieces` random Voronoi pieces, move and turn each, and draw `gap` px lines between them.

    The pieces are the cells round `pieces` random points. Each draws its own move (up to `offset` px each way) and its
    own turn (up to `angle` degrees, about its point) from `seed`, and shows the picture moved and turned that way:
    one rigid move of the input per piece, so the picture inside a piece is not bent, only cut along the borders. The
    lines of `color` are drawn along the borders of the pieces where they lie, over the moved pictures.
    """
    height, width = rgb.shape[:2]
    rng = np.random.default_rng(params.seed)
    extent = np.array([width, height])  # x then y
    sites = rng.random((params.pieces, len(extent))) * extent  # (x, y), in px from the picture's top left corner
    moves = rng.random((params.pieces, len(extent) + 1)) * 2 - 1  # each piece's share, -1..1, of x, y and the turn
    pieces = cells((height, width), sites)
    centre = sites - 0.5  # a site in pixel indices, where a pixel's centre is a whole number
    moved = centre + moves[:, :2] * params.offset  # where each piece's centre has gone
    angle = np.radians(params.angle) * moves[:, 2]
    # a pixel of a piece reads the picture at the point that the piece's move and turn carry to it: take it back by the
    # move, then back round the piece's centre
    across = np.arange(width)[None, :] - moved[pieces.owner, 0]
    down = np.arange(height)[:, None] - moved[pieces.owner, 1]
    cos, sin = np.cos(angle)[pieces.owner], np.sin(angle)[pieces.owner]
    rows = centre[pieces.owner, 1] - across * sin + down * cos
    cols = centre[pieces.owner, 0] + across * cos + down * sin
    shown = remap(rgb, rows, cols, interpolation=params.interpolation, edge=params.edge)
    line = np.array(params.color, np.float32)
    return shown + (line - shown) * line_coverage(pieces.border_distance, params.gap)[..., None]
