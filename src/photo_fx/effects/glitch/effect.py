"""The `glitch` effect: colour channels pulled apart, horizontal bands knocked sideways, some of them turned to text."""

import math
from typing import Annotated, Self

import numpy as np
from pydantic import AfterValidator, model_validator

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.textart import check_palette, text_art

# A text-art palette: at least two distinct characters, checked when the value is written.
Palette = Annotated[str, AfterValidator(check_palette)]


class GlitchParams(EffectParams):
    """The settings of `glitch`: the sliding bands, the red/blue split, and the strips that turn to text."""

    split: int = param(
        10,
        unit="px",
        min=-500,
        max=500,
        doc="How far red is pushed right and blue left, -500 to 500 px (negative swaps the directions); 0 = no "
        "colour split. The picture wraps around at the edges.",
    )
    bands: int = param(
        7,
        unit="count",
        min=0,
        max=500,
        doc="How many random horizontal strips slide sideways, 0 to 500; 0 = none.",
    )
    shift: float = param(
        0.06,
        unit="fraction",
        min=0,
        max=1,
        doc="The furthest a strip slides, as a fraction 0 to 1 of the picture's width; each strip slides a random "
        "distance up to this, left or right, wrapping around.",
    )
    ascii_share: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The fraction 0 to 1 of the strips that also dissolve into text (rounded up to whole strips); 0 = no text.",
    )
    ascii_cell: int = param(
        9,
        unit="px",
        min=1,
        max=200,
        doc="Width of one character of the text strips, 1 to 200 px. Larger = bigger, fewer characters.",
    )
    ascii_photo: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of the strip shows through behind its characters, 0 to 1: 0 = text on flat `ascii_paper`, "
        "1 = text laid over the strip, keeping all its detail.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed: the same seed always gives the same damage; any other gives different strips.",
    )
    band_min_height: float = param(
        0.01,
        unit="fraction",
        min=0,
        max=1,
        doc="The lowest a strip can be, as a fraction 0 to 1 of the picture's height (never below 1 px). Each strip's "
        "height is drawn at random between this and `band_max_height`, which it may not exceed.",
    )
    band_max_height: float = param(
        0.06,
        unit="fraction",
        min=0,
        max=1,
        doc="The tallest a strip can be, as a fraction 0 to 1 of the picture's height. Raising it gives taller strips.",
    )
    ascii_rows: int = param(
        3,
        unit="count",
        min=1,
        max=100,
        doc="The fewest rows of characters a text strip holds, 1 to 100: a thinner strip is widened (each row counted "
        "as twice `ascii_cell` tall) so its text stays legible, up to the whole picture.",
    )
    ascii_gamma: float = param(
        0.55,
        unit="ratio",
        min=0.1,
        max=10,
        doc="Exponent on the amount of ink each character puts down, 0.1 to 10: below 1 (the default) more ink, so the "
        "text carries enough light to read against dark paper; above 1 less.",
    )
    ascii_chars: Palette = param(
        " .:-=+*#%@",
        doc="The character palette of the text strips, in any order, at least two different characters; sorted by how "
        "much ink each glyph puts down, so the densest is used for the brightest areas on dark paper and the darkest "
        "on light paper (see `ascii_paper`).",
    )
    ascii_ink: Color | None = param(
        None,
        doc="The ink colour of the text strips ('#rrggbb', '#rgb' or [r, g, b] 0..1); none (the default) colours each "
        "character from the average colour of the strip under it, brightened as in the ascii effect (strongest "
        "channel scaled to full, by at most 1 / `ascii_ink_floor`).",
    )
    ascii_ink_floor: float = param(
        0.35,
        unit="fraction",
        min=0.01,
        max=1,
        doc="With no `ascii_ink`: the most a character's colour is brightened, as a floor 0.01 to 1 on its strongest "
        "channel: a colour whose strongest channel is below it is scaled by 1 / ascii_ink_floor instead of up to "
        "full, so dark strips stay dark. Lowering it brightens dark colours more; 1 = colours as they are.",
    )
    ascii_paper: Color = param(
        "#000000",
        doc="The paper colour behind the text strips' characters. Dark paper (Rec. 709 luminance below "
        "`ascii_paper_threshold`) puts dense characters on bright areas; lighter paper puts them on dark areas.",
    )
    ascii_paper_threshold: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The Rec. 709 luminance, 0 to 1, below which `ascii_paper` counts as dark (dense characters on bright "
        "areas) rather than light (dense characters on dark areas); 0.5 = mid grey. Raising it lets lighter paper "
        "count as dark: 0 treats every paper as light, 1 every paper but white as dark.",
    )
    ascii_font: str | None = param(
        None,
        doc="Path to a monospaced .ttf/.ttc font for the text strips; none (the default) uses the first system "
        "monospaced font found.",
    )

    @model_validator(mode="after")
    def _band_heights_in_order(self) -> Self:
        """The band height limits are a range: refused, naming both, when the lowest is above the tallest."""
        if self.band_min_height > self.band_max_height:
            raise ValueError(
                f"`band_min_height` ({self.band_min_height}) must not exceed `band_max_height` "
                f"({self.band_max_height}): they are the lowest and the tallest a strip can be"
            )
        return self


@effect(category="glitch", params=GlitchParams)
def glitch(rgb: np.ndarray, params: GlitchParams) -> np.ndarray:
    """`bands` random horizontal strips slide sideways by up to `shift` of the width, then red and blue are pushed
    `split` px apart.

    The same `seed` always gives the same damage. `ascii_share` of the strips also dissolve into text drawn with the
    `ascii_*` settings; the text has its own random stream, so the sliding is identical whether or not text is on. The
    colour split runs last, so the text gets the same red and blue fringes as the rest of the damage. A text font that
    cannot be opened raises OSError.
    """
    rng = np.random.default_rng(params.seed)
    height, width = rgb.shape[:2]
    out = rgb.copy()
    strips = []
    for _ in range(params.bands):
        band_height = max(1, int(rng.uniform(params.band_min_height, params.band_max_height) * height))
        top = int(rng.uniform(0, height - band_height))
        dx = int(rng.uniform(-params.shift, params.shift) * width)
        out[top : top + band_height] = np.roll(out[top : top + band_height], dx, axis=1)
        strips.append((top, band_height))

    if params.ascii_share > 0 and strips:
        # constant: the id of a second random stream, independent of the band offsets drawn above
        chosen = np.random.default_rng([params.seed, 1]).choice(
            len(strips), math.ceil(params.ascii_share * len(strips)), replace=False
        )
        # sized before any font is measured, so the strip height is an estimate
        row_height = 2 * params.ascii_cell  # constant: a monospaced character is about twice as tall as wide
        text_height = min(height, params.ascii_rows * row_height)
        for i in chosen:
            top, band_height = strips[i]
            band_height = max(band_height, text_height)
            top = min(top, height - band_height)
            out[top : top + band_height] = text_art(
                out[top : top + band_height],
                cell=params.ascii_cell,
                chars=params.ascii_chars,
                ink=None if params.ascii_ink is None else np.asarray(params.ascii_ink, np.float32),
                ink_floor=params.ascii_ink_floor,
                paper=np.asarray(params.ascii_paper, np.float32),
                paper_threshold=params.ascii_paper_threshold,
                gamma=params.ascii_gamma,
                photo=params.ascii_photo,
                font=params.ascii_font,
            )

    out[..., 0] = np.roll(out[..., 0], params.split, axis=1)
    out[..., 2] = np.roll(out[..., 2], -params.split, axis=1)
    return out
