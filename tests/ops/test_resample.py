"""Tests of `ops/resample.py`."""

from photo_fx import imageio
from photo_fx.ops.resample import resize


def test_resize_keeps_aspect_ratio_and_range(photo, matte):
    small = resize(photo, 64)
    assert small.shape == (48, 64, 3)
    assert 0.0 <= small.min() and small.max() <= 1.0
    assert resize(matte, 64).shape == (48, 64)


def test_resize_to_same_size_is_a_no_op(photo):
    assert resize(photo, 128) is photo


def test_imageio_resize_is_the_same_function():
    """`imageio.resize` was public before the function moved to ops; it must keep working, identically."""
    assert imageio.resize is resize
