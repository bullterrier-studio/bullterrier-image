"""Tests of `ops/color.py`: colour literals, luminance and how much ink a mark-making effect lays down."""

import numpy as np
import pytest

from photo_fx.ops.color import ink_amount, luminance, parse_color


def test_parse_color_forms():
    np.testing.assert_allclose(parse_color("#ff8000"), [1.0, 128 / 255, 0.0], atol=1e-6)
    np.testing.assert_allclose(parse_color("#f00"), [1, 0, 0])
    np.testing.assert_allclose(parse_color([0.1, 0.2, 0.3]), [0.1, 0.2, 0.3])
    with pytest.raises(ValueError, match="'#12'"):
        parse_color("#12")


def test_luminance_uses_the_rec709_weights():
    primaries = np.eye(3, dtype=np.float32)[None]  # one red, one green, one blue pixel
    np.testing.assert_allclose(luminance(primaries)[0], [0.2126, 0.7152, 0.0722], atol=1e-6)  # ITU-R BT.709


def test_ink_follows_brightness_on_dark_paper_and_darkness_on_light_paper():
    lum = np.array([0.8, 0.3], np.float32)
    np.testing.assert_allclose(ink_amount(lum, parse_color("#000000"), threshold=0.5), [0.8, 0.3], atol=1e-6)
    np.testing.assert_allclose(ink_amount(lum, parse_color("#ffffff"), threshold=0.5), [0.2, 0.7], atol=1e-6)
