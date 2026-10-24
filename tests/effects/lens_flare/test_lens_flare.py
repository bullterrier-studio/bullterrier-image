"""Behaviour of the `lens_flare` effect: a halo, a ring and ghosts of one light, drawn on the picture."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "lens_flare"
HEIGHT, WIDTH = 101, 201
DIAGONAL = float(np.hypot(HEIGHT, WIDTH))
LIGHT = (30, 40)  # (row, column) of the light, which is x = 40 / 200 = 0.2 and y = 30 / 100 = 0.3
CENTRE = (50, 100)  # the picture's middle pixel
OFF = {"halo": 0.0, "ring": 0.0, "ghosts": 0}  # one feature at a time: the others switched off


def flare(**settings: object) -> np.ndarray:
    """The flare of the light at LIGHT on a black picture, in white, with `settings` over the defaults."""
    black = np.zeros((HEIGHT, WIDTH, 3), np.float32)
    return registry.apply(NAME, black, **{"x": 0.2, "y": 0.3, "color": "#ffffff", **settings})[..., 0]


def peak_of(values: np.ndarray) -> tuple[int, int]:
    """(row, column) of the largest value of a 2-D array."""
    row, col = np.unravel_index(values.argmax(), values.shape)
    return int(row), int(col)


def distances(row: float, col: float) -> np.ndarray:
    """Distance in px of every pixel of the picture from (`row`, `col`)."""
    rows, cols = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float64)
    return np.hypot(rows - row, cols - col)


def test_the_ghosts_lie_on_the_line_through_the_light_and_the_centre_at_spacing_steps() -> None:
    """Ghost i sits at light + i x spacing x (centre - light): with spacing 0.5 and (20, 60) from the light to the
    centre (rows, columns), at (40, 70), (50, 100) and (60, 130), each at the brightness 1, 2/3, 1/3 of `amount`
    0.9, the nearest pixel being where a ghost's soft disc peaks."""
    out = flare(**{**OFF, "ghosts": 3}, spacing=0.5, size=0.03, amount=0.9)
    for (row, col), peak in zip([(40, 70), (50, 100), (60, 130)], [0.9, 0.6, 0.3], strict=True):
        window = out[row - 3 : row + 4, col - 3 : col + 4]
        assert peak_of(window) == (3, 3)
        assert out[row, col] == pytest.approx(peak, abs=1e-5)


def test_ghosts_light_only_their_own_discs() -> None:
    """Brute force: a pixel farther than `size` x the diagonal from every ghost centre stays black, one at a ghost's
    centre is lit, and no light lands at the light itself (the ghosts are away from it)."""
    size = 0.03
    out = flare(**{**OFF, "ghosts": 3}, spacing=0.5, size=size, amount=1.0)
    nearest = np.minimum.reduce([distances(row, col) for row, col in [(40, 70), (50, 100), (60, 130)]])
    assert (out[nearest > size * DIAGONAL] == 0).all()
    assert (out[nearest < 0.5 * size * DIAGONAL] > 0).all()
    assert out[LIGHT] == 0


@pytest.mark.parametrize(
    ("light", "spacing", "expected"),
    [
        ((0.2, 0.3), 1.0, CENTRE),  # spacing 1: the first ghost is at the picture's centre
        ((0.2, 0.3), 2.0, (70, 160)),  # spacing 2: the light mirrored through the centre: 2 x (50, 100) - (30, 40)
        ((1.0, 0.0), 2.0, (100, 0)),  # the same from the top right corner: 2 x (50, 100) - (0, 200)
        ((0.5, 0.5), 0.7, CENTRE),  # a light at the centre throws its ghost onto itself
        ((0.2, 0.3), 0.0, LIGHT),  # spacing 0: the ghost stays on the light
    ],
    ids=["spacing 1 reaches the centre", "spacing 2 mirrors the light", "a corner light", "a central light", "none"],
)
def test_the_first_ghost_is_where_spacing_puts_it(
    light: tuple[float, float], spacing: float, expected: tuple[int, int]
) -> None:
    out = flare(**{**OFF, "ghosts": 1}, x=light[0], y=light[1], spacing=spacing, size=0.02, amount=1.0)
    assert peak_of(out) == expected
    assert out[expected] == pytest.approx(1.0, abs=1e-5)


