"""The `dust_scratches` effect: light and dark specks, hairs and scratches drawn on the picture, like a worn print."""

import numpy as np

from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect


class DustScratchesParams(EffectParams):
    """Dust and scratches: marks in a light and a dark colour at random places, as on a dirty scan or a worn print."""

    density: float = param(
        0.6,
        unit="count",
        min=0,
        max=5,
        doc="How many specks and hairs there are: marks per 1000 square px of picture (a 32 x 32 px area), so the "
        "same density looks alike on any picture size. 0 = none, 0.6 is a light dusting, 3 is filthy.",
    )
    size: float = param(
        8.0,
        unit="px",
        min=1,
        max=50,
        doc="The size of the biggest dust mark in px: a speck's diameter or a hair's length. Most marks are smaller "
        "(speck radii are random, with many tiny ones); larger = bigger blotches and longer hairs.",
    )
    hairs: float = param(
        0.15,
        unit="fraction",
        min=0,
        max=1,
        doc="The share of the dust marks drawn as hairs (short thin lines in any direction) instead of round specks: "
        "0 = only specks, 1 = only hairs.",
        advanced=True,
    )
    scratches: int = param(
        2,
        unit="count",
        min=0,
        max=100,
        doc="How many scratches: thin, long, nearly vertical lines, each running over half to all of the picture's "
        "height. 0 = none.",
    )
    angle: float = param(
        4.0,
        unit="degrees",
        min=0,
        max=45,
        doc="The most a scratch may lean away from vertical, in degrees: each scratch gets a random lean up to this, "
        "to either side. 0 = all exactly vertical.",
        advanced=True,
    )
    width: float = param(
        1.0,
        unit="px",
        min=0.5,
        max=10,
        doc="How wide a scratch is in px. Larger = bolder scratches.",
        advanced=True,
    )
    light: Color = param(
        "#ffffff",
        doc="The colour of the light marks (dust on a dark print, scratches into the emulsion): '#rrggbb', '#rgb' or "
        "[r, g, b] 0..1. Each mark is light or dark with equal chance.",
    )
    dark: Color = param(
        "#000000",
        doc="The colour of the dark marks (dust on the scanner glass): '#rrggbb', '#rgb' or [r, g, b] 0..1.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always puts the same marks in the same places, another seed others.",
    )


def _stamp(cover: np.ndarray, start: tuple[float, float], end: tuple[float, float], radius: float) -> None:
    """Draw the segment from `start` to `end` (x, y; a speck has equal ends), `radius` px wide each side, into the
    (h, w) array `cover`: each pixel takes the larger of its value and how far the shape covers it, 0..1, soft by one
    pixel."""
    height, width = cover.shape
    reach = radius + 1
    top, bottom = max(int(min(start[1], end[1]) - reach), 0), min(int(max(start[1], end[1]) + reach) + 1, height)
    left, right = max(int(min(start[0], end[0]) - reach), 0), min(int(max(start[0], end[0]) + reach) + 1, width)
    rows, cols = np.mgrid[top:bottom, left:right]
    along = (end[0] - start[0], end[1] - start[1])
    # how far along the segment (0..1) the nearest point to each pixel is; a speck has no length to divide by
    # constant: a tiny length added so a speck's zero length divides to 0, not NaN
    fraction = ((cols - start[0]) * along[0] + (rows - start[1]) * along[1]) / (along[0] ** 2 + along[1] ** 2 + 1e-9)
    nearest = np.clip(fraction, 0.0, 1.0)
    distance = np.hypot(cols - start[0] - nearest * along[0], rows - start[1] - nearest * along[1])
    # a pixel is a unit box: covered fully within `radius` - 0.5 of the shape, not at all beyond `radius` + 0.5
    cover[top:bottom, left:right] = np.maximum(
        cover[top:bottom, left:right], np.clip(radius + 0.5 - distance, 0.0, 1.0)
    )


@effect(category="texture", params=DustScratchesParams)
def dust_scratches(rgb: np.ndarray, params: DustScratchesParams) -> np.ndarray:
    """Draw light and dark specks, hairs and scratches at random places over the picture.

    `density` marks per 1000 px² are specks (random radius up to `size` / 2, mostly small) or, for a share `hairs` of
    them, one-pixel hairs up to `size` px long; `scratches` long lines lean up to `angle` from vertical and are `width`
    wide. Each mark is `light` or `dark` with equal chance, solid inside, with antialiased edges (where a light mark
    crosses a dark one the light one lies on top). A higher `density` keeps every mark of a lower one and adds more;
    the same `seed` always gives the same marks.
    """
    height, width = rgb.shape[:2]
    light, dark = np.zeros((height, width), np.float32), np.zeros((height, width), np.float32)
    # constant: density is counted per 1000 square px
    dust = np.random.default_rng(params.seed).random((round(params.density * height * width / 1000), 6))
    for x, y, extent, tone, kind, direction in dust:
        centre = (x * width, y * height)
        if kind < params.hairs:  # a hair, up to `size` long, in any direction
            reach = params.size * (0.5 + extent / 2)
            turn = 2 * np.pi * direction
            end = (centre[0] + reach * np.cos(turn), centre[1] + reach * np.sin(turn))
            # constant: a hair is one pixel wide, half a pixel each side of its line
            _stamp(light if tone < 0.5 else dark, centre, end, 0.5)
        else:  # a speck: many small, few large
            _stamp(light if tone < 0.5 else dark, centre, centre, params.size / 2 * extent**2)
    # constant: stream number 1, apart from the dust's (stream 0)
    lines = np.random.default_rng([params.seed, 1]).random((params.scratches, 4))
    for x, extent, tone, lean in lines:
        reach = height * (0.5 + extent / 2)
        turn = np.radians(params.angle) * (2 * lean - 1)
        start = (x * width, (height - reach) * extent)
        end = (start[0] + reach * np.tan(turn), start[1] + reach)
        _stamp(light if tone < 0.5 else dark, start, end, params.width / 2)
    out = rgb.astype(np.float32)
    for cover, colour in ((dark, params.dark), (light, params.light)):
        weight = cover[..., None]
        out = out * (1 - weight) + np.asarray(colour, np.float32) * weight
    return out.astype(np.float32)
