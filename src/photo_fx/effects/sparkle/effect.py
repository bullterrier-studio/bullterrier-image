"""The `sparkle` effect: glints, a soft dot with a cross of rays, on the brightest places of the picture."""

import numpy as np

from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.color import luminance
from photo_fx.ops.particles import draw_segments


class SparkleParams(EffectParams):
    """Sparkle: glints with four rays on the highlights, like light catching on water, metal or glass."""

    count: int = param(
        60,
        unit="count",
        min=0,
        max=2000,
        doc="How many sparkles: at most this many, each on a different pixel brighter than `threshold` (fewer when "
        "the picture has fewer such pixels). 0 = none (the picture unchanged).",
    )
    threshold: float = param(
        0.75,
        unit="fraction",
        min=0,
        max=1,
        doc="How bright a pixel must be to get a sparkle: its brightness (Rec. 709 luminance, 0..1) must be above "
        "this. Raising it keeps the sparkles to the brightest highlights; 1 = none, nothing is brighter than white.",
    )
    size: float = param(
        10.0,
        unit="px",
        min=1,
        max=100,
        doc="The diameter in px of the biggest sparkles' soft dot; the smallest are half as wide. Tuned for a picture "
        "about 480 px high. Larger = bigger glints.",
    )
    rays: float = param(
        2.0,
        unit="ratio",
        min=0,
        max=10,
        doc="How long the four rays of the cross are, as a multiple of the sparkle's dot diameter: each ray runs "
        "this far from the middle, straight left, right, up and down, one pixel thin. 0 = a soft dot only, no rays.",
    )
    opacity: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How bright the biggest sparkles are: the share of white they screen over the picture at their middle; "
        "smaller ones are fainter. 0 = invisible, 1 = solid white.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always puts the same sparkles on the same picture, another seed others.",
    )


@effect(category="atmosphere", params=SparkleParams)
def sparkle(rgb: np.ndarray, params: SparkleParams) -> np.ndarray:
    """Screen sparkles over `count` random pixels brighter than `threshold`: a soft dot with four rays each.

    A sparkle has a random scale s from 0.5 to 1: its dot is `size` x s wide (soft: full brightness inside half its
    radius, fading to nothing at one and a half), its rays are `rays` x `size` x s long, and it is `opacity` x s
    bright, so the small ones are fainter too. Sparkles are screened over the picture, so they only lighten it; the
    same `seed` on the same picture always gives the same sparkles.
    """
    height, width = rgb.shape[:2]
    rng = np.random.default_rng(params.seed)
    highlights = np.flatnonzero(luminance(rgb) > params.threshold)
    chosen = rng.choice(highlights, size=min(params.count, len(highlights)), replace=False)
    scale = 0.5 + 0.5 * rng.random(len(chosen))
    centre = np.stack([chosen % width, chosen // width], axis=1).astype(np.float64)
    brightness = params.opacity * scale
    radius = params.size / 2 * scale
    dots = draw_segments((height, width), centre, centre, radius, brightness, radius)
    # the rays: from the middle left, right, up and down, each a hairline
    reach = params.rays * params.size * scale[:, None]
    # constant: the unit vectors of the four ray directions
    tips = [centre + reach * np.array(direction) for direction in ((1, 0), (-1, 0), (0, 1), (0, -1))]
    # constant: a hairline is half a pixel to each side of its line, with a one-pixel antialiased edge
    rays = draw_segments(
        (height, width), np.tile(centre, (len(tips), 1)), np.concatenate(tips), 0.5, np.tile(brightness, len(tips)), 1.0
    )
    return (rgb + np.maximum(dots, rays)[..., None] * (1 - rgb)).astype(np.float32)
