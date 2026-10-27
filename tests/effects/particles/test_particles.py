"""Behaviour of the `particles` effect: dots and streaks of light scattered over the picture."""

import itertools
from functools import partial

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image.core import registry

NAME = "particles"
SIZE = 160
ONE_PARTICLE = {"density": 0.04}  # 0.04 per 1000 px^2 on 160 x 160 px = 1.02 particles: exactly one
WHITE = {"color": "#ffffff"}
CRISP = {"softness": 0.0, "size_falloff": 0.0, "brightness_falloff": 0.0}  # every particle the full size and light


def light(settings: dict, size: int = SIZE) -> np.ndarray:
    """The light the particles add to a black picture, (h, w) in 0..1 (white unless `settings` say otherwise, so red is
    the light itself)."""
    return registry.apply(NAME, np.zeros((size, size, 3), np.float32), **{**WHITE, **settings})[..., 0]


@pytest.fixture
def lone(lone_particle):
    """`lone(seed, **settings)`: the light of one particle, not cut off by the picture's edge."""
    return partial(lone_particle, light, **ONE_PARTICLE)


def radius_of(dot: np.ndarray) -> float:
    """The radius of a lone crisp dot worked out from the area it touches: a disc of radius r touches the pixels within
    r + 0.5 of its centre, area pi (r + 0.5)^2."""
    return float(np.sqrt((dot > 0).sum() / np.pi) - 0.5)


def axis_and_centre(layer: np.ndarray) -> tuple[float, np.ndarray]:
    """The direction of a lone particle's long axis (degrees clockwise from right on screen, 0..180) and its centre of
    light (x, y), from the first and second moments of the light."""
    rows, cols = np.mgrid[0 : layer.shape[0], 0 : layer.shape[1]]
    centre = np.array([(layer * cols).sum(), (layer * rows).sum()]) / layer.sum()
    dx, dy = cols - centre[0], rows - centre[1]
    angle = 0.5 * np.arctan2(2 * (layer * dx * dy).sum(), (layer * (dx * dx - dy * dy)).sum())
    return float(np.degrees(angle) % 180), centre


def clear_of_edges(layer: np.ndarray) -> bool:
    """Whether the picture's four edges are dark: the particle is not cut off."""
    return layer[0].max() == layer[-1].max() == layer[:, 0].max() == layer[:, -1].max() == 0


def peaks_and_areas(settings: dict) -> tuple[np.ndarray, np.ndarray]:
    """Brightest value and area in px of each separate dot of a sparse 400 x 400 picture."""
    sky = light({"density": 0.1, "size": 8.0, "length": 0.0, "seed": 1, **settings}, size=400)
    patches, found = ndimage.label(sky > 0, structure=np.ones((3, 3)))
    index = range(1, found + 1)
    return np.array(ndimage.maximum(sky, patches, index)), np.array(ndimage.sum(sky > 0, patches, index))


def test_more_density_touches_more_of_the_picture_and_keeps_the_particles_it_had() -> None:
    masks = [light({"density": density, "seed": 4}) > 0 for density in (0.1, 0.3, 1.0, 3.0)]
    fractions = [mask.mean() for mask in masks]
    assert fractions == sorted(fractions)
    assert len(set(fractions)) == len(fractions)  # strictly more each time
    for fewer, more in itertools.pairwise(masks):
        assert not (fewer & ~more).any()  # a particle at a lower density is still there at a higher one


def test_no_density_leaves_the_picture_dark() -> None:
    assert light({"density": 0.0}).max() == 0.0


@pytest.mark.parametrize(("density", "particles"), [(0.1, 16), (0.3, 48)], ids=["16 on 160 000 px^2", "48"])
def test_the_number_of_particles_is_the_density_per_1000_square_pixels(density: float, particles: int) -> None:
    mask = light({"density": density, "size": 4.0, "length": 0.0, "softness": 0.0, "seed": 6}, size=400) > 0
    assert ndimage.label(mask, structure=np.ones((3, 3)))[1] == pytest.approx(particles, rel=0.15)


def test_without_length_a_particle_is_a_round_dot(lone) -> None:
    dot = lone(1, length=0.0, size=10.0)
    rows, columns = np.nonzero(dot)
    assert rows.max() - rows.min() == columns.max() - columns.min()


@pytest.mark.parametrize("length", [10.0, 20.0, 40.0])
def test_longer_streaks_cover_more_of_the_picture(length: float) -> None:
    shorter = light({"density": 0.5, "size": 2.0, "length": length, "seed": 1}).sum()
    longer = light({"density": 0.5, "size": 2.0, "length": length * 2, "seed": 1}).sum()
    assert longer > 1.5 * shorter


