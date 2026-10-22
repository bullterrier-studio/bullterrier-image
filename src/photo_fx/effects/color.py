"""Effects that change colour and tone but not shapes.

This module is the worked example of an effect, to copy: each effect is a frozen parameter model (an `EffectParams`
subclass whose every field is declared with `param`: a description, a unit, a range and a default) and a function
`(rgb, params) -> rgb` registered with `@effect(category=..., params=Model)`. Every value that shapes the result,
including the options of the library calls an effect makes, is a field; only standard constants are written in, each
with a comment naming its standard.
"""

from typing import Literal

import numpy as np
from scipy import ndimage

from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.ops.color import LUMA, luminance


class FillParams(EffectParams):
    """The colour that replaces the picture."""

    color: Color = param("#000000", doc="The colour that fills the whole picture: '#rrggbb', '#rgb' or [r, g, b] 0..1.")


@effect(category="subject", params=FillParams)
def fill(rgb: np.ndarray, params: FillParams) -> np.ndarray:
    """Replace the whole picture with one colour. Masked to the background, this is 'remove the background'."""
    return np.broadcast_to(np.asarray(params.color, np.float32), rgb.shape).copy()


class GradeParams(EffectParams):
    """Basic tone controls: exposure, then contrast, then saturation."""

    exposure: float = param(
        0.0,
        unit="stops",
        min=-10,
        max=10,
        doc="Brightness in photographic stops: +1 doubles every value, -1 halves it.",
    )
    contrast: float = param(
        1.0,
        unit="ratio",
        min=0,
        max=5,
        doc="How far values are pushed away from the pivot: 1 = unchanged, 2 = twice as far, 0 = flat at the pivot.",
    )
    pivot: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The brightness (0..1) that contrast leaves where it is; values above it brighten, below it darken.",
    )
    saturation: float = param(
        1.0,
        unit="ratio",
        min=0,
        max=5,
        doc="Colourfulness: 1 = unchanged, 0 = grey (Rec. 709 brightness kept), above 1 more vivid.",
    )


@effect(category="colour", params=GradeParams)
def grade(rgb: np.ndarray, params: GradeParams) -> np.ndarray:
    """Basic tone controls: `exposure` in stops, then `contrast` around `pivot`, then `saturation`.

    The result is clipped to 0..1 after the contrast (before measuring brightness for the saturation) and at the end.
    """
    out = rgb * np.float32(2.0**params.exposure)  # a stop is a factor of 2 (photographic definition)
    out = params.pivot + (out - params.pivot) * np.float32(params.contrast)
    grey = luminance(np.clip(out, 0, 1))[..., None]
    out = grey + (out - grey) * np.float32(params.saturation)
    return np.clip(out, 0.0, 1.0).astype(np.float32)


class ClarityParams(EffectParams):
    """Local contrast against a Gaussian-blurred copy of the picture (an unsharp mask with a large radius)."""

    radius: float = param(
        30.0,
        unit="px",
        min=0,
        max=500,
        doc="Size of each area's 'surroundings': the Gaussian sigma of the blur, in px at the recipe's size. Larger "
        "compares bigger regions and takes longer (at the top of the range, with a large truncate, seconds per "
        "picture); 0 turns the effect off.",
    )
    amount: float = param(
        1.0,
        unit="ratio",
        min=0,
        max=5,
        doc="How strongly each area's difference from its surroundings is exaggerated: 0 = off, 1 = doubled.",
    )
    truncate: float = param(
        4.0,
        unit="ratio",
        min=1,
        max=10,
        doc="Where the Gaussian is cut off, in sigmas: the blur reaches truncate x radius px. Larger is more exact "
        "and slower.",
    )
    mode: Literal["reflect", "constant", "nearest", "mirror", "wrap"] = param(
        "reflect",
        doc="What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' "
        "(mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge), "
        "'constant' (constant_value).",
    )
    constant_value: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The brightness (0..1) assumed beyond the edge when mode is 'constant'; ignored otherwise.",
    )


