"""Tests of `ops/composite.py`: the blend modes, against the W3C Compositing and Blending Level 1 specification.

The oracle is the specification written out here as plain Python on single pixels (separable modes per channel, the
non-separable `color` mode with the spec's helper functions), so the array code is compared with the spec and not with
itself.
"""

import math
import typing

import numpy as np
import pytest

from bullterrier_image.ops.composite import BlendMode, composite

# --- the specification, one pixel at a time ---------------------------------------------------------------------


def multiply(cb: float, cs: float) -> float:
    return cb * cs


def screen(cb: float, cs: float) -> float:
    return cb + cs - cb * cs


def hard_light(cb: float, cs: float) -> float:
    return multiply(cb, 2 * cs) if cs <= 0.5 else screen(cb, 2 * cs - 1)


def soft_light(cb: float, cs: float) -> float:
    if cs <= 0.5:
        return cb - (1 - 2 * cs) * cb * (1 - cb)
    d = ((16 * cb - 12) * cb + 4) * cb if cb <= 0.25 else math.sqrt(cb)
    return cb + (2 * cs - 1) * (d - cb)


SEPARABLE = {
    "normal": lambda cb, cs: cs,
    "multiply": multiply,
    "screen": screen,
    "overlay": lambda cb, cs: hard_light(cs, cb),  # overlay is hard light with the two swapped
    "soft_light": soft_light,
    "difference": lambda cb, cs: abs(cb - cs),
}


def lum(c):
    return 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]


def clip_color(c):
    luminosity, low, high = lum(c), min(c), max(c)
    if low < 0:
        c = [luminosity + (ci - luminosity) * luminosity / (luminosity - low) for ci in c]
    if high > 1:
        c = [luminosity + (ci - luminosity) * (1 - luminosity) / (high - luminosity) for ci in c]
    return c


def set_lum(c, luminosity):
    d = luminosity - lum(c)
    return clip_color([ci + d for ci in c])


def blend_pixel(mode, backdrop, source):
    if mode == "color":
        return set_lum(list(source), lum(backdrop))
    return [SEPARABLE[mode](cb, cs) for cb, cs in zip(backdrop, source, strict=True)]


def expected(mode: str, backdrop: np.ndarray, source: np.ndarray, alpha: float | np.ndarray) -> np.ndarray:
    """Co = (1 - as) Cb + as B(Cb, Cs), pixel by pixel (opaque backdrop); `alpha` one number or one per pixel."""
    alphas = np.broadcast_to(alpha, backdrop.shape[:2])
    out = np.zeros_like(backdrop, dtype=np.float64)
    for index in np.ndindex(backdrop.shape[:2]):
        cb, cs, a = [float(v) for v in backdrop[index]], [float(v) for v in source[index]], float(alphas[index])
        out[index] = [(1 - a) * b + a * m for b, m in zip(cb, blend_pixel(mode, cb, cs), strict=True)]
    return out


MODES = list(typing.get_args(BlendMode))
COLOURS = {
    "orange": (1.0, 0.5, 0.0),
    "dark teal": (0.0, 0.25, 0.3),
    "light grey": (0.8, 0.8, 0.8),
    "saturated blue": (0.1, 0.2, 0.95),
}


@pytest.mark.parametrize("alpha", [1.0, 0.4], ids=["opaque", "alpha 0.4"])
@pytest.mark.parametrize("colour", COLOURS.values(), ids=COLOURS.keys())
@pytest.mark.parametrize("mode", MODES)
def test_every_mode_follows_the_w3c_formulas_over_a_flat_colour(mode, colour, alpha, photo):
    backdrop = np.ascontiguousarray(photo[::6, ::8])  # 16 x 16, every kind of backdrop tone
    source = np.broadcast_to(np.array(colour, np.float32), backdrop.shape)
    out = composite(backdrop, source, mode, np.float32(alpha))
    np.testing.assert_allclose(out, expected(mode, backdrop, source, alpha), atol=2e-5)


@pytest.mark.parametrize("mode", MODES)
def test_every_mode_follows_the_w3c_formulas_between_two_pictures_with_a_per_pixel_alpha(mode, photo):
    backdrop = np.ascontiguousarray(photo[::6, ::8])
    source = np.ascontiguousarray(photo[::-6, ::-8])
    alpha = np.linspace(0, 1, backdrop.shape[0] * backdrop.shape[1], dtype=np.float32).reshape(backdrop.shape[:2])
    out = composite(backdrop, source, mode, alpha[..., np.newaxis])
    np.testing.assert_allclose(out, expected(mode, backdrop, source, alpha), atol=2e-5)


@pytest.mark.parametrize("mode", MODES)
def test_alpha_0_keeps_the_backdrop_and_the_result_stays_in_range(mode, photo):
    source = np.ascontiguousarray(photo[::-1])
    np.testing.assert_array_equal(composite(photo, source, mode, np.float32(0.0)), photo)
    out = composite(photo, source, mode, np.float32(1.0))
    assert out.dtype == np.float32
    assert out.min() >= 0.0
    assert out.max() <= 1.0
