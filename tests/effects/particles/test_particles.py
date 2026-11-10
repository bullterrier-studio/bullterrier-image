"""The `particles` presets, through the facade: the looks the preset data makes. What the effect draws is tested in the
core (`crates/core/tests/it/particles.rs`)."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "particles"
ONE_PARTICLE = {"density": 0.04}  # 0.04 per 1000 px^2 on 160 x 160 px = 1.02 particles: exactly one
WHITE = {"color": "#ffffff"}
SIZE = 160


def light(settings: dict, size: int = SIZE) -> np.ndarray:
    """The light the particles add to a black picture, (h, w) in 0..1 (white unless `settings` say otherwise, so red is
    the light itself)."""
    return registry.apply(NAME, np.zeros((size, size, 3), np.float32), **{**WHITE, **settings})[..., 0]


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


# --- the looks the presets are ------------------------------------------------------------------------------------


def preset(name: str, **settings) -> dict:
    """The preset's settings with `settings` over them, white and one particle."""
    return {**registry.presets(NAME)[name].params, **WHITE, **ONE_PARTICLE, **settings}


def test_the_embers_preset_makes_sparks_whose_streaks_trail_down_and_to_the_left(lone_particle) -> None:
    spark = lone_particle(light, 3, **preset("embers", length=60.0, size=3.0, softness=0.0))
    axis, _ = axis_and_centre(spark)
    assert axis == pytest.approx(105.0, abs=2.0)


def test_the_dust_preset_makes_round_soft_specks(lone_particle) -> None:
    speck = lone_particle(light, 3, **preset("floating_dust"))
    rows, columns = np.nonzero(speck)
    assert rows.max() - rows.min() == pytest.approx(columns.max() - columns.min(), abs=1)  # no streak
    assert rows.max() - rows.min() > 12  # specks, not dots
    assert ((speck > 0.05 * speck.max()) & (speck < 0.95 * speck.max())).sum() > 0.3 * (speck > 0).sum()  # soft


def test_the_rain_preset_makes_thin_streaks_falling_leaning_to_the_right(lone_particle) -> None:
    streak = lone_particle(light, 3, **preset("rain"))
    axis, _ = axis_and_centre(streak)
    assert axis == pytest.approx(80.0, abs=2.0)
    assert (streak > 0).sum(axis=1).max() <= 4  # thin: a pixel or two across


def test_the_snow_preset_makes_flakes_in_four_layers_the_largest_14_px_wide(lone_particle) -> None:
    radii = [radius_of(lone_particle(light, seed, **preset("snow", softness=0.0))) for seed in range(0, 200, 8)]
    expected = np.array([1.75, 3.5, 5.25, 7.0])
    nearest = expected[np.abs(np.subtract.outer(radii, expected)).argmin(axis=1)]
    np.testing.assert_allclose(radii, nearest, atol=0.5)
    assert set(nearest) == set(expected)


def test_the_stars_preset_makes_small_crisp_dots(lone_particle) -> None:
    star = lone_particle(light, 3, **preset("starfield"))
    rows, columns = np.nonzero(star)
    assert max(rows.max() - rows.min(), columns.max() - columns.min()) <= 3  # 3 px across at most: 4 pixels
    assert star.max() > 0.3
