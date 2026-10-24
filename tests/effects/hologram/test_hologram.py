"""Behaviour of the `hologram` effect: a picture in a glowing colour, with edge light, scanlines and flicker."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "hologram"
OFF = {"tint": 0.0, "glow": 0.0, "scanlines": 0.0, "flicker": 0.0}  # every part off: the picture as it is


def holo(rgb: np.ndarray, **settings: object) -> np.ndarray:
    """The picture through a hologram that has only the parts `settings` turns on."""
    return registry.apply(NAME, rgb, **{**OFF, **settings})


def flat(value: float, height: int = 16, width: int = 12) -> np.ndarray:
    return np.full((height, width, 3), value, np.float32)


def test_a_hologram_with_every_part_off_shows_the_picture_as_it_is(photo: np.ndarray) -> None:
    out = holo(photo)
    np.testing.assert_allclose(out, photo, atol=1e-6)
    assert not np.shares_memory(out, photo)  # a new picture, not the input handed back


@pytest.mark.parametrize(
    ("tint", "expected"),
    [(1.0, [0.5, 0.25098, 0.0]), (0.5, [0.5, 0.375490, 0.25]), (0.0, [0.5, 0.5, 0.5])],
    ids=["tint 1: the grey as light of the colour", "tint 0.5: half of each", "tint 0: the grey itself"],
)
def test_the_tint_replaces_the_picture_by_its_brightness_in_the_colour(tint: float, expected: list[float]) -> None:
    """A grey of 0.5 has the brightness 0.5 and as light of '#ff8000' = (1, 0.50196, 0) is (0.5, 0.25098, 0); the tint
    mixes that with the grey 0.5: tint 0.5 gives (0.5, 0.37549, 0.25)."""
    out = holo(flat(0.5), tint=tint, color="#ff8000")
    np.testing.assert_allclose(out, np.broadcast_to(expected, out.shape), atol=1e-5)


def test_a_coloured_picture_is_replaced_by_its_brightness_not_its_hue() -> None:
    """Rec. 709 brightness of (1, 0, 0) is 0.2126 and of (0, 0, 1) is 0.0722; at tint 1 in white light that is all
    three channels."""
    red, blue = flat(0.0), flat(0.0)
    red[..., 0], blue[..., 2] = 1.0, 1.0
    np.testing.assert_allclose(holo(red, tint=1.0, color="#ffffff"), 0.2126, atol=1e-5)
    np.testing.assert_allclose(holo(blue, tint=1.0, color="#ffffff"), 0.0722, atol=1e-5)


def test_a_flat_picture_has_no_edge_glow() -> None:
    np.testing.assert_allclose(holo(flat(0.4), glow=20.0), holo(flat(0.4), glow=0.0), atol=1e-6)


@pytest.mark.parametrize("vertical", [True, False], ids=["a vertical step", "a horizontal step"])
def test_a_step_glows_on_both_sides_and_nowhere_else(vertical: bool) -> None:
    """A step from 0.2 to 0.8 between columns 5 and 6 reads 0.5 x 0.6 = 0.3 on each of the two pixels beside it with no
    smoothing (a slope of 0.3 per px); at glow 1 in white that light is screened over the pixel: 0.2 + 0.3 x (1 - 0.2) =
    0.44 on the dark side and 0.8 + 0.3 x (1 - 0.8) = 0.86 on the bright one."""
    picture = flat(0.2, 12, 12)
    picture[:, 6:] = 0.8
    if not vertical:
        picture = picture.transpose(1, 0, 2)
    out = holo(picture, glow=1.0, color="#ffffff", edge_sigma=0.0)
    out = out if vertical else out.transpose(1, 0, 2)
    expected = np.where(np.arange(12) < 6, 0.2, 0.8).astype(np.float32)
    expected[5], expected[6] = 0.44, 0.86
    np.testing.assert_allclose(out[:, :, 1], np.tile(expected, (12, 1)), atol=1e-5)


def test_a_scanline_darkens_each_row_by_its_distance_from_the_nearest_line() -> None:
    """With spacing 4 the lines are centred on rows 0, 4, 8... and the dark between them is at 2, 6...: row r is
    darkened by `scanlines` x sin^2(pi r / 4) = 0, 0.5, 1, 0.5 of it, so at 0.5 the gains are 1, 0.75, 0.5, 0.75."""
    out = holo(flat(0.8, 8, 5), scanlines=0.5, scanline_spacing=4)
    gains = np.array([1, 0.75, 0.5, 0.75, 1, 0.75, 0.5, 0.75])
    np.testing.assert_allclose(out[:, :, 0], 0.8 * gains[:, None] * np.ones((1, 5)), atol=1e-6)


def test_the_flicker_is_a_band_of_dim_rows_rolling_down_the_picture() -> None:
    """Bands 8 px high, flicker 0.5: a row loses `flicker` x (1 - cos(2 pi r / 8)) / 2 of its light, 0 at row 0 and the
    most, 0.5, at row 4: gains 1, 0.9268, 0.75, 0.5732, 0.5, 0.5732, 0.75, 0.9268."""
    out = holo(flat(0.8, 16, 5), flicker=0.5, flicker_period=8.0, phase=0.0)
    gains = np.array([1, 0.9268, 0.75, 0.5732, 0.5, 0.5732, 0.75, 0.9268])
    np.testing.assert_allclose(out[:8, 0, 0], 0.8 * gains, atol=1e-4)
    np.testing.assert_allclose(out[8:, 0, 0], 0.8 * gains, atol=1e-4)


@pytest.mark.parametrize(
    ("phase", "rows_down"),
    [(0.25, 2), (0.5, 4), (1.0, 0)],
    ids=["a quarter cycle: 2 of 8 rows down", "half a cycle: 4 rows down", "a whole cycle: back where it was"],
)
def test_the_phase_rolls_the_bands_down_by_that_share_of_their_height(phase: float, rows_down: int) -> None:
    flat_grey = flat(0.8, 16, 5)
    still = holo(flat_grey, flicker=0.5, flicker_period=8.0, phase=0.0)
    rolled = holo(flat_grey, flicker=0.5, flicker_period=8.0, phase=phase)
    np.testing.assert_allclose(rolled, np.roll(still, rows_down, axis=0), atol=1e-5)


def test_the_edge_sigma_widens_the_outline_of_a_step() -> None:
    """Unsmoothed, a step lights only its two neighbouring pixels; smoothed with sigma 2 the light reaches pixels 3 px
    from it."""
    picture = flat(0.2, 12, 20)
    picture[:, 10:] = 0.8
    assert holo(picture, glow=5.0, edge_sigma=0.0, color="#ffffff")[4, 7, 0] == pytest.approx(0.2, abs=1e-6)
    assert holo(picture, glow=5.0, edge_sigma=2.0, color="#ffffff")[4, 7, 0] > 0.25
