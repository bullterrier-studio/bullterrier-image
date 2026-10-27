"""Behaviour of the `blend` effect: a picture fitted to the photo and combined with it by its blend mode and alpha.

The blend modes themselves are tested against the W3C specification in `tests/ops/test_composite.py`; here the
expectations are worked out by hand: a neutral picture per mode, a multiply, a half-transparent picture.
"""

import numpy as np
import pytest

from bullterrier_image import Picture, Recipe, fx, render
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
    out = registry.apply(NAME, photo, image=red, mode="normal", fit="stretch", margin=16)
    np.testing.assert_array_equal(out[:16], photo[:16])
    np.testing.assert_allclose(out[16:80, 16:112], np.broadcast_to([1, 0, 0], (64, 96, 3)), atol=1e-6)


def test_a_transparent_picture_is_resized_without_bleeding_its_hidden_colour(photo):
    half = np.array([[[1, 0, 0, 1], [0, 1, 0, 0]]], np.float32)  # opaque red, then clear with green hidden in it
    out = registry.apply(NAME, photo, image=Picture(half), mode="normal", fit="stretch", resample="bicubic")
    assert (out[..., 1] <= photo[..., 1] + 1e-6).all()  # the hidden green is never mixed in (it would lift green)


# --- a logo: fit "width" sizes the picture by the photo's width ----------------------------------------------------


def logo_pixels() -> np.ndarray:
    """An 8 x 16 (height x width) logo: opaque red on the left half, opaque blue on the right half."""
    pixels = np.ones((8, 16, 4), np.float32)
    pixels[:, :8, :3] = (1, 0, 0)
    pixels[:, 8:, :3] = (0, 0, 1)
    return pixels


def put_logo(photo, logo=None, **params):
    """The logo laid on the photo with the normal mode, fitted by width, hard-edged, as `blend` takes it."""
    picture = Picture(logo_pixels() if logo is None else logo)
    return registry.apply(NAME, photo, image=picture, mode="normal", fit="width", resample="nearest", **params)


def test_a_fit_by_width_sizes_the_picture_as_a_share_of_the_photos_width_and_touches_nothing_else(photo):
    # 96 x 128 photo, scale 0.25: the logo is 32 px wide and so 16 tall (each logo pixel 2 x 2), top left, no margin
    out = put_logo(photo, at="top_left", scale=0.25, margin=0)
    by_hand = np.repeat(np.repeat(logo_pixels()[..., :3], 2, axis=0), 2, axis=1)
    np.testing.assert_array_equal(out[:16, :32], by_hand)
    outside = np.ones(photo.shape[:2], bool)
    outside[:16, :32] = False
    np.testing.assert_array_equal(out[outside], photo[outside])


def test_a_logo_with_alpha_0_leaves_the_photo_as_it_was(photo):
    clear = logo_pixels()
    clear[..., 3] = 0
    np.testing.assert_array_equal(put_logo(photo, clear), photo)


def test_a_half_transparent_logo_mixes_half_of_the_photo_into_its_box(photo):
    half = logo_pixels()
    half[..., 3] = 0.5
    out = put_logo(photo, half, at="top_left", scale=0.25, margin=0)
    by_hand = np.repeat(np.repeat(logo_pixels()[..., :3], 2, axis=0), 2, axis=1)
    np.testing.assert_allclose(out[:16, :32], 0.5 * photo[:16, :32] + 0.5 * by_hand, atol=1e-6)


@pytest.mark.parametrize(
    ("at", "top", "left"),
    [("top_left", 0, 0), ("top_right", 0, 96), ("bottom_left", 80, 0), ("bottom_right", 80, 96), ("center", 40, 48)],
)
def test_a_fit_by_width_puts_the_picture_at_its_anchor(photo, at, top, left):
    rows, columns = np.nonzero(np.any(put_logo(photo, at=at, scale=0.25, margin=0) != photo, axis=2))
    assert (rows.min(), columns.min(), rows.max(), columns.max()) == (top, left, top + 15, left + 31)


def test_the_margin_is_px_from_the_edges(photo):
    out = put_logo(photo, at="bottom_right", scale=0.25, margin=16)
    rows, columns = np.nonzero(np.any(out != photo, axis=2))
    assert (int(rows.max()), int(columns.max())) == (96 - 16 - 1, 128 - 16 - 1)


def test_a_cut_out_keeps_its_shape_so_the_photo_shows_through_its_transparent_pixels(photo):
    cutout = np.zeros((4, 4, 4), np.float32)
    cutout[1:3, 1:3] = (0, 1, 0, 1)  # a green square in the middle of a clear picture
    out = put_logo(photo, cutout, at="top_left", scale=0.25, margin=0)
    np.testing.assert_array_equal(out[8:24, 8:24], np.broadcast_to([0, 1, 0], (16, 16, 3)))  # 32 px: 8 px per pixel
    np.testing.assert_array_equal(out[:8], photo[:8])
    np.testing.assert_array_equal(out[8:24, :8], photo[8:24, :8])


def logo_corner(photo, **settings):
    """The photo with the logo laid by a `blend` step, drawn for 480 px and rendered at the photo's 128 px, and the
    rows and columns the logo changed."""
    step = fx.blend(image=Picture(logo_pixels()), **settings)
    out = render(photo, Recipe("logo", size=480, steps=[step]), size=128)
    return out, *np.nonzero(np.any(out != photo, axis=2))


def test_the_logo_preset_puts_an_opaque_picture_a_fifth_of_the_photos_width_in_the_bottom_right_corner(photo):
    out, rows, columns = logo_corner(photo, preset="overlay")
    assert abs((columns.max() - columns.min() + 1) - 0.2 * 128) <= 1  # a fifth of the photo's 128 px
    assert 1 <= 96 - 1 - rows.max() <= 4  # a small margin from the bottom edge
    assert 1 <= 128 - 1 - columns.max() <= 4  # and from the right edge
    np.testing.assert_array_equal(out[rows.max(), columns.min()], [1, 0, 0])  # shown as it is: the normal mode


def test_a_parameter_given_beside_the_logo_preset_wins(photo):
    _, _rows, columns = logo_corner(photo, preset="overlay", scale=0.01)
    assert columns.max() - columns.min() + 1 <= 2  # 1 % of 128 px
