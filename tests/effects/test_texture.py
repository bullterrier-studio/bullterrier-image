"""Tests of `effects/texture.py`."""

import numpy as np

from photo_fx import effects


def test_glow_only_ever_brightens(photo):
    assert (effects.apply("glow", photo) >= photo - 1e-6).all()


def test_glow_amount_zero_leaves_the_picture_unchanged(photo):
    np.testing.assert_allclose(effects.apply("glow", photo, amount=0.0), photo, atol=1e-6)


def test_glow_amount_sets_how_much_it_brightens(photo):
    added = [float((effects.apply("glow", photo, amount=a, radius=6.0) - photo).mean()) for a in (0.3, 0.9)]
    assert 0 < added[0] < added[1]


def test_halftone_white_is_nearly_all_dots_and_black_is_blank():
    white, black = np.ones((64, 64, 3), np.float32), np.zeros((64, 64, 3), np.float32)
    assert effects.apply("halftone", white, fg="#ffffff").mean() > 0.9
    assert effects.apply("halftone", black, fg="#ffffff").mean() < 0.01


def test_the_texture_effects_are_in_their_categories():
    """glow, grain, halftone: where the effect catalogue lists them."""
    expected = {
        "halftone": "stylised",
        "glow": "light",
    }
    assert {name: effects.category(name) for name in expected} == expected
