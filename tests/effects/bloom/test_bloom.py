"""Behaviour of the `bloom` effect."""

import numpy as np
import pytest

from bullterrier_image import Recipe, fx, render
from bullterrier_image.core import registry

NAME = "bloom"


def flat(value: float, size: int = 32) -> np.ndarray:
    return np.full((size, size, 3), value, np.float32)


def screen(base: float, light: float) -> float:
    """The screen blend mode, from its definition: 1 - (1 - a)(1 - b)."""
    return 1.0 - (1.0 - base) * (1.0 - light)


def test_bloom_only_ever_brightens(photo: np.ndarray) -> None:
    assert (registry.apply(NAME, photo, radius=8.0) >= photo - 1e-6).all()


def test_black_stays_black() -> None:
    np.testing.assert_array_equal(registry.apply(NAME, flat(0.0)), flat(0.0))


def test_a_threshold_above_every_value_leaves_the_picture_unchanged(photo: np.ndarray) -> None:
    dim = photo * 0.5
    np.testing.assert_allclose(registry.apply(NAME, dim, threshold=0.5, amount=10.0), dim, atol=1e-6)


def test_amount_sets_how_much_it_brightens(photo: np.ndarray) -> None:
    added = [float((registry.apply(NAME, photo, amount=a, radius=6.0) - photo).mean()) for a in (0.3, 0.9)]
    assert 0 < added[0] < added[1]


@pytest.mark.parametrize(
    ("octaves", "persistence"), [(1, 0.5), (3, 0.0), (4, 0.5), (6, 1.0)], ids=["one", "widest only", "halving", "equal"]
)
@pytest.mark.parametrize(("value", "amount", "threshold"), [(0.8, 0.6, 0.55), (0.9, 3.0, 0.5)])
def test_an_even_picture_is_screened_with_its_own_highlight_whatever_the_octaves(
    octaves: int, persistence: float, value: float, amount: float, threshold: float
) -> None:
    """Every octave leaves an even picture as it is and the weights add up to 1, so the halo is the highlight:
    (value - threshold) / (1 - threshold)."""
    highlight = (value - threshold) / (1 - threshold)
    expected = screen(value, min(amount * highlight, 1.0))
    out = registry.apply(
        NAME, flat(value), radius=4.0, amount=amount, threshold=threshold, octaves=octaves, persistence=persistence
    )
    np.testing.assert_allclose(out, expected, atol=1e-5)


@pytest.mark.parametrize(
    ("radius", "octaves", "persistence"),
    [
        pytest.param(8.0, 1, 0.5, id="one octave is one Gaussian of the radius"),
        pytest.param(8.0, 3, 0.0, id="persistence 0 keeps the widest octave alone"),
        pytest.param(8.0, 3, 1.0, id="persistence 1 weighs sigma 8, 4 and 2 equally"),
        pytest.param(8.0, 3, 0.5, id="weights 1, 1/2, 1/4 for sigma 8, 4, 2"),
    ],
)
def test_a_single_bright_pixel_spreads_as_the_weighted_sum_of_octave_gaussians(
    radius: float, octaves: int, persistence: float
) -> None:
    """The impulse response, worked out in the test: octave k is a 2-D Gaussian of sigma radius / 2^k weighing
    persistence^k, the weights scaled to add up to 1; screened over black it is amount x that sum."""
    size, centre, amount = 81, 40, 0.9
    picture = flat(0.0, size)
    picture[centre, centre] = 1.0
    y, x = np.mgrid[0:size, 0:size]
    squared = ((x - centre) ** 2 + (y - centre) ** 2).astype(np.float64)
    weights = np.array([persistence**k for k in range(octaves)])
    weights /= weights.sum()
    halo = sum(
        w * np.exp(-squared / (2 * (radius / 2**k) ** 2)) / (2 * np.pi * (radius / 2**k) ** 2)
        for k, w in enumerate(weights)
    )
    expected = amount * halo
    out = registry.apply(
        NAME, picture, radius=radius, amount=amount, threshold=0.5, octaves=octaves, persistence=persistence
    )
    mask = squared > 0  # the bright pixel itself is white
    np.testing.assert_allclose(out[..., 0][mask], expected[mask], atol=2e-6)
    assert out[centre, centre, 0] == 1.0


def test_the_bloom_keeps_the_colour_of_the_light() -> None:
    red_light = flat(0.0, 41)
    red_light[18:23, 18:23, 0] = 1.0
    out = registry.apply(NAME, red_light, radius=4.0, amount=1.0)
    assert out[20, 27, 0] > 0.01  # the red spills beyond the spot
    assert out[..., 1:].max() == 0.0  # and nothing else does


