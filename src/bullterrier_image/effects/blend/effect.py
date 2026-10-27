"""The `blend` effect: lay a second picture over the photo with a blend mode (multiply a texture, screen a light, put
a logo on it...)."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, Picture, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.composite import BlendMode, composite
from bullterrier_image.ops.place import Anchor, Resample, place


class BlendParams(EffectParams):
    """The picture, how it is fitted to the photo, and the blend mode."""

    image: Picture = param(
        "#808080",
        doc="The picture laid over the photo (a texture, a light leak): a path in a recipe file (relative to the "
        "file), a colour such as '#808080' (a flat picture) or, in Python, a Picture or an array. Its own "
        "transparency is respected.",
    )
    mode: BlendMode = param(
        "multiply",
        doc="How the picture combines with the photo, as the W3C Compositing and Blending Level 1 modes: 'normal' "
        "shows the picture, 'multiply' darkens, 'screen' lightens, 'overlay' deepens contrast by the photo's tones, "
        "'soft_light' does so gently by the picture's, 'difference' subtracts, 'color' gives the photo the picture's "
        "hue and saturation and keeps its brightness. Mid grey leaves the photo unchanged under 'overlay' and "
        "'soft_light'.",
    )
    fit: Literal["cover", "contain", "stretch", "width", "tile"] = param(
        "cover",
        doc="How the picture is fitted to the photo: 'cover' fills it keeping the aspect and crops the overhang, "
        "'contain' fits inside it keeping the aspect (the rest of the photo is untouched), 'stretch' fills it "
        "ignoring the aspect, 'width' keeps the aspect and makes the picture as wide as the photo times `scale` (a "
        "logo or sticker), 'tile' repeats that picture over the photo.",
    )
    at: Anchor = param(
        "center",
        doc="Where the fitted picture sits, or which tile sits there: 'top_left', 'top', 'top_right', 'left', "
        "'center', 'right', 'bottom_left', 'bottom' or 'bottom_right'. It decides what 'cover' crops.",
    )
    scale: float = param(
        1.0,
        unit="ratio",
        min=0.01,
        max=2,
        doc="The fitted picture's size as a multiple of the size `fit` gives it: 1 as fitted, 0.5 half as wide and "
        "tall. With 'width' it is the picture's width as a share of the photo's (0.2 a fifth); with 'tile', 0.25 "
        "puts four tiles across the photo.",
    )
    margin: float = param(
        0.0,
        unit="px",
        min=0,
        max=216,
        doc="Space kept clear around the fitted picture on every side, 0 to 216 px (never more than leaves a pixel "
        "of the shorter side). Raising it shrinks what 'cover', 'contain' and 'stretch' fill and "
        "moves a smaller picture at an edge or corner inward.",
    )
    resample: Resample = param(
        "lanczos",
        advanced=True,
        doc="The filter that sizes the picture: 'nearest' keeps hard pixels, 'bilinear' is soft, 'bicubic' is "
        "sharper, 'lanczos' is sharpest and the best for shrinking.",
    )


@effect(category="compositing", params=BlendParams)
def blend(rgb: np.ndarray, params: BlendParams) -> np.ndarray:
    """Lay a second picture over the photo with a blend mode (overlay a logo, double exposure), fitted by cover,
    contain, stretch, width or tile."""
    layer = place(
        params.image.pixels,
        rgb.shape[:2],
        fit=params.fit,
        at=params.at,
        scale=params.scale,
        margin=params.margin,
        resample=params.resample,
    )
    return composite(rgb, layer[..., :3], params.mode, layer[..., 3:])
