"""Behaviour of the `halftone` effect."""

import math

import numpy as np
import pytest

from photo_fx import effects

NAME = "halftone"
WHITE_ON_BLACK = {"fg": "#ffffff", "bg": "#000000"}
BLANK = 0.01  # the most ink black may show: a dot's soft edge leaves a faint pixel at a zero-tone dot's centre


def flat(value: float, size: int = 200) -> np.ndarray:
    return np.full((size, size, 3), value, np.float32)


def half_black_half_white(size: int = 200) -> np.ndarray:
    """Black on the left, white from column size / 2 on."""
    rgb = np.zeros((size, size, 3), np.float32)
    rgb[:, size // 2 :] = 1.0
    return rgb


def test_white_is_nearly_all_dots_and_black_is_nearly_blank() -> None:
    """(Nearly: a dot's soft edge leaves a faint pixel at the centre of a zero-tone dot.)"""
    assert effects.apply(NAME, flat(1.0, 64), **WHITE_ON_BLACK).mean() > 0.9
    assert effects.apply(NAME, flat(0.0, 64), **WHITE_ON_BLACK).mean() < BLANK


def test_ink_is_proportional_to_tone_while_the_dots_do_not_touch() -> None:
    """A dot's area grows with the tone it stands for. Until dots touch their neighbours (half the cell's width,
    reached at half tone), the ink per tone is constant: pi / 2 (full tone is the circle around the square cell)."""
    for tone in (0.1, 0.25, 0.45):
        ink = effects.apply(NAME, flat(tone), cell=10, angle=0.0, **WHITE_ON_BLACK).mean()
        assert ink / tone == pytest.approx(math.pi / 2, rel=0.04)


def test_on_a_light_background_dark_tones_get_the_ink() -> None:
    black_ink_on_white = {"fg": "#000000", "bg": "#ffffff"}
    assert effects.apply(NAME, flat(1.0, 64), **black_ink_on_white).mean() > 1 - BLANK  # white needs (nearly) no ink
    assert effects.apply(NAME, flat(0.0, 64), **black_ink_on_white).mean() < 0.1  # black is nearly all ink


def test_with_fg_every_pixel_is_a_mix_of_the_ink_and_the_paper(photo: np.ndarray) -> None:
    ink, paper = np.array([1.0, 0.5, 0.0], np.float32), np.array([0.0, 0.0, 0.25], np.float32)
    out = effects.apply(NAME, photo, cell=6, fg="#ff8000", bg="#000040")
    share = out[..., 0]  # the red channel goes from paper (0) to ink (1): it is the share of ink
    np.testing.assert_allclose(out, paper + share[..., None] * (ink - paper), atol=2e-3)  # colours as bytes
    assert 0.0 < share.mean() < 1.0


def test_without_fg_the_dots_take_the_photo_colour_never_darker() -> None:
    red = np.zeros((64, 64, 3), np.float32)
    red[..., 0] = 0.8
    out = effects.apply(NAME, red, cell=8)
    assert out[..., 1:].max() == 0.0
    assert out[..., 0].max() >= 0.8


def test_dots_repeat_every_cell_px_along_an_upright_screen() -> None:
    out = effects.apply(NAME, flat(0.3), cell=12, angle=0.0, **WHITE_ON_BLACK)[..., 0]
    np.testing.assert_allclose(out[:, 12:], out[:, :-12], atol=1e-4)
    np.testing.assert_allclose(out[12:], out[:-12], atol=1e-4)
    assert np.abs(out[:, 6:] - out[:, :-6]).max() > 0.5  # half a cell along, it is not the same


def test_angle_turns_the_screen_and_a_quarter_turn_gives_the_same_square_grid() -> None:
    def screen(angle: float) -> np.ndarray:
        return effects.apply(NAME, flat(0.3), cell=12, angle=angle, **WHITE_ON_BLACK)[..., 0]

    assert np.abs(screen(30.0) - screen(120.0)).mean() < 1e-3
    assert np.abs(screen(30.0) - screen(0.0)).mean() > 0.1
    assert screen(30.0).mean() == pytest.approx(screen(0.0).mean(), rel=0.03)  # turning moves ink, adds none


def test_each_dot_stands_for_the_tone_at_its_centre(photo: np.ndarray) -> None:
    """With no tone blur, a picture and a copy of flat cells (each the colour at its cell's centre) print alike."""
    cell = 8
    centres = np.arange(photo.shape[0]) // cell * cell + cell // 2, np.arange(photo.shape[1]) // cell * cell + cell // 2
    cells = photo[np.ix_(*centres)]
    upright = {"cell": cell, "angle": 0.0, "sigma": 0.0}
    np.testing.assert_array_equal(effects.apply(NAME, photo, **upright), effects.apply(NAME, cells, **upright))


def test_sigma_spreads_tone_into_neighbouring_cells() -> None:
    picture = half_black_half_white()
    band = (slice(20, -20), slice(90, 100))  # the cells just left of the edge, all black

    def ink(**params: float) -> float:
        return float(effects.apply(NAME, picture, cell=10, angle=0.0, **WHITE_ON_BLACK, **params)[band].mean())

    assert ink(sigma=0.0) < BLANK  # each dot sees only the pixel at its centre
    assert ink(sigma=3.0) > 0.05  # each dot sees the tone 3 cells around it


def test_truncate_sets_how_far_the_tone_spreads() -> None:
    picture = half_black_half_white()
    band = (slice(20, -20), slice(60, 70))  # 30 to 40 px left of the edge

    def ink(truncate: float) -> float:
        out = effects.apply(NAME, picture, cell=10, angle=0.0, sigma=3.0, truncate=truncate, **WHITE_ON_BLACK)
        return float(out[band].mean())

    assert ink(1.0) < BLANK  # the blur reaches 1 x 3 cells = 30 px
    assert ink(4.0) > 0.02


def test_mode_and_constant_value_set_what_lies_beyond_the_edge() -> None:
    picture = flat(0.0)
    border = (slice(20, -20), slice(0, 10))

    def ink(**params: object) -> float:
        out = effects.apply(NAME, picture, cell=10, angle=0.0, sigma=2.0, **WHITE_ON_BLACK, **params)
        return float(out[border].mean())

    assert ink(mode="reflect") < BLANK
    assert ink(mode="constant", constant_value=1.0) > 0.05  # white beyond the edge bleeds in
    assert ink(mode="constant", constant_value=0.0) < BLANK


def test_feather_softens_the_dot_edges_without_changing_the_ink() -> None:
    def dots(feather: float) -> np.ndarray:
        return effects.apply(NAME, flat(0.25), cell=20, angle=0.0, feather=feather, **WHITE_ON_BLACK)[..., 0]

    def soft_share(out: np.ndarray) -> float:
        return float(((out > 0.01) & (out < 0.99)).mean())

    assert soft_share(dots(4.0)) > 2 * soft_share(dots(1.0))
    assert dots(4.0).mean() == pytest.approx(dots(1.0).mean(), rel=0.03)


def test_halftone_is_stylised() -> None:
    assert effects.category(NAME) == "stylised"