@effect(category="light", params=ClarityParams)
def clarity(rgb: np.ndarray, params: ClarityParams) -> np.ndarray:
    """Local contrast: exaggerate how each area differs from its surroundings, so features stand out even when
    the whole face sits in a narrow band of brightness. `radius` = size of 'surroundings' in px, `amount` 0 = off.
    Put it before effects that turn tone into marks (ascii, halftone), which are blind to small tonal differences.
    """
    blurred = np.stack(
        [
            ndimage.gaussian_filter(
                rgb[..., c], params.radius, mode=params.mode, cval=params.constant_value, truncate=params.truncate
            )
            for c in range(3)
        ],
        axis=-1,
    )
    return np.clip(rgb + params.amount * (rgb - blurred), 0.0, 1.0).astype(np.float32)


# Only keeps a flat picture (every pixel equally bright) from dividing by zero: such a picture maps to the first stop.
_FLAT = 1e-6


class GradientMapParams(EffectParams):
    """A gradient that brightness is mapped onto, after an automatic contrast stretch."""

    stops: tuple[Color, ...] = param(
        ("#000000", "#ffffff"),
        doc="The gradient's colours from darkest to brightest, evenly spaced; at least two. Two make a duotone, three "
        "a tritone.",
        min_length=2,
    )
    shadow_clip: float = param(
        0.01,
        unit="fraction",
        min=0,
        max=0.5,
        doc="Share of the darkest pixels (0..0.5) that the contrast stretch pushes onto the first stop; 0 maps only "
        "the darkest pixel there. Larger gives deeper shadows.",
    )
    highlight_clip: float = param(
        0.01,
        unit="fraction",
        min=0,
        max=0.5,
        doc="Share of the brightest pixels (0..0.5) that the contrast stretch pushes onto the last stop. Larger gives "
        "brighter highlights.",
    )
    quantile_method: Literal[
        "linear",
        "lower",
        "higher",
        "nearest",
        "midpoint",
        "inverted_cdf",
        "averaged_inverted_cdf",
        "closest_observation",
        "interpolated_inverted_cdf",
        "hazen",
        "weibull",
        "median_unbiased",
        "normal_unbiased",
    ] = param(
        "linear",
        doc="How the stretch's clip points are read from the picture's brightness (numpy's quantile methods): 'linear' "
        "interpolates between the two nearest pixels; 'lower', 'higher', 'nearest' and 'midpoint' take the darker, "
        "the brighter, the nearer pixel or their mean; 'inverted_cdf', 'averaged_inverted_cdf', "
        "'closest_observation', 'interpolated_inverted_cdf', 'hazen', 'weibull', 'median_unbiased' and "
        "'normal_unbiased' are the textbook sample-quantile definitions (Hyndman and Fan). Only visible on small or "
        "flat pictures.",
    )


@effect(category="colour", params=GradientMapParams)
def gradient_map(rgb: np.ndarray, params: GradientMapParams) -> np.ndarray:
    """Recolour by brightness: darkest pixels get the first stop, brightest the last, evenly spaced between.

    Two stops make a duotone, three a tritone; e.g. stops = ["#0b0020", "#ff2bd6", "#00f0ff"] is a neon scheme.
    The picture is first stretched to use its full tonal range (cutting `shadow_clip` and `highlight_clip` off its
    ends), so the whole gradient is always visible.
    """
    colors = np.array(params.stops, dtype=np.float32)
    count = len(params.stops)
    lum = luminance(rgb)
    lo, hi = np.quantile(lum, [params.shadow_clip, 1.0 - params.highlight_clip], method=params.quantile_method)
    t = np.clip((lum - lo) / max(hi - lo, _FLAT), 0.0, 1.0) * (count - 1)
    i = np.minimum(t.astype(int), count - 2)
    f = (t - i)[..., None]
    return (colors[i] * (1 - f) + colors[i + 1] * f).astype(np.float32)