def test_truncate_sets_where_each_octave_is_cut_off() -> None:
    dark = flat(0.0, 81)
    dark[40, 40] = 1.0
    at_six_px = (40, 46)  # 3 sigma out for the widest octave at radius 2
    assert registry.apply(NAME, dark, radius=2.0, octaves=1, threshold=0.5, truncate=4.0)[at_six_px][0] > 0
    assert registry.apply(NAME, dark, radius=2.0, octaves=1, threshold=0.5, truncate=2.0)[at_six_px][0] == 0


def test_edge_sets_what_the_blur_sees_beyond_the_edge() -> None:
    lit_edge = flat(0.0, 41)
    lit_edge[20, 0] = 1.0  # a bright pixel on the left edge
    far_side = (20, 40)  # the right edge of the same row, one pixel away round the back
    wrapped = registry.apply(NAME, lit_edge, radius=3.0, threshold=0.5, edge="wrap")
    reflected = registry.apply(NAME, lit_edge, radius=3.0, threshold=0.5, edge="reflect")
    assert wrapped[far_side][0] > 0.01
    assert reflected[far_side][0] == 0.0


def test_edge_constant_lets_the_bloom_fade_toward_the_edge_and_reflect_does_not() -> None:
    bright = flat(0.8)
    reflected = registry.apply(NAME, bright, radius=4.0, octaves=1, edge="reflect")
    np.testing.assert_allclose(reflected[0, 0], reflected[16, 16], atol=1e-6)  # an even picture stays even
    dark_beyond = registry.apply(NAME, bright, radius=4.0, octaves=1, edge="constant", constant_value=0.0)
    lit_beyond = registry.apply(NAME, bright, radius=4.0, octaves=1, edge="constant", constant_value=1.0)
    assert dark_beyond[0, 0, 0] < dark_beyond[16, 16, 0] - 0.01
    assert lit_beyond[0, 0, 0] > dark_beyond[0, 0, 0]


def test_constant_value_is_the_highlight_assumed_beyond_the_edge() -> None:
    """Beyond the edge the picture is a flat highlight of `constant_value`; far from the edge it does not matter."""
    out_of_range = registry.apply(NAME, flat(0.0, 81), radius=2.0, octaves=1, edge="constant", constant_value=1.0)
    np.testing.assert_allclose(out_of_range[40, 40], 0.0, atol=1e-6)  # 40 px in: 20 sigma from the edge
    assert out_of_range[0, 0, 0] > 0.4  # in the corner, 3/4 of the blur's weight is beyond the edge


def soft_focus(picture: np.ndarray, **values: float) -> np.ndarray:
    size = picture.shape[1]  # the recipe is as wide as the picture, so its px are the picture's px
    return render(picture, Recipe("soft-focus", size=size, steps=[fx.bloom(preset="soft_focus", **values)]))


def test_the_soft_focus_preset_screens_half_a_blurred_copy_of_the_whole_picture() -> None:
    """With nothing to blur (radius 0) the copy is the picture itself: v screened with v / 2 is v + v/2 (1 - v)."""
    dim = flat(0.2)
    np.testing.assert_allclose(soft_focus(dim, radius=0), flat(0.28), atol=1e-6)  # 0.2 + 0.1 x 0.8, by hand


def test_the_soft_focus_preset_blurs_by_its_radius_and_brightens_shadows_too(photo: np.ndarray) -> None:
    out = soft_focus(photo)
    assert (out >= photo - 1e-6).all()
    assert (out[photo < 0.3] > photo[photo < 0.3]).all()  # threshold 0: even the shadows glow
    far, near = soft_focus(photo, radius=2), soft_focus(photo, radius=0)
    assert np.abs(far - near).max() > 1e-3  # the radius is used


def test_the_soft_focus_preset_spreads_one_point_as_a_single_gaussian_of_sigma_12_at_half_strength() -> None:
    """The impulse response worked out here: a lone white pixel on black becomes 0.5 x a 2-D Gaussian of sigma 12."""
    size, centre, sigma = 161, 80, 12.0
    picture = flat(0.0, size)
    picture[centre, centre] = 1.0
    y, x = np.mgrid[0:size, 0:size]
    squared = ((x - centre) ** 2 + (y - centre) ** 2).astype(np.float64)
    expected = 0.5 * np.exp(-squared / (2 * sigma**2)) / (2 * np.pi * sigma**2)
    inside = (squared > 0) & (squared <= 30**2)  # well within the blur's reach of 4 sigma
    np.testing.assert_allclose(soft_focus(picture)[..., 0][inside], expected[inside], atol=2e-6)
