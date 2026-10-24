"""Behaviour of the `overlay` effect: a logo, sized by the photo's width, placed at an anchor, with its own alpha."""

import numpy as np
import pytest

from bullterrier_image import Picture
from bullterrier_image.core import registry

NAME = "overlay"


def logo_pixels() -> np.ndarray:
    """An 8 x 16 (height x width) logo: opaque red on the left half, opaque blue on the right half."""
    pixels = np.ones((8, 16, 4), np.float32)
    pixels[:, :8, :3] = (1, 0, 0)
    pixels[:, 8:, :3] = (0, 0, 1)
    return pixels


def test_a_logo_with_alpha_0_leaves_the_photo_as_it_was(photo):
    clear = logo_pixels()
    clear[..., 3] = 0
    np.testing.assert_array_equal(registry.apply(NAME, photo, image=Picture(clear)), photo)


def test_an_opaque_logo_fills_its_box_with_the_logo_resized_by_hand_and_nothing_else(photo):
    # 96 x 128 photo, scale 0.25: the logo is 32 px wide and so 16 tall (each logo pixel 2 x 2), top left, no margin
    out = registry.apply(
        NAME, photo, image=Picture(logo_pixels()), at="top_left", scale=0.25, margin=0, resample="nearest"
    )
    by_hand = np.repeat(np.repeat(logo_pixels()[..., :3], 2, axis=0), 2, axis=1)
    np.testing.assert_array_equal(out[:16, :32], by_hand)
    mask = np.ones(photo.shape[:2], bool)
    mask[:16, :32] = False
    np.testing.assert_array_equal(out[mask], photo[mask])


def test_a_half_transparent_logo_mixes_half_of_the_photo_into_its_box(photo):
    half = logo_pixels()
    half[..., 3] = 0.5
    out = registry.apply(NAME, photo, image=Picture(half), at="top_left", scale=0.25, margin=0, resample="nearest")
    by_hand = np.repeat(np.repeat(logo_pixels()[..., :3], 2, axis=0), 2, axis=1)
    np.testing.assert_allclose(out[:16, :32], 0.5 * photo[:16, :32] + 0.5 * by_hand, atol=1e-6)


@pytest.mark.parametrize(
    ("at", "top", "left"),
    [("top_left", 0, 0), ("top_right", 0, 96), ("bottom_left", 80, 0), ("bottom_right", 80, 96), ("center", 40, 48)],
)
def test_the_logo_sits_at_its_anchor(photo, at, top, left):
    out = registry.apply(NAME, photo, image=Picture(logo_pixels()), at=at, scale=0.25, margin=0, resample="nearest")
    changed = np.any(out != photo, axis=2)
    rows, columns = np.nonzero(changed)
    assert (rows.min(), columns.min(), rows.max(), columns.max()) == (top, left, top + 15, left + 31)


def test_the_margin_moves_a_logo_at_an_edge_inward_by_a_share_of_the_shorter_side(photo):
    out = registry.apply(
        NAME, photo, image=Picture(logo_pixels()), at="bottom_right", scale=0.25, margin=0.125, resample="nearest"
    )  # 12 px of the 96 px shorter side
    rows, columns = np.nonzero(np.any(out != photo, axis=2))
    assert (int(rows.max()), int(columns.max())) == (96 - 12 - 1, 128 - 12 - 1)


def test_a_cut_out_keeps_its_shape_so_the_photo_shows_through_its_transparent_pixels(photo):
    cutout = np.zeros((4, 4, 4), np.float32)
    cutout[1:3, 1:3] = (0, 1, 0, 1)  # a green square in the middle of a clear picture
    out = registry.apply(NAME, photo, image=Picture(cutout), at="top_left", scale=0.25, margin=0, resample="nearest")
    np.testing.assert_array_equal(out[8:24, 8:24], np.broadcast_to([0, 1, 0], (16, 16, 3)))  # 32 px: 8 px per pixel
    np.testing.assert_array_equal(out[:8], photo[:8])
    np.testing.assert_array_equal(out[8:24, :8], photo[8:24, :8])
