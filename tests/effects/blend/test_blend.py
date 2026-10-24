"""Behaviour of the `blend` effect.

The oracle for the blend modes is the W3C Compositing and Blending Level 1 specification, written out here as plain
Python on single pixels (separable modes per channel, the non-separable `color` mode with the spec's helper functions),
so the effect's array code is compared with the spec and not with itself.
"""

import math

import numpy as np
import pytest

from bullterrier_image import Picture
from bullterrier_image.core import registry

NAME = "blend"


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


def expected_blend(mode: str, rgb: np.ndarray, colour: tuple[float, float, float], alpha: float = 1.0) -> np.ndarray:
    """The photo blended with one colour (opaque backdrop): Co = (1 - as) Cb + as B(Cb, Cs)."""
    out = np.zeros_like(rgb, dtype=np.float64)
    for index in np.ndindex(rgb.shape[:2]):
        backdrop = [float(v) for v in rgb[index]]
        mixed = blend_pixel(mode, backdrop, colour)
        out[index] = [(1 - alpha) * cb + alpha * bm for cb, bm in zip(backdrop, mixed, strict=True)]
    return out


MODES = [*SEPARABLE, "color"]
COLOURS = {
    "orange": (1.0, 0.5, 0.0),
    "dark teal": (0.0, 0.25, 0.3),
    "light grey": (0.8, 0.8, 0.8),
    "saturated blue": (0.1, 0.2, 0.95),
}


@pytest.mark.parametrize("colour", COLOURS.values(), ids=COLOURS.keys())
@pytest.mark.parametrize("mode", MODES)
def test_a_one_colour_picture_blends_like_that_colour_by_the_w3c_formulas(mode, colour, photo):
    rgb = np.ascontiguousarray(photo[::6, ::8])  # 16 x 16, every kind of backdrop tone
    picture = Picture(np.array([[colour]], np.float32))
    out = registry.apply(NAME, rgb, texture=picture, mode=mode)
    np.testing.assert_allclose(out, expected_blend(mode, rgb, colour), atol=2e-5)


@pytest.mark.parametrize("mode", MODES)
def test_the_pictures_own_alpha_mixes_the_blend_in_partway(mode, photo):
    rgb = np.ascontiguousarray(photo[::6, ::8])
    colour = (0.9, 0.3, 0.1)
    picture = Picture(np.array([[(*colour, 0.4)]], np.float32))
    out = registry.apply(NAME, rgb, texture=picture, mode=mode)
    np.testing.assert_allclose(out, expected_blend(mode, rgb, colour, alpha=0.4), atol=2e-5)


IDENTITIES = {
    "multiply by white": ("multiply", (1.0, 1.0, 1.0, 1.0)),
    "screen by black": ("screen", (0.0, 0.0, 0.0, 1.0)),
    "difference with black": ("difference", (0.0, 0.0, 0.0, 1.0)),
    "overlay with mid grey": ("overlay", (0.5, 0.5, 0.5, 1.0)),
    "soft light with mid grey": ("soft_light", (0.5, 0.5, 0.5, 1.0)),
    "normal with alpha 0": ("normal", (0.2, 0.7, 0.4, 0.0)),
    "color with alpha 0": ("color", (0.2, 0.7, 0.4, 0.0)),
}


@pytest.mark.parametrize(("mode", "pixel"), IDENTITIES.values(), ids=IDENTITIES.keys())
def test_the_neutral_picture_of_each_mode_leaves_the_photo_as_it_was(mode, pixel, photo):
    picture = Picture(np.array([[pixel]], np.float32))
    np.testing.assert_allclose(registry.apply(NAME, photo, texture=picture, mode=mode), photo, atol=1e-6)


# --- fitting the picture to the photo -----------------------------------------------------------------------------


def two_by_two() -> np.ndarray:
    return np.array([[[1, 0, 0, 1], [0, 0, 1, 1]], [[0, 0, 1, 1], [1, 0, 0, 1]]], np.float32)


def test_stretch_covers_the_whole_photo_with_the_picture_enlarged_to_its_size(photo):
    out = registry.apply(NAME, photo, texture=Picture(two_by_two()), mode="normal", fit="stretch", resample="nearest")
    expected = np.repeat(np.repeat(two_by_two()[..., :3], 48, axis=0), 64, axis=1)  # the photo is 96 x 128
    np.testing.assert_array_equal(out, expected)


def test_contain_leaves_the_photo_untouched_beside_the_picture(photo):
    out = registry.apply(
        NAME, photo, texture=Picture(two_by_two()), mode="normal", fit="contain", at="left", resample="nearest"
    )
    np.testing.assert_array_equal(out[:, 96:], photo[:, 96:])  # a 96 x 96 square at the left of 96 x 128
    np.testing.assert_array_equal(out[:48, :48, 0], np.ones((48, 48), np.float32))
    np.testing.assert_array_equal(out[:48, 48:96, 0], np.zeros((48, 48), np.float32))


def test_tile_repeats_the_picture_scale_times_the_photos_width(photo):
    out = registry.apply(
        NAME,
        photo,
        texture=Picture(two_by_two()),
        mode="normal",
        fit="tile",
        scale=0.25,
        at="top_left",
        resample="nearest",
    )  # tiles 32 px wide and tall, four across, three down
    expected = np.tile(np.repeat(np.repeat(two_by_two()[..., :3], 16, axis=0), 16, axis=1), (3, 4, 1))
    np.testing.assert_array_equal(out, expected)


def test_the_margin_keeps_the_edge_of_the_photo_clear(photo):
    red = Picture.of_color("#ff0000")
    out = registry.apply(NAME, photo, texture=red, mode="normal", fit="stretch", margin=0.1)  # 10 of 96 px
    np.testing.assert_array_equal(out[:10], photo[:10])
    np.testing.assert_allclose(out[10:86, 10:118], np.broadcast_to([1, 0, 0], (76, 108, 3)), atol=1e-6)


def test_a_transparent_picture_is_resized_without_bleeding_its_hidden_colour(photo):
    half = np.array([[[1, 0, 0, 1], [0, 1, 0, 0]]], np.float32)  # opaque red, then clear with green hidden in it
    out = registry.apply(NAME, photo, texture=Picture(half), mode="normal", fit="stretch", resample="bicubic")
    assert (out[..., 1] <= photo[..., 1] + 1e-6).all()  # the hidden green is never mixed in (it would lift green)
