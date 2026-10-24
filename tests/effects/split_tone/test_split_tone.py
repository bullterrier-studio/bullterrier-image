"""Behaviour of the `split_tone` effect."""

import numpy as np

from photo_fx.core import registry


def test_split_tone_tints_without_changing_brightness(photo: np.ndarray) -> None:
    midtones = 0.25 + photo * 0.5  # away from pure black and white, where a tint would clip and change brightness
    out = registry.apply("split_tone", midtones, amount=0.2)
    assert not np.allclose(out, midtones)
    rec_709 = [0.2126, 0.7152, 0.0722]  # ITU-R BT.709 luma weights of red, green, blue
    np.testing.assert_allclose(out @ rec_709, midtones @ rec_709, atol=1e-4)
    np.testing.assert_array_equal(registry.apply("split_tone", photo, amount=0.0), photo)


def test_split_tone_leaves_pure_black_black() -> None:
    assert registry.apply("split_tone", np.zeros((4, 4, 3), np.float32), amount=1.0).max() == 0.0


def test_split_tone_softness_sets_how_sharply_one_tint_hands_over_to_the_other() -> None:
    pixel = np.full((1, 1, 3), 0.6, np.float32)  # a little above the balance of 0.5
    red = np.array([1.0, 0.0, 0.0], np.float32)
    red_offset = 0.4 * (red - 0.2126)  # amount x (red minus its Rec. 709 brightness): the pure highlight tint
    sharp = registry.apply("split_tone", pixel, shadow="#0000ff", highlight="#ff0000", amount=0.4, softness=0.1)
    np.testing.assert_allclose(sharp[0, 0] - 0.6, red_offset, atol=1e-6)  # 0.1 above balance is past the hand-over
    soft = registry.apply("split_tone", pixel, shadow="#0000ff", highlight="#ff0000", amount=0.4)
    assert soft[0, 0, 2] > sharp[0, 0, 2]  # with the default softness some of the blue shadow tint is still mixed in


def test_split_tone_black_fade_is_the_brightness_up_to_which_the_tint_fades_in() -> None:
    dark = np.full((1, 1, 3), 0.05, np.float32)
    tinted = {
        fade: registry.apply("split_tone", dark, amount=0.5, black_fade=fade)[0, 0] - 0.05 for fade in (0.05, 0.1, 0.2)
    }
    np.testing.assert_allclose(tinted[0.1], tinted[0.05] * 0.5, atol=1e-6)  # halfway up the fade: half the tint
    np.testing.assert_allclose(tinted[0.2], tinted[0.05] * 0.25, atol=1e-6)
