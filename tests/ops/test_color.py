"""Tests of `ops/color.py`: luminance and how much ink a mark-making effect lays down."""

import numpy as np

from photo_fx.ops.color import ink_amount, luminance


def test_luminance_uses_the_rec709_weights():
    primaries = np.eye(3, dtype=np.float32)[None]  # one red, one green, one blue pixel
    np.testing.assert_allclose(luminance(primaries)[0], [0.2126, 0.7152, 0.0722], atol=1e-6)  # ITU-R BT.709


def test_ink_follows_brightness_on_dark_paper_and_darkness_on_light_paper():
    lum = np.array([0.8, 0.3], np.float32)
    np.testing.assert_allclose(ink_amount(lum, np.zeros(3, np.float32), threshold=0.5), [0.8, 0.3], atol=1e-6)
    np.testing.assert_allclose(ink_amount(lum, np.ones(3, np.float32), threshold=0.5), [0.2, 0.7], atol=1e-6)