def test_the_halo_falls_smoothly_from_the_light_to_nothing_at_its_radius() -> None:
    """Brightness (1 - (d / radius)^2)^2 of `amount`: 1 at the light, 0.5625 half way out, 0 at the radius."""
    radius = 0.2
    out = flare(**{**OFF, "halo": radius}, amount=1.0)
    reach = radius * DIAGONAL
    assert out[LIGHT] == pytest.approx(1.0, abs=1e-5)
    half = round(reach / 2)
    assert out[LIGHT[0], LIGHT[1] + half] == pytest.approx((1 - (half / reach) ** 2) ** 2, abs=1e-5)
    assert (out[distances(*LIGHT) >= reach] == 0).all()
    assert (out[distances(*LIGHT) < 0.9 * reach] > 0).all()


def test_the_ring_is_a_thin_shell_round_the_light() -> None:
    """Brightness (1 - ((d - radius) / width)^2)^2 within `width` of the radius: 1 on it, 0.5625 half a width off,
    nothing at the light itself or beyond the shell."""
    radius, width = 0.15, 0.04
    out = flare(**{**OFF, "ring": radius, "ring_width": width}, amount=1.0)
    ring, wide = radius * DIAGONAL, width * DIAGONAL
    on_it = round(ring)
    assert out[LIGHT[0], LIGHT[1] + on_it] == pytest.approx((1 - ((on_it - ring) / wide) ** 2) ** 2, abs=1e-5)
    assert out[LIGHT[0], LIGHT[1] + on_it] > 0.95
    off = round(ring + wide / 2)
    assert out[LIGHT[0], LIGHT[1] + off] == pytest.approx((1 - ((off - ring) / wide) ** 2) ** 2, abs=1e-5)
    assert out[LIGHT] == 0
    away = np.abs(distances(*LIGHT) - ring)
    assert (out[away >= wide] == 0).all()
    assert (out[away < 0.8 * wide] > 0).all()


def test_color_tints_the_light() -> None:
    black = np.zeros((HEIGHT, WIDTH, 3), np.float32)
    out = registry.apply(NAME, black, **{**OFF, "halo": 0.2}, x=0.2, y=0.3, amount=1.0, color=(1.0, 0.5, 0.0))
    np.testing.assert_allclose(out[LIGHT], [1.0, 0.5, 0.0], atol=1e-5)


@pytest.mark.parametrize(
    "settings",
    [{}, {"ghosts": 12, "spacing": 3.0, "x": 1.0, "y": 1.0, "amount": 10.0}, {"halo": 1.0, "ring": 1.0, "size": 0.3}],
    ids=["defaults", "ghosts far beyond the picture", "everything wide"],
)
def test_the_picture_only_ever_gets_brighter(settings: dict, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, **settings)
    assert (out >= photo - 1e-6).all()
    assert (out > photo + 0.01).any()


def test_light_adds_to_what_is_there_by_screening_it() -> None:
    """A grey of 0.5 under light 0.4 (the halo's centre, amount 0.4) is 1 - (1 - 0.5)(1 - 0.4) = 0.7."""
    grey = np.full((HEIGHT, WIDTH, 3), 0.5, np.float32)
    out = registry.apply(NAME, grey, **{**OFF, "halo": 0.1}, x=0.2, y=0.3, amount=0.4, color="#ffffff")
    assert out[LIGHT][0] == pytest.approx(0.7, abs=1e-5)


def test_a_flare_without_features_changes_nothing(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo, **OFF), photo, atol=1e-6)
