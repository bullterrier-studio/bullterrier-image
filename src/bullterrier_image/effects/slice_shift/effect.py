"""The `slice_shift` effect: horizontal strips of the picture slid sideways, wrapping around."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.shift import slide_bands


class SliceShiftParams(EffectParams):
    """The settings of `slice_shift`: how many strips, how far they slide, and the random seed."""

    slices: int = param(
        30,
        unit="count",
        min=1,
        max=500,
        doc="How many horizontal strips the picture is cut into, 1 to 500 (never more than it has rows). The cuts fall "
        "at random rows, so the strips differ in height; 1 slides the whole picture as one. More strips make a finer, "
        "busier tear.",
    )
    amount: float = param(
        0.05,
        unit="fraction",
        min=0,
        max=1,
        doc="The furthest a strip slides, as a fraction 0 to 1 of the picture's width: each strip slides by a random "
        "whole number of pixels between that far left and that far right, so a strip may also stay put. 0 leaves the "
        "picture unchanged; 1 can slide a strip by a whole picture width.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed for where the cuts fall and how far each strip slides: the same seed always gives the same "
        "tear, any other seed gives a different one.",
    )


@effect(category="distortion", params=SliceShiftParams)
def slice_shift(rgb: np.ndarray, params: SliceShiftParams) -> np.ndarray:
    """Cut the picture into horizontal strips at random rows and slide each sideways by a random distance.

    A strip that slides out of the picture on one side comes back in on the other (it wraps around), and the strip
    moves by whole pixels, so every row keeps exactly the pixels it had, only in a different order. The cuts and the
    slides depend only on `seed`, `slices`, `amount` and the picture's size.
    """
    height, width = rgb.shape[:2]
    rng = np.random.default_rng(params.seed)
    strips = min(params.slices, height)
    furthest = round(params.amount * width)
    starts = (
        np.sort(rng.choice(height - 1, strips - 1, replace=False)) + 1
    )  # the first row of every strip but the first
    slides = rng.integers(-furthest, furthest + 1, size=strips)
    tops = [0, *starts.tolist()]
    heights = np.diff([*tops, height]).tolist()
    return slide_bands(rgb, zip(tops, heights, slides.tolist(), strict=True), edge="wrap")
