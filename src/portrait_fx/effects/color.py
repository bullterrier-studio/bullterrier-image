"""Effects that change colour and tone but not shapes."""

import numpy as np
from scipy import ndimage

from ._common import LUMA, Color, luminance, parse_color
from ._registry import effect


@effect
def fill(rgb: np.ndarray, color: Color = "#000000") -> np.ndarray:
    """Replace the whole picture with one colour. Masked to the background, this is 'remove the background'."""
    return np.broadcast_to(parse_color(color), rgb.shape).copy()


@effect
def grade(rgb: np.ndarray, exposure: float = 0.0, contrast: float = 1.0, saturation: float = 1.0) -> np.ndarray:
    """Basic tone controls. `exposure` in stops (+1 = twice as bright), `contrast` and `saturation` 1.0 = unchanged."""
    out = rgb * np.float32(2.0**exposure)
    out = 0.5 + (out - 0.5) * np.float32(contrast)
    grey = luminance(np.clip(out, 0, 1))[..., None]
    out = grey + (out - grey) * np.float32(saturation)
    return np.clip(out, 0.0, 1.0).astype(np.float32)


@effect
def clarity(rgb: np.ndarray, radius: float = 30.0, amount: float = 1.0) -> np.ndarray:
    """Local contrast: exaggerate how each area differs from its surroundings, so features stand out even when
    the whole face sits in a narrow band of brightness. `radius` = size of 'surroundings' in px, `amount` 0 = off.
    Put it before effects that turn tone into marks (ascii, halftone), which are blind to small tonal differences.
    """
    blurred = np.stack([ndimage.gaussian_filter(rgb[..., c], radius) for c in range(3)], axis=-1)
    return np.clip(rgb + amount * (rgb - blurred), 0.0, 1.0).astype(np.float32)


@effect
def gradient_map(rgb: np.ndarray, stops: list[Color] = ("#000000", "#ffffff")) -> np.ndarray:
    """Recolour by brightness: darkest pixels get the first stop, brightest the last, evenly spaced between.

    Two stops make a duotone, three a tritone; e.g. stops = ["#0b0020", "#ff2bd6", "#00f0ff"] is a neon scheme.
    The picture is first stretched to use its full tonal range, so the whole gradient is always visible.
    """
    if len(stops) < 2:
        raise ValueError("gradient_map needs at least two stops")
    colors = np.stack([parse_color(c) for c in stops])
    lum = luminance(rgb)
    lo, hi = np.percentile(lum, [1, 99])
    t = np.clip((lum - lo) / max(hi - lo, 1e-6), 0.0, 1.0) * (len(stops) - 1)
    i = np.minimum(t.astype(int), len(stops) - 2)
    f = (t - i)[..., None]
    return (colors[i] * (1 - f) + colors[i + 1] * f).astype(np.float32)


@effect
def split_tone(
    rgb: np.ndarray,
    shadow: Color = "#3a1bff",
    highlight: Color = "#ff2bd6",
    amount: float = 0.3,
    balance: float = 0.5,
) -> np.ndarray:
    """Tint the dark parts with `shadow` and the bright parts with `highlight`, without changing brightness.

    The subtle way to get a neon or cinematic cast: skin tones and detail stay, the colour shifts toward the tints.
    `amount` 0..1 (0.2 is a hint, 0.6 is strong), `balance` = the brightness where one tint hands over to the other.
    Pure black stays black (a tint fades in over the darkest 10% of brightness), so a black background is not coloured.
    Near pure white a tint would clip, so those few pixels shift slightly in brightness.
    """
    lum = luminance(rgb)
    t = np.clip(0.5 + (lum - balance) * 2.0, 0.0, 1.0)[..., None]
    tint = parse_color(shadow) * (1.0 - t) + parse_color(highlight) * t
    offset = tint - luminance(tint)[..., None]  # the tint's colour with its brightness taken out
    fade_in = np.clip(lum / 0.1, 0.0, 1.0)[..., None]  # nothing is added to pure black
    return np.clip(rgb + amount * offset * fade_in, 0.0, 1.0).astype(np.float32)


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


@effect
def hue_shift(rgb: np.ndarray, degrees: float = 90.0) -> np.ndarray:
    """Rotate every hue around the colour wheel by `degrees`: the quickest way to try another colour scheme.

    Brightness is preserved (as `luminance` measures it), so the picture keeps its lighting while colours change,
    except for very saturated colours whose rotated version cannot be displayed and gets clipped.
    """
    c, s = np.cos(np.radians(degrees)), np.sin(np.radians(degrees))
    rotate_chroma = np.array([[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]])  # hue = angle in the (Cb, Cr) plane
    matrix = (_LUMA_CHROMA_INVERSE @ rotate_chroma @ _LUMA_CHROMA).astype(np.float32)
    return np.clip(rgb @ matrix.T, 0.0, 1.0).astype(np.float32)
