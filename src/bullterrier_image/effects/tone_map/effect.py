"""The `tone_map` effect: local tone mapping that compresses broad brightness differences and keeps fine detail."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.color import luminance
from bullterrier_image.ops.guided import guided_filter

# constant: luminance below this (about 13 stops under white) counts as this much in the logarithm, so black has a
# finite logarithm; a guard against log(0), as in Reinhard et al. 2002's small delta
_LUMINANCE_FLOOR = 1e-4


class ToneMapParams(EffectParams):
    """Local tone mapping: how much the broad brightness differences are compressed and how much detail is added."""

    contrast: float = param(
        0.5,
        unit="ratio",
        min=0,
        max=1,
        doc="How much of the broad brightness differences between areas is kept: 1 = all of them (no compression), "
        "0.5 = halved in stops (a picture with dark areas three stops below its bright ones gets them one and a "
        "half apart), 0 = every area brought up to white and only the detail left. Lowering it lightens dark "
        "areas the most; white stays white.",
    )
    amount: float = param(
        0.5,
        unit="ratio",
        min=0,
        max=5,
        doc="How strongly the fine detail (what is left of the brightness once the broad areas are taken out) is "
        "exaggerated: 0 = kept as it is, 1 = doubled in stops. Raising it brings out texture; it can clip "
        "highlights.",
    )
    radius: float = param(
        30.0,
        unit="px",
        min=0,
        max=500,
        doc="Size of what counts as a broad area, in px at the recipe's size (rounded to whole px): the base layer "
        "is an average over a square of 2 x radius + 1 px. Larger treats bigger structures as base and keeps "
        "only finer ones as detail; 0 makes every pixel its own base, a plain global curve (a grey v becomes "
        "v^contrast).",
    )
    edge_threshold: float = param(
        0.5,
        unit="stops",
        min=0.01,
        max=8,
        doc="The brightness step, in stops, that counts as an edge the base layer must not blur across: steps above it "
        "keep their own side of the base, which avoids halos around them; steps below it are averaged away and "
        "count as detail. Raising it makes the base smoother and more like a plain blur.",
        advanced=True,
    )


@effect(category="light", params=ToneMapParams)
def tone_map(rgb: np.ndarray, params: ToneMapParams) -> np.ndarray:
    """Compress the broad brightness differences of the picture and keep (or boost) its fine detail.

    Brightness is the base 2 logarithm L of the Rec. 709 luma Y of the sRGB-encoded picture (Y floored at 1e-4), in
    stops below white. A guided filter (He, Sun and Tang 2010, guided by L itself) over a window of 2 x `radius` + 1 px,
    edge-preserving by `edge_threshold` stops, splits it into a base B and detail D = L - B (Durand and Dorsey 2002).
    The new log brightness is `contrast` x B + (1 + `amount`) x D, and every pixel's red, green and blue are
    multiplied by 2^(new - L) = 2^((`contrast` - 1) B + `amount` D), so hue and saturation stay and the
    result is held to 0..1. An even area of luma Y comes out as Y^`contrast`. B is an average of values of L, so it
    never leaves their range: with `amount` 0 lowering `contrast` only ever brightens.
    """
    log_luma = np.log2(np.maximum(luminance(rgb), _LUMINANCE_FLOOR))
    base = guided_filter(log_luma, log_luma, radius=round(params.radius), epsilon=params.edge_threshold**2)
    gain = np.exp2((params.contrast - 1.0) * base + params.amount * (log_luma - base))
    return np.clip(rgb * gain[..., None], 0.0, 1.0).astype(np.float32)
