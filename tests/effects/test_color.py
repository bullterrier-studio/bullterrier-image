"""Tests of `effects/color.py` (and the colour parsing its effects share)."""

import numpy as np
import pytest

from photo_fx import effects
from photo_fx.effects._common import parse_color


def test_parse_color_forms():
    np.testing.assert_allclose(parse_color("#ff8000"), [1.0, 128 / 255, 0.0], atol=1e-6)
    np.testing.assert_allclose(parse_color("#f00"), [1, 0, 0])
    np.testing.assert_allclose(parse_color([0.1, 0.2, 0.3]), [0.1, 0.2, 0.3])
    with pytest.raises(ValueError):
        parse_color("#12")


def test_fill_ignores_the_picture(photo):
    out = effects.apply("fill", photo, color="#102030")
    assert np.allclose(out, parse_color("#102030"))


def test_gradient_map_end_points_use_first_and_last_stop():
    ramp = np.linspace(0, 1, 256, dtype=np.float32)[None, :, None].repeat(3, axis=2).repeat(4, axis=0)
    out = effects.apply("gradient_map", ramp, stops=["#ff0000", "#00ff00", "#0000ff"])
    np.testing.assert_allclose(out[0, 0], [1, 0, 0], atol=0.02)
    np.testing.assert_allclose(out[0, -1], [0, 0, 1], atol=0.02)
    np.testing.assert_allclose(out[0, 128], [0, 1, 0], atol=0.05)


def test_gradient_map_needs_two_stops(photo):
    with pytest.raises(ValueError):
        effects.apply("gradient_map", photo, stops=["#000000"])


def test_hue_shift_full_turn_is_identity(photo):
    np.testing.assert_allclose(effects.apply("hue_shift", photo, degrees=360), photo, atol=1e-5)


def test_hue_shift_keeps_brightness_and_moves_colour():
    from photo_fx.effects._common import luminance

    skin_and_wall = np.array([[[0.8, 0.6, 0.5], [0.5, 0.35, 0.3], [0.6, 0.4, 0.4]]], np.float32)  # in-gamut colours
    out = effects.apply("hue_shift", skin_and_wall, degrees=120)
    np.testing.assert_allclose(luminance(out), luminance(skin_and_wall), atol=1e-4)
    assert (out.argmax(axis=-1) != skin_and_wall.argmax(axis=-1)).all()  # the dominant channel moved


def test_split_tone_tints_without_changing_brightness(photo):
    from photo_fx.effects._common import luminance

    midtones = 0.25 + photo * 0.5  # away from pure black and white, where a tint would clip and change brightness
    out = effects.apply("split_tone", midtones, amount=0.2)
    assert not np.allclose(out, midtones)
    np.testing.assert_allclose(luminance(out), luminance(midtones), atol=1e-4)
    np.testing.assert_array_equal(effects.apply("split_tone", photo, amount=0.0), photo)


def test_split_tone_leaves_pure_black_black():
    assert effects.apply("split_tone", np.zeros((4, 4, 3), np.float32), amount=1.0).max() == 0.0


def test_grade_exposure_one_stop_doubles_brightness():
    dark = np.full((8, 8, 3), 0.2, dtype=np.float32)
    assert np.allclose(effects.apply("grade", dark, exposure=1.0, contrast=1.0), 0.4, atol=1e-5)


def test_clarity_zero_amount_is_identity(photo):
    np.testing.assert_allclose(effects.apply("clarity", photo, amount=0.0), photo, atol=1e-6)


def test_the_color_effects_are_in_their_categories():
    """clarity, fill, grade, gradient_map, hue_shift, split_tone: where the effect catalogue lists them."""
    expected = {
        "fill": "subject",
        "grade": "colour",
        "clarity": "light",
        "gradient_map": "colour",
        "split_tone": "colour",
        "hue_shift": "colour",
    }
    assert {name: effects.category(name) for name in expected} == expected
