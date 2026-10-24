"""Behaviour of the `snow` effect: soft white flakes in depth layers, screened over the picture."""

import itertools

import numpy as np
import pytest
from scipy import ndimage

from photo_fx.core import registry

NAME = "snow"
SIZE = 160
ONE_FLAKE = {"density": 0.04}  # 0.04 per 1000 px^2 on 160 x 160 px = 1.02 flakes: exactly one


def black(size: int = SIZE) -> np.ndarray:
    return np.zeros((size, size, 3), np.float32)


def light(settings: dict, size: int = SIZE) -> np.ndarray:
    """The light the flakes add to a black picture, (h, w) in 0..1: the flakes are white, so red is the light itself."""
    return registry.apply(NAME, black(size), **settings)[..., 0]


def lone_flake(seed: int, **settings: float) -> np.ndarray:
    """The light of the single flake of the first seed from `seed` on that is not cut off by the picture's edge."""
    for candidate in range(seed, seed + 50):
        flake = light({**ONE_FLAKE, "seed": candidate, **settings})
        if flake[0].max() == 0 == flake[-1].max() == flake[:, 0].max() == flake[:, -1].max():
            return flake
    raise AssertionError("every one of 50 seeds put the flake on the picture's edge")


def radius_of(flake: np.ndarray) -> float:
    """The radius of a lone flake worked out from the area it touches: a disc of radius r touches the pixels within
    r + 0.5 of its centre, area pi (r + 0.5)^2, when its edge is one pixel soft."""
    return float(np.sqrt((flake > 0).sum() / np.pi) - 0.5)


def test_no_density_leaves_the_picture_unchanged(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, density=0.0)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)


def test_more_density_touches_more_of_the_picture_and_keeps_the_flakes_it_had() -> None:
    masks = [light({"density": density, "seed": 4}) > 0 for density in (0.1, 0.3, 1.0, 3.0)]
    fractions = [mask.mean() for mask in masks]
    assert fractions == sorted(fractions)
    assert len(set(fractions)) == len(fractions)  # strictly more each time
    for fewer, more in itertools.pairwise(masks):
        assert not (fewer & ~more).any()  # a flake at a lower density is still there at a higher one


def test_the_number_of_flakes_is_the_density_per_1000_square_pixels() -> None:
    """Small flakes far apart: density 0.1 on 400 x 400 px = 160 000 px^2 is 16 flakes, density 0.3 is 48."""
    for density, flakes in ((0.1, 16), (0.3, 48)):
        mask = light({"density": density, "size": 4.0, "seed": 6}, size=400) > 0
        assert ndimage.label(mask, structure=np.ones((3, 3)))[1] == pytest.approx(flakes, rel=0.15)


def test_each_flake_is_a_disc_in_one_of_the_layers_the_largest_as_wide_as_the_size() -> None:
    """With 4 layers a flake's diameter is 1/4, 2/4, 3/4 or all of `size` (24 px): radii 3, 6, 9 and 12 px."""
    radii = [radius_of(lone_flake(seed, size=24.0, layers=4, softness=0.0)) for seed in range(0, 200, 8)]
    expected = np.array([3.0, 6.0, 9.0, 12.0])
    nearest = expected[np.abs(np.subtract.outer(radii, expected)).argmin(axis=1)]
    np.testing.assert_allclose(radii, nearest, atol=0.35)
    assert set(nearest) == set(expected)  # all four layers occur


def test_a_single_layer_makes_every_flake_as_wide_as_the_size() -> None:
    radii = [radius_of(lone_flake(seed, size=30.0, layers=1, softness=0.0)) for seed in range(8)]
    np.testing.assert_allclose(radii, 15.0, atol=0.35)


def test_far_flakes_are_fainter_by_the_same_depth_that_makes_them_smaller() -> None:
    """The flake at depth d (its radius a share d of `size` / 2) is `opacity` x (0.5 + 0.5 d) at its brightest."""
    for seed in range(0, 120, 8):
        flake = lone_flake(seed, size=24.0, layers=4, softness=0.0, opacity=0.8)
        depth = radius_of(flake) / 12.0
        assert flake.max() == pytest.approx(0.8 * (0.5 + 0.5 * depth), abs=0.03)


@pytest.mark.parametrize("softness", [0.0, 0.5, 1.0])
def test_a_flake_is_half_as_bright_at_its_radius_whatever_its_softness(softness: float) -> None:
    flake = lone_flake(2, size=40.0, layers=1, softness=softness, opacity=1.0)
    rows, cols = np.mgrid[0:SIZE, 0:SIZE]
    centre = ((flake * rows).sum() / flake.sum(), (flake * cols).sum() / flake.sum())
    distance = np.hypot(rows - centre[0], cols - centre[1])
    ring = np.abs(distance - 20.0) < 0.5
    assert flake[ring].mean() == pytest.approx(0.5, abs=0.05)
    assert flake.max() == pytest.approx(1.0, abs=0.001)


def test_a_softer_flake_has_a_wider_ramp_from_full_brightness_to_none() -> None:
    def ramp_pixels(softness: float) -> int:
        flake = lone_flake(2, size=40.0, layers=1, softness=softness, opacity=1.0)
        return int(((flake > 0.05) & (flake < 0.95)).sum())

    widths = [ramp_pixels(softness) for softness in (0.0, 0.5, 1.0)]
    assert widths[0] * 2 < widths[1] < widths[2]


def test_flakes_only_add_light_and_never_darken(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, density=2.0, seed=2)
    assert (out >= photo).all()
    assert (out > photo).any()
    assert registry.apply(NAME, black(), density=1.0, seed=2).max() > 0.5  # and show on a black picture


def test_the_flakes_are_screened_over_the_picture_so_a_bright_picture_stays_within_range() -> None:
    """Screen over mid grey 0.5 with light l gives 1 - (1 - 0.5)(1 - l) = 0.5 + 0.5 l, and over white stays white."""
    on_black = light({"density": 2.0, "seed": 3})
    grey = registry.apply(NAME, np.full((SIZE, SIZE, 3), 0.5, np.float32), density=2.0, seed=3)
    np.testing.assert_allclose(grey[..., 0], 0.5 + 0.5 * on_black, atol=1e-6)
    white = registry.apply(NAME, np.ones((SIZE, SIZE, 3), np.float32), density=2.0, seed=3)
    np.testing.assert_array_equal(white, 1.0)


def test_the_opacity_scales_the_light_of_every_flake() -> None:
    full = registry.apply(NAME, black(), density=1.0, opacity=1.0, seed=1)
    half = registry.apply(NAME, black(), density=1.0, opacity=0.5, seed=1)
    np.testing.assert_allclose(half, full / 2, atol=1e-6)


@pytest.mark.parametrize("shape", [(60, 240), (240, 60)], ids=["wide", "tall"])
def test_the_flakes_are_spread_over_the_whole_of_a_wide_or_a_tall_picture(shape: tuple[int, int]) -> None:
    lit = registry.apply(NAME, np.zeros((*shape, 3), np.float32), density=3.0, seed=1)[..., 0] > 0
    for axis in (0, 1):  # the picture's rows, then its columns: every third of them holds some flakes
        thirds = np.array_split(lit.any(axis=1 - axis), 3)
        assert all(third.any() for third in thirds)


def test_the_same_seed_gives_the_same_snow_and_another_seed_other_snow(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, seed=1), registry.apply(NAME, photo, seed=1))
    assert (registry.apply(NAME, photo, seed=1) != registry.apply(NAME, photo, seed=2)).any()
