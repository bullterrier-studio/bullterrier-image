"""Behaviour of the `color_balance` effect."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "color_balance"
BLACK, MID, WHITE = (0.0, 0.0, 0.0), (0.5, 0.5, 0.5), (1.0, 1.0, 1.0)


def pixel(colour: tuple[float, ...]) -> np.ndarray:
    return np.array(colour, np.float32).reshape(1, 1, 3)


def luma(rgb: np.ndarray) -> np.ndarray:
    """Rec. 709 luma of gamma-encoded sRGB (the brightness the tone weights are read from)."""
    return rgb @ np.array([0.2126, 0.7152, 0.0722], np.float32)


# Each row: a picture of one pixel, the settings, and the pixel that comes out, worked out by hand from the documented
# weights: shadows 1 - s(L), midtones s(L) - s(2L - 1), highlights s(2L - 1) with s the smoothstep 3x^2 - 2x^3 of 2L
# (clamped to 0..1), so black is all shadows, mid grey all midtones, white all highlights.
SHIFTS = {
    "shadows_red moves black's red": (BLACK, {"shadows_red": 0.1}, (0.1, 0.0, 0.0)),
    "shadows_green moves black's green": (BLACK, {"shadows_green": 0.2}, (0.0, 0.2, 0.0)),
    "shadows_blue moves black's blue": (BLACK, {"shadows_blue": 0.3}, (0.0, 0.0, 0.3)),
    "midtones_red moves mid grey's red": (MID, {"midtones_red": 0.1}, (0.6, 0.5, 0.5)),
    "midtones_green moves mid grey's green": (MID, {"midtones_green": -0.2}, (0.5, 0.3, 0.5)),
    "midtones_blue moves mid grey's blue": (MID, {"midtones_blue": 0.3}, (0.5, 0.5, 0.8)),
    "highlights_red moves white's red": (WHITE, {"highlights_red": -0.1}, (0.9, 1.0, 1.0)),
    "highlights_green moves white's green": (WHITE, {"highlights_green": -0.2}, (1.0, 0.8, 1.0)),
    "highlights_blue moves white's blue": (WHITE, {"highlights_blue": -0.3}, (1.0, 1.0, 0.7)),
    "a shadow shift leaves white alone": (WHITE, {"shadows_red": 0.5, "shadows_blue": -0.5}, WHITE),
    "a highlight shift leaves black alone": (BLACK, {"highlights_green": 0.5, "highlights_red": 0.5}, BLACK),
    "a midtone shift leaves black alone": (BLACK, {"midtones_red": 0.5}, BLACK),
    "a midtone shift leaves white alone": (WHITE, {"midtones_blue": -0.5}, WHITE),
    "a shadow shift leaves mid grey alone": (MID, {"shadows_green": 0.5}, MID),
    "a highlight shift leaves mid grey alone": (MID, {"highlights_red": 0.5}, MID),
    "grey 0.25 is half shadow, half midtone": (
        (0.25, 0.25, 0.25),
        {"shadows_red": 0.2, "midtones_red": 0.4},
        (0.55, 0.25, 0.25),  # 0.25 + 0.5 * 0.2 + 0.5 * 0.4
    ),
    "grey 0.75 is half midtone, half highlight": (
        (0.75, 0.75, 0.75),
        {"midtones_green": -0.2, "highlights_green": -0.4},
        (0.75, 0.45, 0.75),  # 0.75 - 0.5 * 0.2 - 0.5 * 0.4
    ),
    "grey 0.125 is 0.84375 shadow, 0.15625 midtone": (
        (0.125, 0.125, 0.125),
        {"shadows_green": 0.32, "midtones_green": -0.16},
        (0.125, 0.125 + 0.84375 * 0.32 - 0.15625 * 0.16, 0.125),
    ),
    "the result is held to 0..1": (WHITE, {"highlights_red": 0.4, "highlights_green": -1.0}, (1.0, 0.0, 1.0)),
}


@pytest.mark.parametrize(("start", "settings", "expected"), SHIFTS.values(), ids=list(SHIFTS))
def test_each_shift_moves_its_own_channel_in_its_own_tones(start, settings, expected) -> None:
    out = registry.apply(NAME, pixel(start), preserve_luminosity=False, **settings)
    np.testing.assert_allclose(out, pixel(expected), atol=1e-6)


@pytest.mark.parametrize("preserve", [True, False], ids=["luminosity kept", "luminosity free"])
def test_zero_shifts_leave_the_picture_unchanged(photo: np.ndarray, preserve: bool) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo, preserve_luminosity=preserve), photo, atol=1e-6)


@pytest.mark.parametrize(
    ("preserve", "expected"),
    [(False, (0.6, 0.5, 0.5)), (True, (0.57874, 0.47874, 0.47874))],  # 0.1 of red is 0.2126 * 0.1 = 0.02126 of luma
    ids=["luminosity free", "luminosity kept: the shift's luma is taken off every channel"],
)
def test_preserve_luminosity_takes_the_brightness_the_shift_added_back_out(
    preserve: bool, expected: tuple[float, ...]
) -> None:
    out = registry.apply(NAME, pixel(MID), midtones_red=0.1, preserve_luminosity=preserve)
    np.testing.assert_allclose(out, pixel(expected), atol=1e-6)


def test_preserve_luminosity_keeps_every_pixels_luma_where_nothing_clips(photo: np.ndarray) -> None:
    moderate = 0.3 + 0.4 * photo  # 0.3..0.7, so shifts of at most 0.2 cannot reach 0 or 1
    shifts = {"shadows_red": 0.2, "midtones_blue": -0.2, "highlights_green": 0.2, "midtones_red": 0.1}
    kept = registry.apply(NAME, moderate, preserve_luminosity=True, **shifts)
    free = registry.apply(NAME, moderate, preserve_luminosity=False, **shifts)
    np.testing.assert_allclose(luma(kept), luma(moderate), atol=1e-5)
    assert np.abs(luma(free) - luma(moderate)).max() > 0.02  # without it the luma does move


def test_a_shadow_shift_moves_the_dark_pixels_and_never_the_bright_ones(photo: np.ndarray) -> None:
    moved = np.abs(registry.apply(NAME, photo, shadows_red=0.3, preserve_luminosity=False)[..., 0] - photo[..., 0])
    brightness = luma(photo)
    assert moved[brightness < 0.3].mean() > moved[(brightness >= 0.3) & (brightness < 0.5)].mean() > 0
    assert (moved[brightness >= 0.5] == 0).all()  # shadows end at mid grey


def test_a_highlight_shift_moves_the_bright_pixels_and_never_the_dark_ones(photo: np.ndarray) -> None:
    moved = np.abs(registry.apply(NAME, photo, highlights_blue=-0.3, preserve_luminosity=False)[..., 2] - photo[..., 2])
    brightness = luma(photo)
    assert moved[brightness > 0.7].mean() > moved[(brightness <= 0.7) & (brightness > 0.5)].mean() > 0
    assert (moved[brightness <= 0.5] == 0).all()  # highlights start at mid grey
