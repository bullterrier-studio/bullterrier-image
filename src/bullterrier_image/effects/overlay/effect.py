"""The `overlay` effect: put a logo or sticker on the photo, in a corner or the middle, using its own transparency."""

import numpy as np

from bullterrier_image.core.params import EffectParams, Picture, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.place import Anchor, Resample, place


class OverlayParams(EffectParams):
    """The logo, its size and where it sits."""

    logo: Picture = param(
        "#ffffff",
        doc="The picture put on the photo: a path in a recipe file (relative to the file; a PNG with transparency "
        "keeps its cut-out shape), a colour such as '#ffffff' (a flat square) or, in Python, a Picture or an array.",
    )
    at: Anchor = param(
        "bottom_right",
        doc="Where the logo sits: 'top_left', 'top', 'top_right', 'left', 'center', 'right', 'bottom_left', "
        "'bottom' or 'bottom_right'.",
    )
    scale: float = param(
        0.2,
        unit="fraction",
        min=0.01,
        max=1,
        doc="The logo's width as a share of the photo's width, 0.01 to 1; its height follows its aspect. Raising it "
        "makes the logo larger.",
    )
    margin: float = param(
        0.03,
        unit="fraction",
        min=0,
        max=0.45,
        doc="Space between the logo and the photo's edge, as a share of the photo's shorter side, 0 to 0.45. "
        "Raising it moves a logo at an edge or corner inward.",
    )
    resample: Resample = param(
        "lanczos",
        advanced=True,
        doc="The filter that sizes the logo: 'nearest' keeps hard pixels, 'bilinear' is soft, 'bicubic' is sharper, "
        "'lanczos' is sharpest and the best for shrinking.",
    )


@effect(category="compositing", params=OverlayParams)
def overlay(rgb: np.ndarray, params: OverlayParams) -> np.ndarray:
    """Put a logo on the photo at one of nine anchors, sized as a share of the photo's width, with its alpha."""
    layer = place(
        params.logo.pixels,
        rgb.shape[:2],
        fit="width",
        at=params.at,
        scale=params.scale,
        margin=params.margin,
        resample=params.resample,
    )
    return np.clip(rgb + layer[..., 3:] * (layer[..., :3] - rgb), 0.0, 1.0).astype(np.float32)
