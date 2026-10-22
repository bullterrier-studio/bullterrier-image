"""The `ascii` effect: the picture redrawn as a grid of text characters."""

from typing import Annotated

import numpy as np
from pydantic import AfterValidator

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.textart import check_palette, text_art

# A text-art palette: at least two distinct characters, checked when the value is written.
Palette = Annotated[str, AfterValidator(check_palette)]


class AsciiParams(EffectParams):
    """The settings of `ascii`: the character grid, the palette, the colours and how much photo shows through."""

    cell: int = param(
        10,
        unit="px",
        min=1,
        max=200,
        doc="Width of one character, 1 to 200 px; the height follows from the font. Smaller = finer detail, more "
        "characters.",
    )
    chars: Palette = param(
        " .:-=+*#%@",
        doc="The character palette, in any order, at least two different characters. It is sorted by how much ink "
        "each glyph puts down, so the densest is used for the brightest areas on dark paper and the darkest on light "
        "paper (see `paper`). Try ' .:-=+*#%@', ' .oO0@', '01', or blocks like ' ░▒▓█' (the font must have them).",
    )
    ink: Color | None = param(
        None,
        doc="The ink colour of every character ('#rrggbb', '#rgb' or [r, g, b] 0..1); none (the default) colours each "
        "character from the photo under it, brightened so thin strokes stay readable.",
    )
    paper: Color = param(
        "#000000",
        doc="The paper colour behind the characters. Paper darker than mid grey (Rec. 709 luminance below 0.5) puts "
        "dense characters on bright areas; lighter paper puts them on dark areas.",
    )
    gamma: float = param(
        1.0,
        unit="ratio",
        min=0.1,
        max=10,
        doc="Exponent on the amount of ink each character puts down, 0.1 to 10: 1 = linear, above 1 lighter midtones "
        "(less ink), below 1 more ink.",
    )
    photo: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of the original picture shows through the paper behind the characters, 0 to 1: 0 = text on flat "
        "`paper` (fine detail is lost to the grid), 1 = text laid over the untouched photo.",
    )
    font: str | None = param(
        None,
        doc="Path to a monospaced .ttf/.ttc font; none (the default) uses the first system monospaced font found.",
    )


@effect(category="text", params=AsciiParams)
def ascii(rgb: np.ndarray, params: AsciiParams) -> np.ndarray:
    """Redraw the picture as text: a grid of characters `cell` px wide, each chosen by the brightness under it.

    Each character covers one cell of the grid and is picked from `chars` by how much ink it puts down, so the picture's
    tones survive as denser and lighter characters. A font that cannot be opened raises OSError.
    """
    return text_art(
        rgb,
        cell=params.cell,
        chars=params.chars,
        ink=None if params.ink is None else np.asarray(params.ink, np.float32),
        paper=np.asarray(params.paper, np.float32),
        gamma=params.gamma,
        photo=params.photo,
        font=params.font,
    )
