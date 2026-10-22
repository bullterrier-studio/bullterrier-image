"""Tests of `effects/texture.py`."""

import numpy as np

from photo_fx import effects


def test_halftone_white_is_nearly_all_dots_and_black_is_blank():
    white, black = np.ones((64, 64, 3), np.float32), np.zeros((64, 64, 3), np.float32)
    assert effects.apply("halftone", white, fg="#ffffff").mean() > 0.9
    assert effects.apply("halftone", black, fg="#ffffff").mean() < 0.01


def test_halftone_is_stylised():
    assert effects.category("halftone") == "stylised"
