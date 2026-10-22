"""Tests of `ops/resample.py`."""

import numpy as np

from photo_fx import imageio
from photo_fx.ops.resample import resize


def test_resize_keeps_aspect_ratio_and_range(photo, matte):
    small = resize(photo, 64)
    assert small.shape == (48, 64, 3)
    assert small.min() >= 0.0
    assert small.max() <= 1.0
    assert resize(matte, 64).shape == (48, 64)


def test_resize_to_same_size_is_a_no_op(photo):
    assert resize(photo, 128) is photo


def test_imageio_resize_still_works_and_gives_the_same_pixels(photo, matte):
    """`imageio.resize` was public before the function moved to ops; it must keep working, identically."""
    np.testing.assert_array_equal(imageio.resize(photo, 64), resize(photo, 64))
    np.testing.assert_array_equal(imageio.resize(matte, 200), resize(matte, 200))  # enlarging too
