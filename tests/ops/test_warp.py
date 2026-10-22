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


def _shift(image, sx, sy, **kwargs):
    """Warp so that the content moves by (+sx, +sy) pixels: output[r, c] = image[r - sy, c - sx]."""
    h, w = image.shape[:2]
    return remap(image, np.full((h, w), -sx, np.float32), np.full((h, w), -sy, np.float32), **kwargs)


@pytest.mark.parametrize("sx, sy", [(2, 0), (0, 3), (-3, 0), (0, -2), (4, -3), (-5, 6)])
def test_wrap_integer_shift_equals_np_roll(sx, sy):
    image = _picture(7, 9)
    np.testing.assert_array_equal(_shift(image, sx, sy, mode="wrap"), np.roll(image, (sy, sx), axis=(0, 1)))


@pytest.mark.parametrize("sx, sy", [(2, 0), (0, 3), (-3, 0), (0, -2), (4, -3), (-5, 6)])
def test_clamp_integer_shift_equals_a_window_of_the_edge_padded_input(sx, sy):
    image = _picture(7, 9)
    h, w = image.shape[:2]
    padded = np.pad(image, ((6, 6), (6, 6), (0, 0)), mode="edge")
    expected = padded[6 - sy : 6 - sy + h, 6 - sx : 6 - sx + w]
    np.testing.assert_array_equal(_shift(image, sx, sy, mode="clamp"), expected)


@pytest.mark.parametrize("sx, sy", [(2, 0), (0, 3), (-3, 0), (0, -2), (4, -3), (-5, 6)])
def test_mirror_integer_shift_equals_a_window_of_the_symmetric_padded_input(sx, sy):
    image = _picture(7, 9)
    h, w = image.shape[:2]
    padded = np.pad(image, ((6, 6), (6, 6), (0, 0)), mode="symmetric")
    expected = padded[6 - sy : 6 - sy + h, 6 - sx : 6 - sx + w]
    np.testing.assert_array_equal(_shift(image, sx, sy, mode="mirror"), expected)


@pytest.mark.parametrize("sx, sy", [(2, 0), (0, 3), (-3, 0), (0, -2), (4, -3)])
def test_fill_integer_shift_keeps_shifted_input_and_fills_exactly_the_uncovered_border(sx, sy):
    image = _picture(7, 9)
    h, w = image.shape[:2]
    out = _shift(image, sx, sy, mode="fill", fill=0.37)

    expected = np.full_like(image, 0.37)
    src_rows, src_cols = slice(max(0, -sy), h - max(0, sy)), slice(max(0, -sx), w - max(0, sx))
    dst_rows, dst_cols = slice(max(0, sy), h - max(0, -sy)), slice(max(0, sx), w - max(0, -sx))
    expected[dst_rows, dst_cols] = image[src_rows, src_cols]
    np.testing.assert_array_equal(out, expected)


def test_huge_displacements_stay_periodic_in_wrap_and_reflected_in_mirror():
    image = _picture(7, 9)
    h, w = image.shape[:2]
    big_x, big_y = 1_000_003, -2_000_001  # not multiples of the picture size
    wrapped = _shift(image, big_x, big_y, mode="wrap")
    np.testing.assert_array_equal(wrapped, np.roll(image, (big_y % h, big_x % w), axis=(0, 1)))
    # mirror has period 2n, so a shift by exactly 2n pixels is the identity
    np.testing.assert_array_equal(_shift(image, 2 * w * 1000, 2 * h * 1000, mode="mirror"), image)
    far = _shift(image, 10**6, 10**6, mode="clamp")
    np.testing.assert_array_equal(far, np.broadcast_to(image[0, 0], image.shape))
