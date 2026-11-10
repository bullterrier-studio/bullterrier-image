"""The `blend` effect through the Python facade: its `overlay` preset, a `Picture` handed to a step, and the size a
recipe is drawn for. How a picture is fitted and blended is tested in Rust (`crates/core/tests/it/blend_effect.rs`)."""

import numpy as np
import pytest

from bullterrier_image import Picture, Recipe, fx, render


def logo_pixels() -> np.ndarray:
    """An 8 x 16 (height x width) logo: opaque red on the left half, opaque blue on the right half."""
    pixels = np.ones((8, 16, 4), np.float32)
    pixels[:, :8, :3] = (1, 0, 0)
    pixels[:, 8:, :3] = (0, 0, 1)
    return pixels


def logo_corner(photo, **settings):
    """The photo with the logo laid by a `blend` step, drawn for 480 px and rendered at the photo's 128 px, and the
    rows and columns the logo changed."""
    step = fx.blend(image=Picture(logo_pixels()), **settings)
    out = render(photo, Recipe(name="logo", size=480, steps=[step]), size=128)
    return out, *np.nonzero(np.any(out != photo, axis=2))


def test_the_logo_preset_puts_an_opaque_picture_a_fifth_of_the_photos_width_in_the_bottom_right_corner(photo):
    out, rows, columns = logo_corner(photo, preset="overlay")
    assert abs((columns.max() - columns.min() + 1) - 0.2 * 128) <= 1  # a fifth of the photo's 128 px
    assert 1 <= 96 - 1 - rows.max() <= 4  # a small margin from the bottom edge
    assert 1 <= 128 - 1 - columns.max() <= 4  # and from the right edge
    np.testing.assert_array_equal(out[rows.max(), columns.min()], [1, 0, 0])  # shown as it is: the normal mode


@pytest.mark.parametrize("size", [480, 1024], ids=["size 480", "size 1024"])
def test_the_logo_preset_keeps_the_same_margin_relative_to_the_frame_whatever_the_recipes_size(photo, size):
    step = fx.blend(image=Picture(logo_pixels()), preset="overlay")
    out = render(photo, Recipe(name="logo", size=size, steps=[step]))
    _, columns = np.nonzero(np.all(np.isclose(out, (0, 0, 1), atol=0.01), axis=2))  # the logo's blue half
    margin = out.shape[1] - 1 - columns.max()
    assert abs(margin / size - 12 / 480) <= 1.5 / size  # the preset's 12 px of a 480 px frame


def test_a_parameter_given_beside_the_logo_preset_wins(photo):
    _, _rows, columns = logo_corner(photo, preset="overlay", scale=0.01)
    assert columns.max() - columns.min() + 1 <= 2  # 1 % of 128 px


def test_a_colour_given_to_fx_is_a_flat_picture_and_none_leaves_the_photo_as_it_was(photo):
    red = render(photo, Recipe(name="red", steps=[fx.blend(image="#ff0000", mode="normal")]))
    np.testing.assert_array_equal(red, np.broadcast_to([1, 0, 0], photo.shape))
    np.testing.assert_array_equal(render(photo, Recipe(name="none", steps=[fx.blend()])), photo)
