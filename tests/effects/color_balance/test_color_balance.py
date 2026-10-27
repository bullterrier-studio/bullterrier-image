"""Behaviour of the `color_balance` effect."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "color_balance"
BLACK, DARK, MID, LIGHT, WHITE = (0.0,) * 3, (0.2,) * 3, (0.5,) * 3, (0.8,) * 3, (1.0,) * 3
GREY = (0.5, 0.5, 0.5)  # the colour that leaves a range alone


def pixel(colour: tuple[float, ...]) -> np.ndarray:
    return np.array(colour, np.float32).reshape(1, 1, 3)


def luma(rgb: np.ndarray) -> np.ndarray:
    """Rec. 709 luma of gamma-encoded sRGB (the brightness the tone weights are read from)."""
    return rgb @ np.array([0.2126, 0.7152, 0.0722], np.float32)


def push(red: float = 0.0, green: float = 0.0, blue: float = 0.0) -> tuple[float, float, float]:
    """The colour that moves a range's pixels by (red, green, blue) at amount 1: its distance from mid grey."""
    return (0.5 + red, 0.5 + green, 0.5 + blue)


def tones(**colours: object) -> dict[str, object]:
    """Settings that push the named ranges and leave the others at mid grey, with the brightness free to move."""
    return {
        "shadows": GREY,
        "midtones": GREY,
        "highlights": GREY,
        "amount": 1.0,
        "preserve_luminosity": False,
    } | colours


# Each row: a picture of one pixel, the settings, and the pixel that comes out, worked out by hand from the documented
# weights: shadows 1 - s(2L), midtones s(2L) - s(2L - 1), highlights s(2L - 1) with s the smoothstep 3x^2 - 2x^3
# (clamped to 0..1), so black is all shadows, mid grey all midtones, white all highlights.
SHIFTS = {
    "shadows move black's red": (BLACK, tones(shadows=push(red=0.1)), (0.1, 0.0, 0.0)),
    "shadows move black's green": (BLACK, tones(shadows=push(green=0.2)), (0.0, 0.2, 0.0)),
    "shadows move black's blue": (BLACK, tones(shadows=push(blue=0.3)), (0.0, 0.0, 0.3)),
    "midtones move mid grey's red": (MID, tones(midtones=push(red=0.1)), (0.6, 0.5, 0.5)),
    "midtones move mid grey's green": (MID, tones(midtones=push(green=-0.2)), (0.5, 0.3, 0.5)),
    "midtones move mid grey's blue": (MID, tones(midtones=push(blue=0.3)), (0.5, 0.5, 0.8)),
    "highlights move white's red": (WHITE, tones(highlights=push(red=-0.1)), (0.9, 1.0, 1.0)),
    "highlights move white's green": (WHITE, tones(highlights=push(green=-0.2)), (1.0, 0.8, 1.0)),
    "highlights move white's blue": (WHITE, tones(highlights=push(blue=-0.3)), (1.0, 1.0, 0.7)),
    "shadows leave white alone": (WHITE, tones(shadows=push(0.5, 0, -0.5)), WHITE),
    "highlights leave black alone": (BLACK, tones(highlights=push(0.5, 0.5)), BLACK),
    "midtones leave black alone": (BLACK, tones(midtones=push(red=0.5)), BLACK),
    "midtones leave white alone": (WHITE, tones(midtones=push(blue=-0.5)), WHITE),
    "shadows leave mid grey alone": (MID, tones(shadows=push(green=0.5)), MID),
    "highlights leave mid grey alone": (MID, tones(highlights=push(red=0.5)), MID),
    "grey 0.25 is half shadow, half midtone": (
        (0.25,) * 3,
        tones(shadows=push(red=0.2), midtones=push(red=0.4)),
        (0.55, 0.25, 0.25),  # 0.25 + 0.5 * 0.2 + 0.5 * 0.4
    ),
    "grey 0.75 is half midtone, half highlight": (
        (0.75,) * 3,
        tones(midtones=push(green=-0.2), highlights=push(green=-0.4)),
        (0.75, 0.45, 0.75),  # 0.75 - 0.5 * 0.2 - 0.5 * 0.4
    ),
    "grey 0.125 is 0.84375 shadow, 0.15625 midtone": (
        (0.125,) * 3,
        tones(shadows=push(green=0.32), midtones=push(green=-0.16)),
        (0.125, 0.125 + 0.84375 * 0.32 - 0.15625 * 0.16, 0.125),
    ),
    "amount scales the push": (BLACK, tones(shadows=push(red=0.4), amount=0.25), (0.1, 0.0, 0.0)),
    "the result is held above 0": (BLACK, tones(shadows=push(-0.5, 0.5)), (0.0, 0.5, 0.0)),
    "the result is held below 1": (WHITE, tones(highlights=push(0.5, -0.5)), (1.0, 0.5, 1.0)),
}


@pytest.mark.parametrize(("start", "settings", "expected"), SHIFTS.values(), ids=list(SHIFTS))
def test_each_range_pushes_its_colour_in_its_own_tones(start, settings, expected) -> None:
    out = registry.apply(NAME, pixel(start), **settings)
    np.testing.assert_allclose(out, pixel(expected), atol=1e-6)


@pytest.mark.parametrize("preserve", [True, False], ids=["luminosity kept", "luminosity free"])
def test_mid_grey_in_every_range_leaves_the_picture_unchanged(photo: np.ndarray, preserve: bool) -> None:
    out = registry.apply(NAME, photo, **tones(preserve_luminosity=preserve))
    np.testing.assert_allclose(out, photo, atol=1e-6)


