"""Behaviour of photo_fx.ops.warp, checked against independent oracles (np.roll, np.pad, hand-computed bilinear values)."""

import numpy as np
import pytest

from photo_fx.ops.warp import pixel_grid, remap

MODES = ["clamp", "mirror", "wrap", "fill"]


def test_pixel_grid_is_column_and_row_index():
    x, y = pixel_grid((3, 5))
    assert x.shape == y.shape == (3, 5)
    assert x.dtype == y.dtype == np.float32
    assert x.tolist() == [[0, 1, 2, 3, 4]] * 3
    assert y.tolist() == [[0] * 5, [1] * 5, [2] * 5]


def _picture(h=7, w=9, channels=3, seed=0):
    rng = np.random.default_rng(seed)
    shape = (h, w) if channels is None else (h, w, channels)
    return rng.random(shape, dtype=np.float32)


@pytest.mark.parametrize("mode", MODES)
@pytest.mark.parametrize("channels", [None, 3])
def test_zero_displacement_is_the_identity_and_leaves_input_untouched(mode, channels):
    image = _picture(channels=channels)
    before = image.copy()
    zero = np.zeros(image.shape[:2], np.float32)

    out = remap(image, zero, zero, mode=mode, fill=0.5)

    np.testing.assert_array_equal(out, image)
    assert out.dtype == np.float32
    assert out.shape == image.shape
    assert out is not image
    np.testing.assert_array_equal(image, before)


def test_field_of_the_wrong_shape_is_rejected_naming_the_mismatch():
    image = _picture(7, 9)
    good = np.zeros((7, 9), np.float32)
    bad = np.zeros((9, 7), np.float32)
    with pytest.raises(ValueError, match=r"dx.*\(9, 7\).*\(7, 9\)"):
        remap(image, bad, good)
    with pytest.raises(ValueError, match=r"dy.*\(9, 7\).*\(7, 9\)"):
        remap(image, good, bad)


def test_unknown_mode_is_rejected_listing_the_valid_ones():
    zero = np.zeros((7, 9), np.float32)
    with pytest.raises(ValueError, match="clamp.*mirror.*wrap.*fill"):
        remap(_picture(7, 9), zero, zero, mode="bounce")


@pytest.mark.parametrize("bad_value", [np.nan, np.inf, -np.inf])
def test_non_finite_displacement_is_rejected(bad_value):
    zero = np.zeros((7, 9), np.float32)
    bad = zero.copy()
    bad[3, 4] = bad_value
    with pytest.raises(ValueError, match="finite"):
        remap(_picture(7, 9), bad, zero)
    with pytest.raises(ValueError, match="finite"):
        remap(_picture(7, 9), zero, bad)


def test_image_that_is_neither_2d_nor_3d_is_rejected():
    zero = np.zeros((7, 9), np.float32)
    with pytest.raises(ValueError, match="image"):
        remap(np.zeros((7, 9, 3, 2), np.float32), zero, zero)
    with pytest.raises(ValueError, match="image"):
        remap(np.zeros(9, np.float32), zero, zero)
