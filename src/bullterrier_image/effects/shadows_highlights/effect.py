"""The `shadows_highlights` effect: recover detail in dark and bright areas, judged by their local surroundings."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.color import luminance
from bullterrier_image.ops.guided import guided_filter


class ShadowsHighlightsParams(EffectParams):
    """How much to recover in the dark areas and in the bright areas, and how local 'dark' and 'bright' are."""

    shadows: float = param(
        0.5,
        unit="ratio",
        min=0,
        max=4,
        doc="How much dark areas are lightened: 0 = off. In the darkest surroundings a value v becomes "
        "v^(1 / (1 + shadows)), a gamma of 1 + shadows (1 = square root, 3 = fourth root); surroundings that are "
        "brighter are lifted less, and areas brighter than mid grey hardly at all. Black stays black.",
    )
    highlights: float = param(
        0.0,
        unit="ratio",
        min=0,
        max=4,
        doc="How much bright areas are darkened: 0 = off. In the brightest surroundings a value v becomes "
        "v^(1 + highlights), a gamma of 1 + highlights (1 = squared, 3 = to the fourth power); surroundings that "
        "are darker are darkened less, and areas darker than mid grey hardly at all. White stays white.",
    )
    radius: float = param(
        30.0,
        unit="px",
        min=0,
        max=500,
        doc="Size of the surroundings that decide whether an area is dark or bright, in px at the recipe's size "
        "(rounded to whole px): the base layer is an average over a square of 2 x radius + 1 px. Larger judges "
        "bigger regions and recovers whole dark or bright areas; smaller follows fine detail; 0 judges every pixel "
        "by its own brightness (a plain tone curve).",
    )
    edge_threshold: float = param(
        0.1,
        unit="fraction",
        min=0.001,
        max=1,
        doc="The brightness step (0..1, Rec. 709 luma) that counts as an edge the base layer must not blur across: "
        "steps above it keep their own side of the base, which avoids halos around them; steps below it are "
        "averaged away. Raising it makes the base smoother and more like a plain blur; lowering it follows ever "
        "smaller edges.",
        advanced=True,
    )


@effect(category="light", params=ShadowsHighlightsParams)
def shadows_highlights(rgb: np.ndarray, params: ShadowsHighlightsParams) -> np.ndarray:
    """Lighten the dark areas and darken the bright ones of the picture, as judged by an edge-aware local brightness.

    The base layer B (0..1) is the Rec. 709 luma of the sRGB-encoded picture smoothed by a guided filter (He, Sun and
    Tang 2010, guided by the luma itself) over a window of 2 x `radius` + 1 px, which averages flat areas but keeps
    its edges. Every channel c then becomes c^e with e = (1 + `highlights` B^2) / (1 + `shadows` (1 - B)^2): a gamma
    curve whose exponent follows the surroundings B, not the pixel, so a dark pixel in a bright area is lifted less
    than the same pixel in a dark one. Raising `shadows` only lowers e (lifting c towards 1) and raising
    `highlights` only raises it (pulling c towards 0), black and white stay where they are, and 0..1 maps into 0..1.
    """
    luma = luminance(rgb)
    base = guided_filter(luma, luma, radius=round(params.radius), epsilon=params.edge_threshold**2)
    exponent = (1.0 + params.highlights * base**2) / (1.0 + params.shadows * (1.0 - base) ** 2)
    return (rgb ** exponent[..., None]).astype(np.float32)
