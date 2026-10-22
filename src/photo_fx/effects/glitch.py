"""Digital-damage look: colour channels pulled apart, horizontal bands knocked sideways, some of them turned to text."""

import math

import numpy as np

from photo_fx.core.registry import effect
from photo_fx.ops.color import parse_color
from photo_fx.ops.textart import text_art

_TEXT_ROWS = 3  # fewest rows of characters in a text strip
_TEXT_GAMMA = 0.55  # < 1 puts more ink down, so the text carries enough light to read against the black
_TEXT_CHARS = " .:-=+*#%@"  # the character palette of the text strips (the `ascii` effect's default palette)
_TEXT_BACKGROUND = "#000000"  # the paper behind the text strips' characters


@effect(category="glitch")
def glitch(
    rgb: np.ndarray,
    split: int = 10,
    bands: int = 7,
    shift: float = 0.06,
    ascii_share: float = 0.0,
    ascii_cell: int = 9,
    ascii_photo: float = 0.0,
    seed: int = 0,
) -> np.ndarray:
    """`bands` random horizontal strips slide sideways by up to `shift` of the width, then red and blue are pushed
    `split` px apart. The same `seed` always gives the same damage.

    `ascii_share` (0..1) is the fraction of the strips that also dissolve into text, drawn in the photo's own
    colours with characters `ascii_cell` px wide. Text needs room to be legible, so such a strip is widened to
    hold at least three rows of characters. `ascii_photo` (0..1) keeps that much of the photo visible behind the
    text; at 1 the strips keep all their detail and the characters sit on top. The colour split runs last, so the
    text gets the same red and blue fringes as the rest of the damage. With `ascii_share` 0 there is no text.
    """
    rng = np.random.default_rng(seed)
    height, width = rgb.shape[:2]
    out = rgb.copy()
    strips = []
    for _ in range(bands):
        band_height = max(1, int(rng.uniform(0.01, 0.06) * height))
        top = int(rng.uniform(0, height - band_height))
        dx = int(rng.uniform(-shift, shift) * width)
        out[top : top + band_height] = np.roll(out[top : top + band_height], dx, axis=1)
        strips.append((top, band_height))

    if ascii_share > 0 and strips:
        # A separate random stream, so the sliding above is identical whether or not text is switched on.
        chosen = np.random.default_rng([seed, 1]).choice(
            len(strips), math.ceil(ascii_share * len(strips)), replace=False
        )
        text_height = min(height, _TEXT_ROWS * 2 * ascii_cell)  # a character is about twice as tall as it is wide
        for i in chosen:
            top, band_height = strips[i]
            band_height = max(band_height, text_height)
            top = min(top, height - band_height)
            out[top : top + band_height] = text_art(
                out[top : top + band_height],
                cell=ascii_cell,
                chars=_TEXT_CHARS,
                fg=None,  # each character in the colour of the strip under it
                background=parse_color(_TEXT_BACKGROUND),
                gamma=_TEXT_GAMMA,
                photo=ascii_photo,
                font=None,
            )

    out[..., 0] = np.roll(out[..., 0], split, axis=1)
    out[..., 2] = np.roll(out[..., 2], -split, axis=1)
    return out