@pytest.mark.parametrize(
    ("preserve", "expected"),
    [(False, (0.6, 0.5, 0.5)), (True, (0.57874, 0.47874, 0.47874))],  # 0.1 of red is 0.2126 * 0.1 = 0.02126 of luma
    ids=["luminosity free", "luminosity kept: the push's luma is taken off every channel"],
)
def test_preserve_luminosity_takes_the_brightness_the_push_added_back_out(
    preserve: bool, expected: tuple[float, ...]
) -> None:
    out = registry.apply(NAME, pixel(MID), **tones(midtones=push(red=0.1), preserve_luminosity=preserve))
    np.testing.assert_allclose(out, pixel(expected), atol=1e-6)


def test_preserve_luminosity_keeps_every_pixels_luma_where_nothing_clips(photo: np.ndarray) -> None:
    moderate = 0.3 + 0.4 * photo  # 0.3..0.7, so pushes of at most 0.2 cannot reach 0 or 1
    colours = {"shadows": push(red=0.2), "midtones": push(-0.1, blue=-0.2), "highlights": push(green=0.2)}
    kept = registry.apply(NAME, moderate, **tones(preserve_luminosity=True, **colours))
    free = registry.apply(NAME, moderate, **tones(preserve_luminosity=False, **colours))
    np.testing.assert_allclose(luma(kept), luma(moderate), atol=1e-5)
    assert np.abs(luma(free) - luma(moderate)).max() > 0.02  # without it the luma does move


def test_shadows_move_the_dark_pixels_and_never_the_bright_ones(photo: np.ndarray) -> None:
    moved = np.abs(registry.apply(NAME, photo, **tones(shadows=push(red=0.3)))[..., 0] - photo[..., 0])
    brightness = luma(photo)
    assert moved[brightness < 0.3].mean() > moved[(brightness >= 0.3) & (brightness < 0.5)].mean() > 0
    assert (moved[brightness >= 0.5] == 0).all()  # shadows end at mid grey


def test_highlights_move_the_bright_pixels_and_never_the_dark_ones(photo: np.ndarray) -> None:
    moved = np.abs(registry.apply(NAME, photo, **tones(highlights=push(blue=-0.3)))[..., 2] - photo[..., 2])
    brightness = luma(photo)
    assert moved[brightness > 0.7].mean() > moved[(brightness <= 0.7) & (brightness > 0.5)].mean() > 0
    assert (moved[brightness <= 0.5] == 0).all()  # highlights start at mid grey


@pytest.mark.parametrize("softness", [0.01, 0.3, 1.0])
@pytest.mark.parametrize("balance", [0.0, 0.2, 0.5, 0.8, 1.0])
def test_the_three_ranges_always_add_up_to_one(photo: np.ndarray, balance: float, softness: float) -> None:
    """One push given to all three ranges moves every pixel by exactly that push, wherever the ranges sit."""
    moderate = 0.2 + 0.6 * photo  # a push of 0.1 cannot clip
    same = push(0.1, -0.1, 0.05)
    settings = tones(shadows=same, midtones=same, highlights=same)
    out = registry.apply(NAME, moderate, **settings, balance=balance, softness=softness)
    np.testing.assert_allclose(out - moderate, np.broadcast_to([0.1, -0.1, 0.05], out.shape), atol=1e-6)


# Grey 0.4 is 0.104 shadow at balance 0.5 (the shadow/midtone hand-over, centred on 0.25, is 0.896 done); at balance
# 0.8 the hand-over is centred on 0.4, so half shadow; at 1.0 it is centred on 0.5: 0.784 (smoothstep(0.3) = 0.216).
@pytest.mark.parametrize(
    ("balance", "shadow_share"),
    [(0.5, 0.104), (0.8, 0.5), (1.0, 0.784)],
    ids=["centred", "raised: shadows reach higher", "top: the grey is mostly shadow"],
)
def test_raising_balance_makes_more_of_the_picture_shadows(balance: float, shadow_share: float) -> None:
    out = registry.apply(NAME, pixel((0.4,) * 3), **tones(shadows=push(red=0.2)), balance=balance)
    np.testing.assert_allclose(out[0, 0, 0], 0.4 + 0.2 * shadow_share, atol=1e-6)


# At balance 0.5 the shadow/midtone hand-over is centred on 0.25 and `softness` * 0.5 wide, so greys 0.2 and 0.3,
# equally far from it, take shares that add up to 1: (0.648, 0.352) at 1, (0.84375, 0.15625) at 0.4, (1, 0) at 0.1.
@pytest.mark.parametrize(
    ("softness", "dark_share", "lighter_share"),
    [(1.0, 0.648, 0.352), (0.4, 0.84375, 0.15625), (0.1, 1.0, 0.0)],
    ids=["soft", "medium", "sharp"],
)
def test_softness_sets_how_sharply_the_shadows_hand_over(softness, dark_share, lighter_share) -> None:
    dark = registry.apply(NAME, pixel(DARK), **tones(shadows=push(red=0.2)), softness=softness)
    lighter = registry.apply(NAME, pixel((0.3,) * 3), **tones(shadows=push(red=0.2)), softness=softness)
    np.testing.assert_allclose(dark[0, 0, 0], 0.2 + 0.2 * dark_share, atol=1e-6)
    np.testing.assert_allclose(lighter[0, 0, 0], 0.3 + 0.2 * lighter_share, atol=1e-6)


def test_the_names_of_the_cut_effects_and_the_teal_and_orange_grade_are_presets() -> None:
    assert {"split_tone", "color_balance", "teal_orange"} <= set(registry.presets(NAME))
