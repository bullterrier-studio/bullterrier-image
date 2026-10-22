"""Tests of `effects/color.py`."""

import numpy as np
import pytest

from photo_fx import effects
from photo_fx.ops.color import luminance, parse_color


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
    with pytest.raises(ValueError, match="stops"):
        effects.apply("gradient_map", photo, stops=["#000000"])


def test_hue_shift_full_turn_is_identity(photo):
    np.testing.assert_allclose(effects.apply("hue_shift", photo, degrees=360), photo, atol=1e-5)


def test_hue_shift_keeps_brightness_and_moves_colour():
    skin_and_wall = np.array([[[0.8, 0.6, 0.5], [0.5, 0.35, 0.3], [0.6, 0.4, 0.4]]], np.float32)  # in-gamut colours
    out = effects.apply("hue_shift", skin_and_wall, degrees=120)
    np.testing.assert_allclose(luminance(out), luminance(skin_and_wall), atol=1e-4)
    assert (out.argmax(axis=-1) != skin_and_wall.argmax(axis=-1)).all()  # the dominant channel moved


def test_split_tone_tints_without_changing_brightness(photo):
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


# --- the parameters exposed by the migration: each changes what its name says, and nothing at its default ----------


def test_grade_contrast_turns_around_its_pivot():
    grey = np.array([[[0.3, 0.3, 0.3], [0.4, 0.4, 0.4], [0.5, 0.5, 0.5]]], np.float32)
    pivoted = effects.apply("grade", grey, contrast=2.0, pivot=0.3)
    np.testing.assert_allclose(pivoted[0, :, 0], [0.3, 0.5, 0.7], atol=1e-6)  # 0.3 + (v - 0.3) * 2
    default = effects.apply("grade", grey, contrast=2.0)
    np.testing.assert_allclose(default[0, :, 0], [0.1, 0.3, 0.5], atol=1e-6)  # around 0.5: 0.5 + (v - 0.5) * 2


def _dot_on_grey(size: int = 21) -> np.ndarray:
    image = np.full((size, size, 3), 0.5, np.float32)
    image[size // 2, size // 2] = 1.0
    return image


def test_clarity_truncate_limits_how_far_the_surroundings_reach():
    """A Gaussian cut at truncate x sigma (rounded to whole px) has no weight beyond: with sigma 2 and truncate 1 the
    bright dot cannot reach a pixel 4 px away; with the default truncate (4) it does."""
    image = _dot_on_grey()
    near = effects.apply("clarity", image, radius=2.0, truncate=1.0)
    far = effects.apply("clarity", image, radius=2.0)
    assert near[10, 14, 0] == 0.5  # 4 px from the dot: outside the cut kernel, untouched
    assert far[10, 14, 0] < 0.5  # inside the kernel: the bright dot makes it read darker by contrast


def test_clarity_mode_decides_what_lies_beyond_the_picture_edge():
    flat = np.full((12, 12, 3), 0.5, np.float32)
    np.testing.assert_array_equal(effects.apply("clarity", flat, radius=3.0), flat)  # reflect: the edge sees grey
    dark_outside = effects.apply("clarity", flat, radius=3.0, mode="constant", constant_value=0.0)
    assert dark_outside[0, 0, 0] > 0.5  # darkness beyond the edge makes the border stand out brighter
    assert dark_outside[0, 0, 0] > dark_outside[6, 6, 0]
    grey_outside = effects.apply("clarity", flat, radius=3.0, mode="constant", constant_value=0.5)
    np.testing.assert_allclose(grey_outside, flat, atol=1e-6)


def test_gradient_map_clip_fractions_set_how_much_is_pushed_to_the_end_stops():
    ramp = np.linspace(0, 1, 1000, dtype=np.float32)[None, :, None].repeat(3, axis=2)  # every brightness once
    black_white = ["#000000", "#ffffff"]
    unclipped = effects.apply("gradient_map", ramp, stops=black_white, shadow_clip=0.0, highlight_clip=0.0)
    assert (unclipped[0, :, 0] == 0.0).sum() == 1  # only the darkest pixel is the first stop
    clipped = effects.apply("gradient_map", ramp, stops=black_white, shadow_clip=0.1, highlight_clip=0.2)
    assert abs((clipped[0, :, 0] == 0.0).sum() - 100) <= 1  # the darkest 10% are the first stop
    assert abs((clipped[0, :, 0] == 1.0).sum() - 200) <= 1  # the brightest 20% are the last stop


def test_split_tone_softness_sets_how_sharply_one_tint_hands_over_to_the_other():
    pixel = np.full((1, 1, 3), 0.6, np.float32)  # a little above the balance of 0.5
    red = np.array([1.0, 0.0, 0.0], np.float32)
    red_offset = 0.4 * (red - 0.2126)  # amount x (red minus its Rec. 709 brightness): the pure highlight tint
    sharp = effects.apply("split_tone", pixel, shadow="#0000ff", highlight="#ff0000", amount=0.4, softness=0.1)
    np.testing.assert_allclose(sharp[0, 0] - 0.6, red_offset, atol=1e-6)  # 0.1 above balance is past the hand-over
    soft = effects.apply("split_tone", pixel, shadow="#0000ff", highlight="#ff0000", amount=0.4)
    assert soft[0, 0, 2] > sharp[0, 0, 2]  # with the default softness some of the blue shadow tint is still mixed in


def test_split_tone_black_fade_is_the_brightness_up_to_which_the_tint_fades_in():
    dark = np.full((1, 1, 3), 0.05, np.float32)
    tinted = {
        fade: effects.apply("split_tone", dark, amount=0.5, black_fade=fade)[0, 0] - 0.05 for fade in (0.05, 0.1, 0.2)
    }
    np.testing.assert_allclose(tinted[0.1], tinted[0.05] * 0.5, atol=1e-6)  # halfway up the fade: half the tint
    np.testing.assert_allclose(tinted[0.2], tinted[0.05] * 0.25, atol=1e-6)


def test_fill_refuses_a_colour_it_cannot_read(photo):
    with pytest.raises(ValueError, match=r"fill.*`color`.*'#12345'"):
        effects.apply("fill", photo, color="#12345")


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