class SplitToneParams(EffectParams):
    """Two tints, one for the shadows and one for the highlights, that leave brightness alone."""

    shadow: Color = param("#3a1bff", doc="The tint of the dark parts: '#rrggbb', '#rgb' or [r, g, b] 0..1.")
    highlight: Color = param("#ff2bd6", doc="The tint of the bright parts: '#rrggbb', '#rgb' or [r, g, b] 0..1.")
    amount: float = param(
        0.3, unit="fraction", min=0, max=1, doc="How strongly the tints are added: 0 = off, 0.2 a hint, 0.6 strong."
    )
    balance: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The brightness (0..1) where the shadow tint hands over to the highlight tint. Raising it tints more of "
        "the picture with the shadow colour.",
    )
    softness: float = param(
        0.5,
        unit="fraction",
        min=0.01,
        max=1,
        doc="Width of the brightness band (0.01..1), centred on balance, over which one tint blends into the other. "
        "Smaller makes a sharper hand-over.",
    )
    black_fade: float = param(
        0.1,
        unit="fraction",
        min=0.001,
        max=1,
        doc="The brightness (0.001..1) up to which the tints fade in from nothing, so pure black stays black. "
        "Larger keeps more of the shadows untinted.",
    )


@effect(category="colour", params=SplitToneParams)
def split_tone(rgb: np.ndarray, params: SplitToneParams) -> np.ndarray:
    """Tint the dark parts with `shadow` and the bright parts with `highlight`, without changing brightness.

    The subtle way to get a neon or cinematic cast: skin tones and detail stay, the colour shifts toward the tints.
    `amount` 0..1 (0.2 is a hint, 0.6 is strong), `balance` = the brightness where one tint hands over to the other.
    Pure black stays black (a tint fades in up to `black_fade` brightness), so a black background is not coloured.
    Near pure white a tint would clip, so those few pixels shift slightly in brightness.
    """
    lum = luminance(rgb)
    t = np.clip(0.5 + (lum - params.balance) / params.softness, 0.0, 1.0)[..., None]  # 0.5: halfway, at balance
    tint = np.asarray(params.shadow, np.float32) * (1.0 - t) + np.asarray(params.highlight, np.float32) * t
    offset = tint - luminance(tint)[..., None]  # the tint's colour with its brightness taken out
    fade_in = np.clip(lum / params.black_fade, 0.0, 1.0)[..., None]  # nothing is added to pure black
    return np.clip(rgb + params.amount * offset * fade_in, 0.0, 1.0).astype(np.float32)


# Rec. 709 luma/chroma basis built from the same weights as `luminance`: rows give Y, Cb, Cr from R, G, B.
_KR, _KG, _KB = LUMA
_LUMA_CHROMA = np.array(
    [
        [_KR, _KG, _KB],
        [-_KR / (2 * (1 - _KB)), -_KG / (2 * (1 - _KB)), 0.5],
        [0.5, -_KG / (2 * (1 - _KR)), -_KB / (2 * (1 - _KR))],
    ]
)
_LUMA_CHROMA_INVERSE = np.linalg.inv(_LUMA_CHROMA)


class HueShiftParams(EffectParams):
    """How far every hue turns around the colour wheel."""

    hue: float = param(
        90.0,
        unit="degrees",
        min=-360,
        max=360,
        doc="The turn around the colour wheel: 120 sends red to green, 360 is a full turn (unchanged).",
    )


@effect(category="colour", params=HueShiftParams)
def hue_shift(rgb: np.ndarray, params: HueShiftParams) -> np.ndarray:
    """Rotate every hue around the colour wheel by `hue` degrees: the quickest way to try another colour scheme.

    Brightness is preserved (as `luminance` measures it), so the picture keeps its lighting while colours change,
    except for very saturated colours whose rotated version cannot be displayed and gets clipped.
    """
    c, s = np.cos(np.radians(params.hue)), np.sin(np.radians(params.hue))
    rotate_chroma = np.array([[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]])  # hue = angle in the (Cb, Cr) plane
    matrix = (_LUMA_CHROMA_INVERSE @ rotate_chroma @ _LUMA_CHROMA).astype(np.float32)
    return np.clip(rgb @ matrix.T, 0.0, 1.0).astype(np.float32)
