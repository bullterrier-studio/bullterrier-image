"""Behaviour of the `halftone` effect."""

import itertools
import math

import numpy as np
import pytest
from scipy import ndimage

from photo_fx.core import registry

NAME = "halftone"
WHITE_ON_BLACK = {"ink": "#ffffff", "paper": "#000000"}


def flat(value: float, size: int = 200) -> np.ndarray:
    return np.full((size, size, 3), value, np.float32)


def half_black_half_white(size: int = 200) -> np.ndarray:
    """Black on the left, white from column size / 2 on."""
    rgb = np.zeros((size, size, 3), np.float32)
    rgb[:, size // 2 :] = 1.0
    return rgb


def test_white_is_nearly_all_dots_and_black_is_blank() -> None:
    assert registry.apply(NAME, flat(1.0, 64), **WHITE_ON_BLACK).mean() > 0.9
    assert registry.apply(NAME, flat(0.0, 64), **WHITE_ON_BLACK).max() == 0.0


@pytest.mark.parametrize(
    ("picture", "ink", "paper"),
    [(0.0, "#ffffff", (0x20, 0x30, 0x40)), (1.0, "#000000", (0xF0, 0xE8, 0xD0))],
    ids=["black on dark paper", "white on light paper"],
)
def test_where_no_ink_is_needed_the_paper_shows_exactly(picture: float, ink: str, paper: tuple[int, int, int]) -> None:
    out = registry.apply(NAME, flat(picture, 64), ink=ink, paper="#{:02x}{:02x}{:02x}".format(*paper))
    np.testing.assert_array_equal(out, np.broadcast_to(np.array(paper, np.float32) / 255, out.shape))


DOT_RADII = (0.1, 0.25, 0.4, 0.5, 1.0, 2.0, 3.0)  # px


def ink_of_one_dot(radius: float, cell: int = 20) -> float:
    """The ink one dot prints on average (px^2, sum of its coverage) on an even tone of dots this many px in radius.

    The tone comes from the screen's documented geometry: a dot's area is its tone times the full-tone circle around
    the square cell (radius cell / sqrt(2)), so pi radius^2 = tone pi cell^2 / 2. The screen is turned a little so the
    dots' centres fall at every position between pixels: a dot centred on a pixel is sampled unevenly (the pixel grid
    misses or doubles its edge), and the average over positions is the dot's true printed area."""
    tone = 2 * (radius / cell) ** 2
    out = registry.apply(NAME, flat(tone, 400), cell=cell, angle=10.0, **WHITE_ON_BLACK)[..., 0]
    return float(out.mean()) * cell**2  # ink per cell


def test_a_dot_prints_about_its_own_area_however_small() -> None:
    """A soft edge one pixel wide (feather = 1) may add at most pi / 12 px^2 to a dot: a linear ramp of width f centred
    on a circle's edge adds pi f^2 / 12 (integrate the ramp over the ring). 1 % more is allowed for the cells the
    picture's border cuts. A tiny dot must not print much more than its area."""
    for radius in DOT_RADII:
        area = math.pi * radius**2
        assert abs(ink_of_one_dot(radius) - area) <= math.pi / 12 + 0.01 * area, radius
    assert ink_of_one_dot(0.1) < 0.1  # its area is 0.03 px^2: a centre pixel at half strength would be 0.5


def test_a_dot_prints_more_ink_the_larger_its_tone() -> None:
    inks = [ink_of_one_dot(radius) for radius in (0.0, 0.05, *DOT_RADII)]
    assert inks[0] == 0.0
    assert all(smaller < larger for smaller, larger in itertools.pairwise(inks))


def test_ink_is_proportional_to_tone_while_the_dots_do_not_touch() -> None:
    """A dot's area grows with the tone it stands for. Until dots touch their neighbours (half the cell's width,
    reached at half tone), the ink per tone is constant: pi / 2 (full tone is the circle around the square cell)."""
    for tone in (0.1, 0.25, 0.45):
        ink = registry.apply(NAME, flat(tone), cell=10, angle=0.0, **WHITE_ON_BLACK).mean()
        assert ink / tone == pytest.approx(math.pi / 2, rel=0.04)


def test_on_a_light_background_dark_tones_get_the_ink() -> None:
    black_ink_on_white = {"ink": "#000000", "paper": "#ffffff"}
    assert registry.apply(NAME, flat(1.0, 64), **black_ink_on_white).min() == 1.0  # white needs no ink
    assert registry.apply(NAME, flat(0.0, 64), **black_ink_on_white).mean() < 0.1  # black is nearly all ink


def test_with_one_ink_every_pixel_is_a_mix_of_the_ink_and_the_paper(photo: np.ndarray) -> None:
    ink, paper = np.array([1.0, 0.5, 0.0], np.float32), np.array([0.0, 0.0, 0.25], np.float32)
    out = registry.apply(NAME, photo, cell=6, ink="#ff8000", paper="#000040")
    share = out[..., 0]  # the red channel goes from paper (0) to ink (1): it is the share of ink
    np.testing.assert_allclose(out, paper + share[..., None] * (ink - paper), atol=2e-3)  # colours as bytes
    assert 0.0 < share.mean() < 1.0


def test_without_an_ink_the_dots_take_the_photo_colour_never_darker() -> None:
    red = np.zeros((64, 64, 3), np.float32)
    red[..., 0] = 0.8
    out = registry.apply(NAME, red, cell=8)
    assert out[..., 1:].max() == 0.0
    assert out[..., 0].max() >= 0.8


@pytest.mark.parametrize(
    ("ink_floor", "ink"),
    [(0.25, [0.8, 0.4, 0.16]), (1.0, [0.2, 0.1, 0.04])],  # strongest channel 0.2: scaled by 1 / 0.25, or not at all
)
def test_ink_floor_caps_how_much_a_dark_dot_colour_is_brightened(ink_floor: float, ink: list[float]) -> None:
    dark = np.full((64, 64, 3), (0.2, 0.1, 0.04), np.float32)
    out = registry.apply(NAME, dark, cell=8, ink_floor=ink_floor)
    np.testing.assert_allclose(out, registry.apply(NAME, dark, cell=8, ink=ink), atol=1e-6)
    assert out[..., 0].max() > ink[0] / 2  # the dots are there, so their colour was compared


def test_dots_repeat_every_cell_px_along_an_upright_screen() -> None:
    out = registry.apply(NAME, flat(0.3), cell=12, angle=0.0, **WHITE_ON_BLACK)[..., 0]
    np.testing.assert_allclose(out[:, 12:], out[:, :-12], atol=1e-4)
    np.testing.assert_allclose(out[12:], out[:-12], atol=1e-4)
    assert np.abs(out[:, 6:] - out[:, :-6]).max() > 0.5  # half a cell along, it is not the same


def test_angle_turns_the_screen_and_a_quarter_turn_gives_the_same_square_grid() -> None:
    def screen(angle: float) -> np.ndarray:
        return registry.apply(NAME, flat(0.3), cell=12, angle=angle, **WHITE_ON_BLACK)[..., 0]

    assert np.abs(screen(30.0) - screen(120.0)).mean() < 1e-3
    assert np.abs(screen(30.0) - screen(0.0)).mean() > 0.1
    assert screen(30.0).mean() == pytest.approx(screen(0.0).mean(), rel=0.03)  # turning moves ink, adds none


@pytest.mark.parametrize("angle", [20.0, -20.0, 65.0])
def test_angle_turns_the_screen_clockwise_on_screen(angle: float) -> None:
    """Rows of dots run along the direction `angle` degrees clockwise from the x axis (y points down): from any dot,
    the next one along a row lies at (cell cos angle, cell sin angle) in (x, y) and not at its mirror image."""
    cell = 20
    out = registry.apply(NAME, flat(0.3), cell=cell, angle=angle, **WHITE_ON_BLACK)[..., 0]
    labels, count = ndimage.label(out > 0.5)
    centres = np.array(ndimage.center_of_mass(out, labels, range(1, count + 1)))  # (y, x) of every dot
    middle = centres[np.argmin(np.hypot(*(centres - 100).T))]
    step = cell * np.array([math.sin(math.radians(angle)), math.cos(math.radians(angle))])  # (y, x)
    mirror = step * [-1, 1]

    def nearest_dot(where: np.ndarray) -> float:
        return float(np.hypot(*(centres - where).T).min())

    assert nearest_dot(middle + step) < 0.5
    assert nearest_dot(middle + mirror) > 3


def test_each_dot_stands_for_the_tone_at_its_centre(photo: np.ndarray) -> None:
    """With no tone blur, a picture and a copy of flat cells (each the colour at its cell's centre) print alike."""
    cell = 8
    centres = np.arange(photo.shape[0]) // cell * cell + cell // 2, np.arange(photo.shape[1]) // cell * cell + cell // 2
    cells = photo[np.ix_(*centres)]
    upright = {"cell": cell, "angle": 0.0, "sigma": 0.0}
    np.testing.assert_array_equal(registry.apply(NAME, photo, **upright), registry.apply(NAME, cells, **upright))


def test_sigma_spreads_tone_into_neighbouring_cells() -> None:
    picture = half_black_half_white()
    band = (slice(20, -20), slice(90, 100))  # the cells just left of the edge, all black

    def ink(**params: float) -> float:
        return float(registry.apply(NAME, picture, cell=10, angle=0.0, **WHITE_ON_BLACK, **params)[band].mean())

    assert ink(sigma=0.0) == 0.0  # each dot sees only the pixel at its centre
    assert ink(sigma=3.0) > 0.05  # each dot sees the tone 3 cells around it


def test_truncate_sets_how_far_the_tone_spreads() -> None:
    picture = half_black_half_white()
    band = (slice(20, -20), slice(60, 70))  # 30 to 40 px left of the edge

    def ink(truncate: float) -> float:
        out = registry.apply(NAME, picture, cell=10, angle=0.0, sigma=3.0, truncate=truncate, **WHITE_ON_BLACK)
        return float(out[band].mean())

    assert ink(1.0) == 0.0  # the blur reaches 1 x 3 cells = 30 px
    assert ink(4.0) > 0.02


def test_mode_and_constant_value_set_what_lies_beyond_the_edge() -> None:
    picture = flat(0.0)
    border = (slice(20, -20), slice(0, 10))

    def ink(**params: object) -> float:
        out = registry.apply(NAME, picture, cell=10, angle=0.0, sigma=2.0, **WHITE_ON_BLACK, **params)
        return float(out[border].mean())

    assert ink(mode="reflect") == 0.0
    assert ink(mode="constant", constant_value=1.0) > 0.05  # white beyond the edge bleeds in
    assert ink(mode="constant", constant_value=0.0) == 0.0


def test_feather_softens_the_dot_edges_without_changing_the_ink() -> None:
    def dots(feather: float) -> np.ndarray:
        return registry.apply(NAME, flat(0.25), cell=20, angle=0.0, feather=feather, **WHITE_ON_BLACK)[..., 0]

    def soft_share(out: np.ndarray) -> float:
        return float(((out > 0.01) & (out < 0.99)).mean())

    assert soft_share(dots(4.0)) > 2 * soft_share(dots(1.0))
    assert dots(4.0).mean() == pytest.approx(dots(1.0).mean(), rel=0.03)
