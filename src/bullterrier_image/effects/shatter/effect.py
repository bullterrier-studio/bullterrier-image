"""The `shatter` effect: the picture broken into pieces (Voronoi cells or triangles), each moved, turned and
flattened, with lines between them."""

import math
from typing import Literal

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.sites import detail_weights, sites
from bullterrier_image.ops.triangles import triangles
from bullterrier_image.ops.voronoi import cells, line_coverage
from bullterrier_image.ops.warp import remap


class ShatterParams(EffectParams):
    """Broken glass: pieces, each showing the picture a little moved and turned (or flattened to one colour), divided by
    thin lines."""

    pieces: int = param(
        24,
        unit="count",
        min=4,
        max=500,
        doc="How many points the picture breaks round, 4 to 500: with `shape` 'voronoi' each is a piece (the cell "
        "nearer to it than to any other, so the average piece is the picture's area divided by this); with 'triangles' "
        "the points are corners of triangles, and there are more pieces than points (see `shape`). More = smaller "
        "pieces.",
    )
    shape: Literal["voronoi", "triangles"] = param(
        "voronoi",
        doc="The pieces' shape. 'voronoi' = the cells round the points, as broken glass; 'triangles' = the Delaunay "
        "triangles of the points and of points added round the picture's edge, about as far apart as the others are "
        "(about 4 times the square root of `pieces` of them), as in a low-poly picture. That makes 2 x `pieces` + b - "
        "2 triangles for b edge points: about 2.75 per point at 24 points on a 4:3 picture, fewer per point as "
        "`pieces` grows.",
    )
    detail: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How far the points follow the picture's detail: 0 = they lie anywhere at random; 1 = they gather along "
        "its edges, in proportion to how sharply the brightness changes, so edges break into small pieces and plain "
        "areas into large ones. In between mixes the two. A picture with no edges gets random points whatever this is.",
    )
    fill: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How far each piece is flattened to one colour, the average of what it shows: 0 = each piece shows the "
        "picture (moved and turned), as it is; 1 = each piece is one flat colour, as stained glass or low-poly; in "
        "between mixes the two.",
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
        doc="What lies beyond the picture's edge, for a moved piece that reads from there and for the edges `detail` "
        "looks for at the border (with 'wrap' a mismatch between opposite sides counts as an edge): 'reflect' "
        "(mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel "
        "repeated), 'wrap' (the opposite edge).",
        advanced=True,
    )


@effect(category="distortion", params=ShatterParams)
def shatter(rgb: np.ndarray, params: ShatterParams) -> np.ndarray:
    """Break the picture into pieces (Voronoi cells or triangles), move and turn each, optionally flatten each to its
    mean colour (stained glass, low-poly), and draw `gap` px lines between them.

    The pieces are the Voronoi cells round `pieces` points (random, or gathered along the picture's edges by `detail`),
    or the triangles of those points and of points round the picture's edge (`shape`). Each draws its own move (up to
    `offset` px each way) and its own turn (up to `angle` degrees, about its centre) from `seed`, and shows the picture
    moved and turned that way: one rigid move of the input per piece, so the picture inside a piece is not bent, only
    cut along the borders. `fill` then pulls each piece's colours to their average. The lines of `color` are drawn
    along the borders of the pieces where they lie, over the pictures.
    """
    height, width = rgb.shape[:2]
    rng = np.random.default_rng(params.seed)
    weights = None if params.detail == 0 else detail_weights(rgb, detail=params.detail, edge=params.edge)
    points = sites((height, width), params.pieces, rng, weights=weights)  # (x, y), in px from the top left corner
    if params.shape == "voronoi":
        cut = cells((height, width), points)
        owner, border = cut.owner, cut.border_distance
        centre = points - 0.5  # a site in pixel indices, where a pixel's centre is a whole number
    else:
        mesh = triangles((height, width), points, spacing=math.sqrt(height * width / params.pieces))
        owner, border = mesh.owner, mesh.border_distance
        centre = mesh.vertices.mean(axis=1) - 0.5  # the average of a triangle's corners
    moves = rng.random((len(centre), centre.shape[1] + 1)) * 2 - 1  # each piece's share, -1..1, of x, y and the turn
    moved = centre + moves[:, :2] * params.offset  # where each piece's centre has gone
    angle = np.radians(params.angle) * moves[:, 2]
    # a pixel of a piece reads the picture at the point that the piece's move and turn carry to it: take it back by the
    # move, then back round the piece's centre
    across = np.arange(width)[None, :] - moved[owner, 0]
    down = np.arange(height)[:, None] - moved[owner, 1]
    cos, sin = np.cos(angle)[owner], np.sin(angle)[owner]
    rows = centre[owner, 1] - across * sin + down * cos
    cols = centre[owner, 0] + across * cos + down * sin
    shown = remap(rgb, rows, cols, interpolation=params.interpolation, edge=params.edge)
    if params.fill > 0:  # at 0 the pieces keep the picture they show, and the averages are not needed
        size = np.bincount(owner.ravel(), minlength=len(centre))
        channels = [
            np.bincount(owner.ravel(), weights=shown[..., c].ravel(), minlength=len(centre))
            for c in range(shown.shape[-1])
        ]
        average = (np.stack(channels, axis=1) / np.maximum(size, 1)[:, None]).astype(np.float32)  # empty piece: unused
        shown = shown * (1 - params.fill) + average[owner] * params.fill
    line = np.array(params.color, np.float32)
    return shown + (line - shown) * line_coverage(border, params.gap)[..., None]
