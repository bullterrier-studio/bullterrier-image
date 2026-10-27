"""The `parallax` effect: the picture's layers shifted by their depth (or any field), as if the camera moved
sideways."""

from typing import Literal

import numpy as np
from scipy import ndimage

from bullterrier_image.core.params import EffectParams, FieldName, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.warp import remap


class ParallaxParams(EffectParams):
    """A sideways camera move: layers nearer than `focus` shift along `angle`, farther ones the opposite way, each by
    its distance in depth from the focus."""

    field: FieldName = param(
        "depth",
        doc="The field read as how near each layer is, 1 = nearest: `depth` (the scene's depth), `subject` (the "
        "subject moves as one layer against its background), `light` (bright parts move as nearer), or a region of "
        "the recipe.",
    )
    angle: float = param(
        0.0,
        unit="degrees",
        min=0,
        max=360,
        cyclic=True,
        doc="The direction the nearest layers move, in degrees clockwise from pointing right, as seen on screen: 0 = "
        "right, 90 = down, 180 = left. Layers farther than the focus move the opposite way.",
    )
    amount: float = param(
        9.6,
        unit="px",
        min=0,
        max=48,
        doc="How far the nearest layer (depth 1) moves when the focus is the farthest depth, 0 to 48 px: a layer "
        "moves amount x (its depth - focus) px. Raising it exaggerates the shift; 0 leaves the picture unchanged.",
    )
    focus: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The depth that stays still, as a fraction of the picture's depth range: 1 = the nearest, 0 = the "
        "farthest. Nearer layers move along `angle`, farther ones the opposite way; raising it moves more of the "
        "picture against `angle`.",
    )
    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear",
        doc="How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps "
        "every value exactly as it is but edges turn jagged), 'bilinear' blends the four pixels around it (smooth, "
        "slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, "
        "and may ring beside hard edges).",
        advanced=True,
    )
    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What a pixel near the border sees when its shift reads from beyond the picture: 'reflect' (mirrored, "
        "edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' "
        "(the opposite edge).",
        advanced=True,
    )


@effect(category="distortion", params=ParallaxParams)
def parallax(rgb: np.ndarray, params: ParallaxParams, *, field: np.ndarray) -> np.ndarray:
    """Shift every layer of the picture by amount x (depth - focus) px along `angle`, as a sideways camera
    move would.

    Each output pixel reads the picture that far back along `angle`, taking its depth as the nearest within reach of
    the largest shift, so a near object lands whole over what lies behind it. Background it uncovers is the background
    beside it stretched over the gap: a 2.5D look, not a new view.

    `field` is filled in by the recipe from the selection `params.field` names: float32 (h, w), 0..1, 1 = nearest.
    """
    height, width = rgb.shape[:2]
    reach = params.amount  # px a layer moves per unit of depth from the focus
    # A pixel reads where it lies minus its shift, so a pixel a near object lands on must take the object's shift, not
    # its own (farther) one: the depth is replaced by the nearest within the largest shift, ceil(reach x the farthest
    # depth from the focus) px, so near objects move whole instead of tearing. A square is the simplest window that
    # covers every direction, and its maximum is separable (fast at any size).
    radius = int(np.ceil(reach * max(params.focus, 1 - params.focus)))
    nearest = ndimage.maximum_filter(field, size=2 * radius + 1, mode=params.edge)
    shift = reach * (nearest - params.focus)  # at the depth's own precision: a depth equal to the focus moves 0 px
    angle = np.radians(params.angle)
    rows = np.arange(height)[:, None] - shift * np.sin(angle)  # rows count down: a move down reads from above
    cols = np.arange(width)[None, :] - shift * np.cos(angle)
    return remap(rgb, rows, cols, interpolation=params.interpolation, edge=params.edge)
