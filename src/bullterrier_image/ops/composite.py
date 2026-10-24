"""Blend modes: how a picture laid over another combines with it, as the W3C Compositing and Blending Level 1 defines.

`composite` is the one place a blend mode is computed: a recipe step's `blend_mode` (the runner) and the `blend` effect
(a placed picture) both go through it, so a mode means the same in both. `BlendMode` is the one list of their names.
"""

from typing import Literal

import numpy as np

BlendMode = Literal["normal", "multiply", "screen", "overlay", "soft_light", "difference", "color"]


def composite(backdrop: np.ndarray, source: np.ndarray, mode: BlendMode, alpha: np.floating | np.ndarray) -> np.ndarray:
    """`source` laid over `backdrop` with the blend mode `mode`, then mixed in by `alpha`, as a new float32 picture in
    0..1: Co = Cb + alpha (B(Cb, Cs) - Cb), the general formula of section 5.1 for an opaque backdrop.

    backdrop, source: float32 (h, w, 3) in 0..1. alpha: a float32 number or an (h, w, 1) array, 0..1: 0 keeps the
    backdrop, 1 shows the blend fully. `normal` shows the source, `multiply` darkens, `screen` lightens, `overlay`
    and `soft_light` deepen contrast (by the backdrop's tones, and gently by the source's), `difference` subtracts,
    and `color` gives the backdrop the source's hue and saturation at its own luminosity.
    """
    blended = _MODES[mode](backdrop, source)
    out = backdrop + alpha * (blended - backdrop)
    return np.clip(out, 0.0, 1.0, out=out)


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