def test_a_streak_is_as_long_as_the_length_plus_its_width(lone) -> None:
    """A capsule: `length` between the two ends' centres, a round cap `size` / 2 beyond each (down, at angle 90)."""
    streak = lone(2, length=50.0, size=6.0, angle=90.0, **CRISP)
    rows = np.flatnonzero(streak.sum(axis=1) > 0)
    assert len(rows) == pytest.approx(50.0 + 6.0, abs=1.5)


@pytest.mark.parametrize("angle", [0.0, 30.0, 90.0, 135.0, 210.0, 300.0])
def test_a_streak_runs_from_its_head_at_the_angle_clockwise_from_right_on_screen(angle: float) -> None:
    """At 0 it points right, at 90 down, at 180 left, at 270 up. The streak is a capsule, so its centre of light is half
    its length from the head along the angle; the head is the same for one seed whatever the angle."""
    settings = {"length": 40.0, "size": 3.0, **CRISP}
    for seed in range(100):
        layers = {a: light({**ONE_PARTICLE, **settings, "angle": a, "seed": seed}) for a in (angle, 90.0)}
        if all(clear_of_edges(layer) for layer in layers.values()):
            break
    else:
        pytest.fail("every one of 100 seeds put the streak on the picture's edge")
    (_, centre), (_, down_centre) = (axis_and_centre(layers[a]) for a in (angle, 90.0))
    turn = np.radians(angle)
    expected = 20.0 * np.array([np.cos(turn), np.sin(turn)]) - 20.0 * np.array([0.0, 1.0])
    np.testing.assert_allclose(centre - down_centre, expected, atol=0.5)


@pytest.mark.parametrize("angle", [20.0, 80.0, 100.0, 160.0])
def test_a_streak_lies_along_its_angle(angle: float, lone) -> None:
    axis, _ = axis_and_centre(lone(3, length=60.0, size=3.0, angle=angle, **CRISP))
    assert axis == pytest.approx(angle, abs=1.0)


def test_a_size_falloff_makes_far_particles_smaller_without_making_them_fainter() -> None:
    peaks, areas = peaks_and_areas({"size": 12.0, "size_falloff": 0.5, "brightness_falloff": 0.0, "softness": 0.0})
    np.testing.assert_allclose(peaks, 1.0, atol=0.01)
    assert areas.max() > 2 * areas.min()  # the widths run from 6 to 12 px: areas from 1 to 4


def test_a_brightness_falloff_makes_far_particles_fainter_without_making_them_smaller() -> None:
    peaks, areas = peaks_and_areas({"size_falloff": 0.0, "brightness_falloff": 1.0, "softness": 0.0})
    assert peaks.min() < 0.3
    assert peaks.max() > 0.9
    np.testing.assert_allclose(areas, areas.max(), rtol=0.1)  # a full-size dot of 8 px: the same area every time


def test_without_falloff_every_particle_is_alike() -> None:
    peaks, areas = peaks_and_areas({"size_falloff": 0.0, "brightness_falloff": 0.0, "softness": 0.0})
    np.testing.assert_allclose(peaks, 1.0, atol=0.01)
    np.testing.assert_allclose(areas, areas.max(), rtol=0.1)


def test_a_streak_shortens_with_its_width_by_the_size_falloff(lone) -> None:
    """A far particle has the share s of the nearest one's width and length (and its radius): the extent down is
    length x s + size x s, and with the light its peak is s too if the brightness falls off as much."""
    for seed in range(6):
        streak = lone(seed, length=80.0, size=6.0, angle=90.0, size_falloff=0.5, brightness_falloff=0.5, softness=0.0)
        scale = streak.max()  # = the share of the nearest, 0.5 to 1
        rows = np.flatnonzero(streak.sum(axis=1) > 0)
        assert (len(rows) - 1) / (80.0 + 6.0) == pytest.approx(scale, abs=0.03)  # less the pixel of antialiasing


def test_with_n_layers_the_particles_have_n_widths_the_largest_as_wide_as_the_size(lone) -> None:
    """Full size falloff, 4 layers, size 24: diameters 1/4, 2/4, 3/4 and all of 24 px, radii 3, 6, 9 and 12."""
    radii = [radius_of(lone(seed, size=24.0, layers=4, size_falloff=1.0, softness=0.0)) for seed in range(0, 200, 8)]
    expected = np.array([3.0, 6.0, 9.0, 12.0])
    nearest = expected[np.abs(np.subtract.outer(radii, expected)).argmin(axis=1)]
    np.testing.assert_allclose(radii, nearest, atol=0.35)
    assert set(nearest) == set(expected)  # all four layers occur


