"""The `voronoi_cells` effect: random cells (a Voronoi diagram) filled from a colour ramp, in place of the picture."""

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.ramp import ramp
from bullterrier_image.ops.voronoi import cells, line_coverage


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
        doc="Seeds the random positions and colours: the same seed gives the same cells.",
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
    found = cells((height, width), sites)
    dome = found.site_distance / (found.site_distance + found.border_distance)  # 0 at the site, 1 on the border
    out = palette[found.owner] * (1 - params.shade * dome)[..., None]
    line = np.array(params.line_color, np.float32)
    out = out + (line - out) * line_coverage(found.border_distance, params.line_width)[..., None]
    return out.astype(np.float32)
