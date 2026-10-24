"""Tests of `ops/resample.py`."""

from bullterrier_image.ops.resample import resize


def test_resize_keeps_aspect_ratio_and_range(photo, matte):
    small = resize(photo, 64)
    assert small.shape == (48, 64, 3)
    assert small.min() >= 0.0
    assert small.max() <= 1.0
    assert resize(matte, 64).shape == (48, 64)


def test_resize_to_same_size_is_a_no_op(photo):
    assert resize(photo, 128) is photo


def test_resize_enlarges_too_keeping_aspect_ratio_and_range(matte):
    big = resize(matte, 200)
    assert big.shape == (150, 200)
    assert big.min() >= 0.0
    assert big.max() <= 1.0
