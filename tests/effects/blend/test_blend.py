"""Behaviour of the `blend` effect: a picture fitted to the photo and combined with it by its blend mode and alpha.

The blend modes themselves are tested against the W3C specification in `tests/ops/test_composite.py`; here the
expectations are worked out by hand: a neutral picture per mode, a multiply, a half-transparent picture.
"""

import numpy as np
import pytest

from bullterrier_image import Picture
from bullterrier_image.core import registry

NAME = "blend"


def test_multiply_by_a_flat_colour_multiplies_every_channel_by_it(photo):
    out = registry.apply(NAME, photo, image=Picture.of_color((0.5, 1.0, 0.25)), mode="multiply")
    np.testing.assert_allclose(out, photo * np.array([0.5, 1.0, 0.25], np.float32), atol=1e-6)


def test_the_pictures_own_alpha_mixes_the_blend_in_partway(photo):
    colour = (0.9, 0.3, 0.1)
    picture = Picture(np.array([[(*colour, 0.4)]], np.float32))
    out = registry.apply(NAME, photo, image=picture, mode="normal")
    np.testing.assert_allclose(out, 0.6 * photo + 0.4 * np.array(colour, np.float32), atol=1e-6)


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
    np.testing.assert_allclose(registry.apply(NAME, photo, image=picture, mode=mode), photo, atol=1e-6)


# --- fitting the picture to the photo -----------------------------------------------------------------------------


def two_by_two() -> np.ndarray:
    return np.array([[[1, 0, 0, 1], [0, 0, 1, 1]], [[0, 0, 1, 1], [1, 0, 0, 1]]], np.float32)


def test_stretch_covers_the_whole_photo_with_the_picture_enlarged_to_its_size(photo):
    out = registry.apply(NAME, photo, image=Picture(two_by_two()), mode="normal", fit="stretch", resample="nearest")
    expected = np.repeat(np.repeat(two_by_two()[..., :3], 48, axis=0), 64, axis=1)  # the photo is 96 x 128
    np.testing.assert_array_equal(out, expected)


def test_contain_leaves_the_photo_untouched_beside_the_picture(photo):
    out = registry.apply(
        NAME, photo, image=Picture(two_by_two()), mode="normal", fit="contain", at="left", resample="nearest"
    )
    np.testing.assert_array_equal(out[:, 96:], photo[:, 96:])  # a 96 x 96 square at the left of 96 x 128
    np.testing.assert_array_equal(out[:48, :48, 0], np.ones((48, 48), np.float32))
    np.testing.assert_array_equal(out[:48, 48:96, 0], np.zeros((48, 48), np.float32))


def test_tile_repeats_the_picture_scale_times_the_photos_width(photo):
    out = registry.apply(
        NAME,
        photo,
        image=Picture(two_by_two()),
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
    out = registry.apply(NAME, photo, image=red, mode="normal", fit="stretch", margin=0.1)  # 10 of 96 px
    np.testing.assert_array_equal(out[:10], photo[:10])
    np.testing.assert_allclose(out[10:86, 10:118], np.broadcast_to([1, 0, 0], (76, 108, 3)), atol=1e-6)


def test_a_transparent_picture_is_resized_without_bleeding_its_hidden_colour(photo):
    half = np.array([[[1, 0, 0, 1], [0, 1, 0, 0]]], np.float32)  # opaque red, then clear with green hidden in it
    out = registry.apply(NAME, photo, image=Picture(half), mode="normal", fit="stretch", resample="bicubic")
    assert (out[..., 1] <= photo[..., 1] + 1e-6).all()  # the hidden green is never mixed in (it would lift green)
