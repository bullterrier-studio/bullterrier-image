"""Tests of `ops/resample.py`."""

import numpy as np

from bullterrier_image.ops.resample import resize, stretch


def test_resize_keeps_aspect_ratio_and_range(photo, subject):
    small = resize(photo, 64)
    assert small.shape == (48, 64, 3)
    assert small.min() >= 0.0
    assert small.max() <= 1.0
    assert resize(subject, 64).shape == (48, 64)


def test_resize_to_same_size_is_a_no_op(photo):
    assert resize(photo, 128) is photo


def test_resize_enlarges_too_keeping_aspect_ratio_and_range(subject):
    big = resize(subject, 200)
    assert big.shape == (150, 200)
    assert big.min() >= 0.0
    assert big.max() <= 1.0


def test_stretch_reaches_exactly_the_size_asked_for_and_stays_within_the_maps_range():
    ramp = np.tile(np.linspace(0.2, 0.7, 40, dtype=np.float32), (10, 1))  # rising to the right, wide
    out = stretch(ramp, 30, 20)  # tall: the aspect ratio follows the target
    assert out.shape == (30, 20)
    assert out.dtype == np.float32
    assert out.min() >= 0.2 - 1e-6  # float rounding of the blend only
    assert out.max() <= 0.7 + 1e-6
    assert (np.diff(out, axis=1) >= 0).all()  # still rising to the right
    assert (out == out[0]).all()  # and still the same in every row


def test_stretch_gives_an_8_bit_maps_levels_as_floats():
    np.testing.assert_array_equal(stretch(np.full((4, 6), 200, np.uint8), 9, 3), np.full((9, 3), 200, np.float32))


def test_stretch_blends_neighbours_linearly_by_pixel_centre():
    """Two pixels, 0 and 1, to four: the output centres fall at -0.25, 0.25, 0.75 and 1.25 input pixels, so (with the
    border repeated) they read 0, 1/4, 3/4 and 1."""
    out = stretch(np.array([[0.0, 1.0]], np.float32), 1, 4)
    np.testing.assert_allclose(out, [[0.0, 0.25, 0.75, 1.0]], atol=1e-6)
