"""The `pattern` effect: stripes, checks, dots or a grid in two colours in place of the picture."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect


class PatternParams(EffectParams):
    """A regular pattern of `cell` px, drawn in a foreground colour on a background colour."""

    kind: Literal["stripes", "checks", "dots", "grid"] = param(
        "grid",
        doc="The pattern: 'stripes' (parallel bands, `thickness` wide), 'checks' (a chessboard of `cell` px squares, "
        "the top-left one in the foreground colour), 'dots' (a disc of `radius` in every cell) or 'grid' (lines "
        "`thickness` wide along the cell borders).",
    )
    cell: float = param(
        32.0,
        unit="px",
        min=2,
        max=512,
        doc="The size of one repeat in px: the distance from one stripe to the next, the side of a check, a dot or a "
        "grid square. Tuned for a picture about 480 px high; larger = a coarser pattern.",
    )
    angle: float = param(
        0.0,
        unit="degrees",
        min=0,
        max=360,
        cyclic=True,
        doc="The direction the pattern varies along, in degrees clockwise from pointing right, about the picture's "
        "top-left corner: at 0 it changes from left to right, so the stripes are vertical and the checks, dots and "
        "grid lines are aligned with the picture's edges; at 90 it changes downward and the stripes are horizontal.",
    )
    foreground: Color = param("#ffffff", doc="The colour of the stripes, the dark checks, the dots and the grid lines.")
    background: Color = param("#101018", doc="The colour between them.")
    thickness: float = param(
        0.2,
        unit="fraction",
        min=0,
        max=1,
        doc="Used by 'stripes' and 'grid': the width of a stripe or a grid line as a fraction of `cell`; 1 fills "
        "everything with the foreground colour. Ignored by 'checks' and 'dots'.",
    )
    radius: float = param(
        0.3,
        unit="fraction",
        min=0,
        max=0.75,
        doc="Used by 'dots' only: the radius of a dot as a fraction of `cell`; 0.5 touches the neighbours, above 0.71 "
        "the dots overlap into a filled picture. Ignored by 'stripes', 'checks' and 'grid'.",
    )
    feather: float = param(
        1.0,
        unit="px",
        min=0,
        max=32,
        doc="The width of the soft edge in px, centred on the shape's border and blended linearly: 0 = hard "
        "(jagged) edges, 1 = anti-aliased, larger = blurred shapes.",
    )


@effect(category="backgrounds", params=PatternParams)
def pattern(rgb: np.ndarray, params: PatternParams) -> np.ndarray:
    """Replace the picture with a repeating pattern of stripes, checks, dots or a grid in two colours.

    A generated background: the input is used only for its height and width, its content is ignored. The pattern is
    anchored at the top-left corner, so it does not shift when the picture is resized. Every shape is drawn from
    its signed distance in px to its border, so edges have the same softness at every angle.
    """
    height, width = rgb.shape[:2]
    cell = params.cell
    angle = np.radians(np.float32(params.angle))
    x = np.arange(width, dtype=np.float32)[None, :] + 0.5  # pixel centres
    y = np.arange(height, dtype=np.float32)[:, None] + 0.5
    u = x * np.cos(angle) + y * np.sin(angle)  # position along the pattern's own axes, px
    v = y * np.cos(angle) - x * np.sin(angle)
    in_u, in_v = u % cell, v % cell  # position inside the cell
    near_u, near_v = np.minimum(in_u, cell - in_u), np.minimum(in_v, cell - in_v)  # distance to the nearest border
    half_line = params.thickness * cell / 2
    if params.kind == "stripes":
        inside = half_line - np.abs(in_u - cell / 2)
    elif params.kind == "grid":
        inside = half_line - np.minimum(near_u, near_v)
    elif params.kind == "dots":
        inside = params.radius * cell - np.hypot(in_u - cell / 2, in_v - cell / 2)
    else:
        even = (np.floor(u / cell) + np.floor(v / cell)) % 2 == 0
        inside = np.minimum(near_u, near_v) * np.where(even, 1.0, -1.0)
    # `inside` is the signed distance in px to the foreground's border, positive within it
    coverage = (inside > 0) if params.feather == 0 else np.clip(inside / params.feather + 0.5, 0.0, 1.0)
    coverage = coverage[..., None].astype(np.float32)
    foreground = np.array(params.foreground, np.float32)
    background = np.array(params.background, np.float32)
    return (background + (foreground - background) * coverage).astype(np.float32)
