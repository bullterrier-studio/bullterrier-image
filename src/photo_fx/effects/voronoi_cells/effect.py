"""The `voronoi_cells` effect: random cells (a Voronoi diagram) filled from a colour ramp, in place of the picture."""

import numpy as np
from scipy.spatial import Delaunay, cKDTree

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.ramp import ramp


class VoronoiCellsParams(EffectParams):
    """Random points, each owning the area nearer to it than to any other, filled with a colour and lined."""

    cells: int = param(
        80,
        unit="count",
        min=4,
        max=2000,
        doc="How many cells (random points, the sites) the picture is divided into. More = smaller cells; the "
        "average cell is the picture's area divided by this.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**31 - 1,
        doc="Seeds the random positions and colours: the same seed gives the same cells. The sites are "
        "numpy.random.default_rng(seed).random((cells, 2)), columns x and y as fractions of the width and the "
        "height; each cell's value on the colour list is drawn next.",
    )
    colors: tuple[Color, ...] = param(
        ("#12002b", "#6a1bff", "#ff2bd6", "#ffb347"),
        doc="The colour ramp, evenly spaced; at least two. Each cell takes the colour at its own random position "
        "along it, so a longer list gives more variety.",
        min_length=2,
    )
    shade: float = param(
        0.35,
        unit="fraction",
        min=0,
        max=1,
        doc="How much each cell darkens away from its site: 0 = flat colour, 1 = black on the cell's border. The "
        "darkening is the distance to the site over the distance to the site plus the distance to the border, so "
        "it grows steadily from the site to the border and gives every cell a dome look.",
    )
    line_width: float = param(
        1.5,
        unit="px",
        min=0,
        max=32,
        doc="The width of the border lines between cells in px, with anti-aliased edges; 0 = no borders (below 1 px "
        "the lines get fainter). Tuned for a picture about 480 px high.",
    )
    line_color: Color = param("#0a0014", doc="The colour of the border lines.")


@effect(category="backgrounds", params=VoronoiCellsParams)
def voronoi_cells(rgb: np.ndarray, params: VoronoiCellsParams) -> np.ndarray:
    """Replace the picture with a Voronoi diagram: random cells filled from a colour ramp, with thin borders.

    A generated background: the input is used only for its height and width, its content is ignored. Every pixel
    belongs to the cell of its nearest site; a cell's colour is a random point on `colors`, darkened toward its
    border by `shade`, and the border lines are drawn in `line_color`.
    """
    height, width = rgb.shape[:2]
    rng = np.random.default_rng(params.seed)
    extent = np.array([width, height])  # x then y
    sites = rng.random((params.cells, len(extent))) * extent
    palette = ramp(rng.random(params.cells), params.colors)
    y, x = np.mgrid[0:height, 0:width]
    px, py = (x.ravel() + 0.5).astype(np.float32), (y.ravel() + 0.5).astype(np.float32)
    own_distance, own = cKDTree(sites).query(np.stack([px, py], axis=1))
    # A pixel's distance to the border of its cell is its least distance to the bisector line of its site and one of the
    # site's neighbours (the cell is the intersection of the half-planes that line bounds). Neighbours are the sites
    # the Delaunay triangulation joins to it; short lists are padded by repeating one, which changes no minimum.
    indptr, indices = Delaunay(sites).vertex_neighbor_vertices
    slots = np.arange(np.diff(indptr).max())
    slot = np.minimum(slots[None, :], np.diff(indptr)[:, None] - 1)
    neighbour = indices[indptr[:-1, None] + slot]  # (cells, most neighbours of any site)
    away = sites[neighbour] - sites[:, None, :]  # from each site to each of its neighbours
    normal = (away / np.linalg.norm(away, axis=-1, keepdims=True)).astype(np.float32)  # unit, pointing out of the cell
    offset = (normal * (sites[:, None, :] + sites[neighbour]) / 2).sum(axis=-1).astype(np.float32)  # normal . midpoint
    border = np.full(px.shape, np.inf, np.float32)
    for k in slots:
        np.minimum(border, offset[own, k] - normal[own, k, 0] * px - normal[own, k, 1] * py, out=border)
    dome = own_distance / (own_distance + border)  # 0 at the site, 1 on the border
    out = palette[own] * (1 - params.shade * dome)[:, None]
    # coverage of a pixel by a line `line_width` wide: a one-pixel ramp across the line's edge, fainter below 1 px
    # constant: a pixel is one px wide, so its edge ramp spans 1 px (half of it each side)
    coverage = np.clip(params.line_width / 2 - border + 0.5, 0.0, 1.0) * min(params.line_width, 1.0)
    line = np.array(params.line_color, np.float32)
    out = out + (line - out) * coverage[:, None]
    return out.reshape(height, width, 3).astype(np.float32)
