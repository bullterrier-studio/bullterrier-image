"""The `slice_shift` effect: horizontal strips of the picture slid sideways, wrapping around."""

from typing import Self

import numpy as np
from pydantic import model_validator

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.shift import slide_bands


class SliceShiftParams(EffectParams):
    """The settings of `slice_shift`: the tear, the bands, how far they slide, and the random seed."""

    slices: int = param(
        30,
        unit="count",
        min=0,
        max=500,
        doc="How many horizontal strips the whole picture is cut into, 0 to 500 (never more than it has rows). The "
        "cuts fall at random rows, so the strips differ in height; 1 slides the whole picture as one; 0 = no tear, "
        "only the bands move. More strips make a finer, busier tear.",
    )
    amount: float = param(
        24.0,
        unit="px",
        min=0,
        max=2000,
        doc="The furthest a strip or band slides, in px, 0 to 2000, tuned for a picture about 480 px high: each "
        "slides by a random whole number of pixels between that far left and that far right, so one may also stay "
        "put. 0 leaves the picture unchanged; the picture's width or more can slide one by a whole picture width.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed for where the cuts and the bands fall and how far each slides: the same seed always gives the "
        "same tear, any other seed gives a different one.",
    )
    bands: int = param(
        0,
        unit="count",
        min=0,
        max=500,
        doc="How many extra strips slide, 0 to 500, on top of the tear: each is placed at a random row, so they may "
        "overlap (a row in two slides by both) and the rest of the picture stays; 0 = none. A few bands with `slices` "
        "0 make a corrupted video frame.",
    )
    band_min_height: float = param(
        4.8,
        unit="px",
        min=0,
        max=2000,
        doc="The lowest a band can be, in px, 0 to 2000, tuned for a picture about 480 px high (never below 1 px). "
        "Each band's height is drawn at random between this and `band_max_height`, which it may not exceed.",
    )
    band_max_height: float = param(
        28.8,
        unit="px",
        min=0,
        max=2000,
        doc="The tallest a band can be, in px, 0 to 2000, tuned for a picture about 480 px high. Raising it gives "
        "taller bands.",
    )

    @model_validator(mode="after")
    def _band_heights_in_order(self) -> Self:
        """The band height limits are a range: refused, naming both, when the lowest is above the tallest."""
        if self.band_min_height > self.band_max_height:
            raise ValueError(
                f"`band_min_height` ({self.band_min_height}) must not exceed `band_max_height` "
                f"({self.band_max_height}): they are the lowest and the tallest a band can be"
            )
        return self


@effect(category="distortion", params=SliceShiftParams)
def slice_shift(rgb: np.ndarray, params: SliceShiftParams) -> np.ndarray:
    """Glitch tearing: cut the picture into horizontal strips at random rows and slide each sideways by a random
    distance, then slide `bands` more strips of random height placed at random rows.

    A strip that slides out of the picture on one side comes back in on the other (it wraps around), and the strip
    moves by whole pixels, so every row keeps exactly the pixels it had, only in a different order. The cuts, the
    bands and the slides depend only on `seed`, `slices`, `bands`, `amount` and the picture's size.
    """
    height = len(rgb)
    rng = np.random.default_rng(params.seed)
    furthest = round(params.amount)
    tear: list[tuple[int, int, int]] = []
    if params.slices:
        strips = min(params.slices, height)
        starts = np.sort(rng.choice(height - 1, strips - 1, replace=False)) + 1  # every strip's first row but the top's
        tops = [0, *starts.tolist()]  # constant: the first strip starts at the top row
        heights = np.diff([*tops, height]).tolist()
        slides = rng.integers(-furthest, furthest + 1, size=strips).tolist()
        tear = list(zip(tops, heights, slides, strict=True))
    placed = []
    for _ in range(params.bands):
        band_height = min(height, max(1, int(rng.uniform(params.band_min_height, params.band_max_height))))
        top = int(rng.uniform(0, height - band_height))
        placed.append((top, band_height, int(rng.uniform(-params.amount, params.amount))))
    return slide_bands(rgb, tear + placed, edge="wrap")
