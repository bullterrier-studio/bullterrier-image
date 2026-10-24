"""Behaviour of the `drop_shadow` effect."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "drop_shadow"
GREY = 0.8


def grey(size: int = 32) -> np.ndarray:
    return np.full((size, size, 3), GREY, np.float32)


def one_pixel(row: int = 10, col: int = 12, size: int = 32) -> np.ndarray:
    matte = np.zeros((size, size), np.float32)
    matte[row, col] = 1.0
    return matte


@pytest.mark.parametrize(
    ("dx", "dy", "lands_at"),
    [
        pytest.param(5, 3, [(13, 17)], id="positive dx is right and positive dy is down"),
        pytest.param(-4, 2, [(12, 8)], id="negative dx is left"),
        pytest.param(0, -6, [(4, 12)], id="negative dy is up"),
        pytest.param(0.5, 3, [(13, 12), (13, 13)], id="half a pixel shares the shadow between two pixels"),
    ],
)
def test_the_shadow_lands_dx_dy_away_and_nowhere_else(dx: float, dy: float, lands_at: list[tuple[int, int]]) -> None:
    """With no blur and a black shadow of full opacity, a shadow pixel is `picture x (1 - coverage)`; the coverage of
    a pixel moved by a whole number of pixels is 1 there, and of one moved by half a pixel is 1/2 on two pixels."""
    out = registry.apply(NAME, grey(), matte=one_pixel(), dx=dx, dy=dy, blur=0.0, opacity=1.0)
    expected = grey()
    for row, col in lands_at:
        expected[row, col] = GREY * (1 - 1 / len(lands_at))
    np.testing.assert_allclose(out, expected, atol=1e-6)


def test_opacity_zero_is_the_identity_and_the_subject_is_never_changed(photo: np.ndarray, matte: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, matte=matte, opacity=0.0), photo)
    for kwargs in ({}, {"dx": 3, "dy": -2, "blur": 5.0}, {"dx": 0, "dy": 0, "color": "#ff0000", "opacity": 1.0}):
        out = registry.apply(NAME, photo, matte=matte, **kwargs)
        np.testing.assert_array_equal(out[matte == 1], photo[matte == 1])


@pytest.mark.parametrize(
    ("kwargs", "expected"),
    [
        pytest.param({"opacity": 0.25}, GREY * 0.75, id="opacity is the share of the picture the shadow replaces"),
        pytest.param({"color": "#ff0000"}, [1.0, 0.0, 0.0], id="a coloured shadow is that colour at full opacity"),
        pytest.param({"color": "#ff0000", "opacity": 0.5}, [0.9, 0.4, 0.4], id="halfway to red"),
    ],
)
def test_the_shadow_is_the_colour_mixed_in_by_opacity(kwargs: dict, expected: float | list[float]) -> None:
    out = registry.apply(NAME, grey(), matte=one_pixel(), dx=3, dy=3, blur=0.0, **({"opacity": 1.0} | kwargs))
    np.testing.assert_allclose(out[13, 15], expected, atol=1e-6)


def test_blur_spreads_the_shadow_as_a_gaussian_of_that_sigma_and_keeps_its_amount() -> None:
    """Over a flat picture a black shadow removes `GREY x coverage`; the coverage of one pixel blurred by sigma is the
    2-D Gaussian, which peaks at 1 / (2 pi sigma^2) and sums to 1."""
    sigma = 2.0
    out = registry.apply(NAME, grey(64), matte=one_pixel(30, 30, 64), dx=6, dy=6, blur=sigma, opacity=1.0)
    removed = GREY - out[..., 0]
    assert removed[36, 36] == pytest.approx(GREY / (2 * np.pi * sigma**2), abs=1e-4)
    assert removed[36, 36 + 2] == pytest.approx(GREY * np.exp(-4 / (2 * sigma**2)) / (2 * np.pi * sigma**2), abs=1e-4)
    assert removed.sum() == pytest.approx(GREY, abs=1e-3)


def test_a_subject_cut_by_the_border_casts_its_shadow_without_a_gap() -> None:
    """The subject fills the two left columns, so it goes on beyond the border: its shadow, moved right, starts
    where the subject ends."""
    matte = np.zeros((8, 8), np.float32)
    matte[:, :2] = 1.0
    out = registry.apply(NAME, grey(8), matte=matte, dx=5, dy=0, blur=0.0, opacity=1.0)
    np.testing.assert_allclose(out[:, 2:7], 0.0, atol=1e-6)
    np.testing.assert_allclose(out[:, 7:], GREY, atol=1e-6)


def test_without_a_subject_there_is_no_shadow(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo), photo)