def test_one_layer_makes_every_particle_as_wide_as_the_size(lone) -> None:
    radii = [radius_of(lone(seed, size=30.0, layers=1, size_falloff=1.0, softness=0.0)) for seed in range(8)]
    np.testing.assert_allclose(radii, 15.0, atol=0.35)


def test_without_layers_the_widths_run_through_all_values(lone) -> None:
    radii = {round(radius_of(lone(seed, size=24.0, size_falloff=1.0, softness=0.0))) for seed in range(0, 200, 8)}
    assert len(radii) > 6  # 4 layers would give 4


def test_a_far_layer_is_fainter_by_the_brightness_falloff_of_its_nearness(lone) -> None:
    """In layer k of n (nearness d = k / n; its radius a share d of `size` / 2 at full size falloff) a particle's peak
    is 1 - brightness_falloff x (1 - d)."""
    for seed in range(0, 120, 8):
        flake = lone(seed, size=24.0, layers=4, size_falloff=1.0, brightness_falloff=0.5, softness=0.0)
        nearness = radius_of(flake) / 12.0
        assert flake.max() == pytest.approx(0.5 + 0.5 * nearness, abs=0.03)


@pytest.mark.parametrize("softness", [0.0, 0.5, 1.0])
def test_a_dot_is_half_as_bright_at_its_radius_whatever_its_softness(softness: float, lone) -> None:
    dot = lone(2, size=40.0, length=0.0, size_falloff=0.0, brightness_falloff=0.0, softness=softness)
    rows, cols = np.mgrid[0:SIZE, 0:SIZE]
    centre = ((dot * rows).sum() / dot.sum(), (dot * cols).sum() / dot.sum())
    ring = np.abs(np.hypot(rows - centre[0], cols - centre[1]) - 20.0) < 0.5
    assert dot[ring].mean() == pytest.approx(0.5, abs=0.05)
    assert dot.max() == pytest.approx(1.0, abs=0.001)


def test_a_softer_particle_has_a_wider_ramp_from_full_brightness_to_none(lone) -> None:
    def ramp_pixels(softness: float) -> int:
        dot = lone(2, size=40.0, length=0.0, size_falloff=0.0, brightness_falloff=0.0, softness=softness)
        return int(((dot > 0.05) & (dot < 0.95)).sum())

    widths = [ramp_pixels(softness) for softness in (0.0, 0.5, 1.0)]
    assert widths[0] * 2 < widths[1] < widths[2]


def test_a_bigger_size_makes_bigger_dots(lone) -> None:
    small, large = (lone(1, size=size, length=0.0, softness=0.0) for size in (12.0, 36.0))
    assert (large > 0).sum() > 6 * (small > 0).sum()  # discs: area grows with the square of the size, here 9 x


def test_no_particle_is_brighter_than_its_colour_even_where_particles_cross() -> None:
    """The nearest ones screen their full `color` (grey 0x80 here); where two cross the brighter shows."""
    dense = light({"density": 50.0, "color": "#808080", "size_falloff": 0.0, "brightness_falloff": 0.0, "seed": 1})
    assert dense.max() == pytest.approx(128 / 255, abs=0.01)


# --- the looks the presets are ------------------------------------------------------------------------------------


def preset(name: str, **settings) -> dict:
    """The preset's settings with `settings` over them, white and one particle."""
    return {**registry.presets(NAME)[name].params, **WHITE, **ONE_PARTICLE, **settings}


def test_the_embers_preset_makes_sparks_whose_streaks_trail_down_and_to_the_left(lone_particle) -> None:
    spark = lone_particle(light, 3, **preset("embers", length=60.0, size=3.0, softness=0.0))
    axis, _ = axis_and_centre(spark)
    assert axis == pytest.approx(105.0, abs=2.0)


def test_the_dust_preset_makes_round_soft_specks(lone_particle) -> None:
    speck = lone_particle(light, 3, **preset("dust"))
    rows, columns = np.nonzero(speck)
    assert rows.max() - rows.min() == pytest.approx(columns.max() - columns.min(), abs=1)  # no streak
    assert rows.max() - rows.min() > 12  # specks, not dots
    assert ((speck > 0.05 * speck.max()) & (speck < 0.95 * speck.max())).sum() > 0.3 * (speck > 0).sum()  # soft


def test_the_rain_preset_makes_thin_streaks_falling_leaning_to_the_right(lone_particle) -> None:
    streak = lone_particle(light, 3, **preset("rain"))
    axis, _ = axis_and_centre(streak)
    assert axis == pytest.approx(80.0, abs=2.0)
    assert (streak > 0).sum(axis=1).max() <= 4  # thin: a pixel or two across
