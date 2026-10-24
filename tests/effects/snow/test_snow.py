"""Behaviour of the `snow` effect: soft white flakes in depth layers, screened over the picture."""

import itertools
from functools import partial

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image.core import registry

NAME = "snow"
SIZE = 160
ONE_FLAKE = {"density": 0.04}  # 0.04 per 1000 px^2 on 160 x 160 px = 1.02 flakes: exactly one


def light(settings: dict, size: int = SIZE) -> np.ndarray:
    """The light the flakes add to a black picture, (h, w) in 0..1: the flakes are white, so red is the light itself."""
    return registry.apply(NAME, np.zeros((size, size, 3), np.float32), **settings)[..., 0]


@pytest.fixture
def lone_flake(lone_particle):
    """`lone_flake(seed, **settings)`: the light of one flake, not cut off by the picture's edge."""
    return partial(lone_particle, light, **ONE_FLAKE)


def radius_of(flake: np.ndarray) -> float:
    """The radius of a lone flake worked out from the area it touches: a disc of radius r touches the pixels within
    r + 0.5 of its centre, area pi (r + 0.5)^2, when its edge is one pixel soft."""
    return float(np.sqrt((flake > 0).sum() / np.pi) - 0.5)


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


def test_each_flake_is_a_disc_in_one_of_the_layers_the_largest_as_wide_as_the_size(lone_flake) -> None:
    """With 4 layers a flake's diameter is 1/4, 2/4, 3/4 or all of `size` (24 px): radii 3, 6, 9 and 12 px."""
    radii = [radius_of(lone_flake(seed, size=24.0, layers=4, softness=0.0)) for seed in range(0, 200, 8)]
    expected = np.array([3.0, 6.0, 9.0, 12.0])
    nearest = expected[np.abs(np.subtract.outer(radii, expected)).argmin(axis=1)]
    np.testing.assert_allclose(radii, nearest, atol=0.35)
    assert set(nearest) == set(expected)  # all four layers occur


def test_a_single_layer_makes_every_flake_as_wide_as_the_size(lone_flake) -> None:
    radii = [radius_of(lone_flake(seed, size=30.0, layers=1, softness=0.0)) for seed in range(8)]
    np.testing.assert_allclose(radii, 15.0, atol=0.35)


def test_far_flakes_are_fainter_by_the_same_depth_that_makes_them_smaller(lone_flake) -> None:
    """The flake at depth d (its radius a share d of `size` / 2) is 0.5 + 0.5 d at its brightest."""
    for seed in range(0, 120, 8):
        flake = lone_flake(seed, size=24.0, layers=4, softness=0.0)
        depth = radius_of(flake) / 12.0
        assert flake.max() == pytest.approx(0.5 + 0.5 * depth, abs=0.03)


@pytest.mark.parametrize("softness", [0.0, 0.5, 1.0])
def test_a_flake_is_half_as_bright_at_its_radius_whatever_its_softness(softness: float, lone_flake) -> None:
    flake = lone_flake(2, size=40.0, layers=1, softness=softness)
    rows, cols = np.mgrid[0:SIZE, 0:SIZE]
    centre = ((flake * rows).sum() / flake.sum(), (flake * cols).sum() / flake.sum())
    distance = np.hypot(rows - centre[0], cols - centre[1])
    ring = np.abs(distance - 20.0) < 0.5
    assert flake[ring].mean() == pytest.approx(0.5, abs=0.05)
    assert flake.max() == pytest.approx(1.0, abs=0.001)


def test_a_softer_flake_has_a_wider_ramp_from_full_brightness_to_none(lone_flake) -> None:
    def ramp_pixels(softness: float) -> int:
        flake = lone_flake(2, size=40.0, layers=1, softness=softness)
        return int(((flake > 0.05) & (flake < 0.95)).sum())

    widths = [ramp_pixels(softness) for softness in (0.0, 0.5, 1.0)]
    assert widths[0] * 2 < widths[1] < widths[2]
