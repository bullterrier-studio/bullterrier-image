"""Tests of `effects/texture.py`."""

import numpy as np

from photo_fx import effects


def test_glow_only_ever_brightens(photo):
    assert (effects.apply("glow", photo) >= photo - 1e-6).all()


def test_grain_seed_controls_noise(photo):
    a, b = effects.apply("grain", photo, seed=1), effects.apply("grain", photo, seed=2)
    assert not np.allclose(a, b)
    np.testing.assert_array_equal(a, effects.apply("grain", photo, seed=1))


def test_halftone_white_is_nearly_all_dots_and_black_is_blank():
    white, black = np.ones((64, 64, 3), np.float32), np.zeros((64, 64, 3), np.float32)
    assert effects.apply("halftone", white, fg="#ffffff").mean() > 0.9
    assert effects.apply("halftone", black, fg="#ffffff").mean() < 0.01
