"""The `blend` effect: lay a second picture over the photo with a blend mode (multiply a texture, screen a light...)."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, Picture, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.place import Anchor, Resample, place

Mode = Literal["normal", "multiply", "screen", "overlay", "soft_light", "difference", "color"]


class BlendParams(EffectParams):
    """The picture, how it is fitted to the photo, and the blend mode."""

    texture: Picture = param(
        "#808080",
        doc="The picture laid over the photo: a path in a recipe file (relative to the file), a colour such as "
        "'#808080' (a flat picture) or, in Python, a Picture or an array. Its own transparency is respected.",
    )
    mode: Mode = param(
        "multiply",
        doc="How the picture combines with the photo, as the W3C Compositing and Blending Level 1 modes: 'normal' "
        "shows the picture, 'multiply' darkens, 'screen' lightens, 'overlay' deepens contrast by the photo's tones, "
        "'soft_light' does so gently by the picture's, 'difference' subtracts, 'color' gives the photo the picture's "
        "hue and saturation and keeps its brightness. Mid grey leaves the photo unchanged under 'overlay' and "
        "'soft_light'.",
    )
    fit: Literal["cover", "contain", "stretch", "tile"] = param(
        "cover",
        doc="How the picture is fitted to the photo: 'cover' fills it keeping the aspect and crops the overhang, "
        "'contain' fits inside it keeping the aspect (the rest of the photo is untouched), 'stretch' fills it "
        "ignoring the aspect, 'tile' repeats the picture, one tile as wide as the photo times `scale`.",
    )
    at: Anchor = param(
        "center",
        doc="Where the fitted picture sits, or which tile sits there: 'top_left', 'top', 'top_right', 'left', "
        "'center', 'right', 'bottom_left', 'bottom' or 'bottom_right'. It decides what 'cover' crops.",
    )
    scale: float = param(
        1.0,
        unit="ratio",
        min=0.05,
        max=2,
        doc="The fitted picture's size as a multiple of the size `fit` gives it: 1 as fitted, 0.5 half as wide and "
        "tall. With 'tile', 0.25 puts four tiles across the photo.",
    )
    margin: float = param(
        0.0,
        unit="fraction",
        min=0,
        max=0.45,
        advanced=True,
        doc="Space kept clear around the fitted picture, as a share of the photo's shorter side, 0 to 0.45. Raising "
        "it shrinks what 'cover', 'contain' and 'stretch' fill and moves the picture inward.",
    )
    resample: Resample = param(
        "lanczos",
        advanced=True,
        doc="The filter that sizes the picture: 'nearest' keeps hard pixels, 'bilinear' is soft, 'bicubic' is "
        "sharper, 'lanczos' is sharpest and the best for shrinking.",
    )


@effect(category="compositing", params=BlendParams)
def blend(rgb: np.ndarray, params: BlendParams) -> np.ndarray:
    """Lay a second picture over the photo with a blend mode, fitted by cover, contain, stretch or tile."""
    layer = place(
        params.texture.pixels,
        rgb.shape[:2],
        fit=params.fit,
        at=params.at,
        scale=params.scale,
        margin=params.margin,
        resample=params.resample,
    )
    source, alpha = layer[..., :3], layer[..., 3:]
    mixed = _MODES[params.mode](rgb, source)
    # W3C Compositing and Blending Level 1, section 5.1 (general formula), for an opaque backdrop
    return np.clip(rgb + alpha * (mixed - rgb), 0.0, 1.0).astype(np.float32)


def _multiply(backdrop: np.ndarray, source: np.ndarray) -> np.ndarray:
    return backdrop * source


def _screen(backdrop: np.ndarray, source: np.ndarray) -> np.ndarray:
    return backdrop + source - backdrop * source


def _overlay(backdrop: np.ndarray, source: np.ndarray) -> np.ndarray:
    """Hard light with the two swapped (W3C Compositing and Blending Level 1, section 10.1.7)."""
    return np.where(backdrop <= 0.5, _multiply(source, 2 * backdrop), _screen(source, 2 * backdrop - 1))


def _soft_light(backdrop: np.ndarray, source: np.ndarray) -> np.ndarray:
    """W3C Compositing and Blending Level 1, section 10.1.8."""
    # constant: the W3C soft light formula's D(x), a cubic below 0.25 and the square root above it
    curve = np.where(backdrop <= 0.25, ((16 * backdrop - 12) * backdrop + 4) * backdrop, np.sqrt(backdrop))
    return np.where(
        source <= 0.5,
        backdrop - (1 - 2 * source) * backdrop * (1 - backdrop),
        backdrop + (2 * source - 1) * (curve - backdrop),
    )


def _luminosity(colour: np.ndarray) -> np.ndarray:
    """Lum(C) of W3C Compositing and Blending Level 1, section 10.2: (h, w, 3) to (h, w, 1)."""
    weights = np.array([0.3, 0.59, 0.11], np.float32)  # constant: W3C Lum(C) weights of red, green and blue
    return (colour @ weights)[..., np.newaxis]


def _color(backdrop: np.ndarray, source: np.ndarray) -> np.ndarray:
    """The source's hue and saturation at the backdrop's luminosity: SetLum(Cs, Lum(Cb)), with ClipColor
    (W3C Compositing and Blending Level 1, sections 10.2 and 10.3.4)."""
    luminosity = _luminosity(backdrop)
    moved = source + (luminosity - _luminosity(source))
    low, high = moved.min(axis=-1, keepdims=True), moved.max(axis=-1, keepdims=True)
    below, above = low < 0, high > 1
    moved = np.where(
        below, luminosity + (moved - luminosity) * luminosity / np.where(below, luminosity - low, 1), moved
    )
    return np.where(
        above, luminosity + (moved - luminosity) * (1 - luminosity) / np.where(above, high - luminosity, 1), moved
    )


_MODES = {
    "normal": lambda backdrop, source: source,
    "multiply": _multiply,
    "screen": _screen,
    "overlay": _overlay,
    "soft_light": _soft_light,
    "difference": lambda backdrop, source: np.abs(backdrop - source),
    "color": _color,
}
