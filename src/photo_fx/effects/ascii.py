"""ASCII art: the picture redrawn as a grid of text characters."""

import numpy as np

from photo_fx.core.registry import effect
from photo_fx.core.types import Color
from photo_fx.ops.color import parse_color
from photo_fx.ops.textart import text_art


@effect(category="text")
def ascii(
    rgb: np.ndarray,
    cell: int = 10,
    chars: str = " .:-=+*#%@",
    fg: Color | None = None,
    bg: Color = "#000000",
    gamma: float = 1.0,
    photo: float = 0.0,
    font: str | None = None,
) -> np.ndarray:
    """Redraw the picture as text. `cell` is the character width in px (smaller = finer detail).

    `chars` is the palette in any order; it is sorted by how much ink each glyph actually puts on the page, so
    '@' is used for the brightest areas on a dark `bg` and the darkest on a light one. Try " .:-=+*#%@", " .oO0@",
    "01", or block characters like " ░▒▓█" (the font must have them). `fg` = one colour for all text; leave it
    out to colour each character from the photo. `gamma` > 1 darkens the midtones.
    `photo` (0..1) is how much of the original picture shows through behind the characters: 0 = text on a flat `bg`
    (fine detail is lost to the character grid), 1 = text laid over the untouched photo, so every detail stays visible.
    `font` = path to a monospaced .ttf/.ttc if you do not want the system one.
    """
    return text_art(
        rgb,
        cell=cell,
        chars=chars,
        fg=None if fg is None else parse_color(fg),
        background=parse_color(bg),
        gamma=gamma,
        photo=photo,
        font=font,
    )
