"""Behaviour of the `relight` effect: the picture lit from the shape of its depth."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "relight"
HEIGHT, WIDTH = 96, 128  # the `photo` fixture's size
RISE = 0.0075  # depth gained per px along a test ramp: 0.75 px per px under a relief 100 px tall
INSIDE = (slice(12, -12), slice(12, -12))  # beyond the reach of the edge rule through a blur of 2 px


def ramp(direction: float) -> np.ndarray:
    """A depth of 0.5 at the picture's middle that rises by RISE per px towards `direction` (degrees clockwise from
    up), so it is nearer the camera that way."""
    y, x = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float64)
    a = np.radians(direction)
    return (0.5 + RISE * ((x - WIDTH / 2) * np.sin(a) - (y - HEIGHT / 2) * np.cos(a))).astype(np.float32)


def hand_gain(slope_x: float, slope_y: float, angle: float, elevation: float) -> float:
    """The gain of a surface rising `slope_x` px per px to the right and `slope_y` down, from the geometry: its unit
    normal (-slope_x, -slope_y, 1) / length dotted with the unit vector towards the light (x right, y down, z up),
    over what a flat surface (normal (0, 0, 1)) catches."""
    a, e = np.radians(angle), np.radians(elevation)
    light = np.array([np.cos(e) * np.sin(a), -np.cos(e) * np.cos(a), np.sin(e)])
    normal = np.array([-slope_x, -slope_y, 1.0]) / np.sqrt(1 + slope_x**2 + slope_y**2)
    return float(normal @ light / light[2])


def disc() -> np.ndarray:
    """A depth: a near disc (1) of radius 25 px with a soft rim, in front of a far background (0)."""
    y, x = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float32)
    return np.clip((25 - np.hypot(x - WIDTH / 2, y - HEIGHT / 2)) / 6, 0, 1).astype(np.float32)


def toward(angle: float) -> np.ndarray:
    """The half of the picture on the light's side of the middle, for a light from `angle`."""
    y, x = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float64)
    a = np.radians(angle)
    return np.asarray((x - WIDTH / 2) * np.sin(a) - (y - HEIGHT / 2) * np.cos(a) > 0)


@pytest.mark.parametrize(
    ("depth", "settings"),
    [
        pytest.param(np.full((HEIGHT, WIDTH), 0.4, np.float32), {}, id="a constant depth is flat"),
        pytest.param(np.ones((HEIGHT, WIDTH), np.float32), {"angle": 100, "elevation": 1}, id="flat, grazing light"),
        pytest.param(np.zeros((HEIGHT, WIDTH), np.float32), {"height": 1000, "blur": 0}, id="flat, tallest relief"),
        pytest.param(disc(), {"height": 0}, id="height 0 flattens any depth"),
        pytest.param(ramp(30), {"height": 0, "elevation": 10}, id="height 0, low light on a ramp"),
    ],
)
def test_a_flat_relief_leaves_the_picture_unchanged(
    photo: np.ndarray, depth: np.ndarray, settings: dict[str, float]
) -> None:
    """The photo's own brightness varies: the lighting does not come from it."""
    np.testing.assert_allclose(registry.apply(NAME, photo, depth=depth, **settings), photo, atol=1e-6)


@pytest.mark.parametrize(
    ("direction", "settings", "expected"),
    [
        pytest.param(270, {"angle": 90}, 1.4, id="rising away from a light at the right faces it: (1 + 3/4) / (5/4)"),
        pytest.param(90, {"angle": 90}, 0.2, id="rising towards a light at the right faces away: (1 - 3/4) / (5/4)"),
        pytest.param(180, {"angle": 0}, 1.4, id="nearer downwards, light from above: faces it"),
        pytest.param(90, {"angle": 0}, 0.8, id="light across the slope: only the tilt dims, 1 / (5/4)"),
        pytest.param(90, {"angle": 90, "elevation": 90}, 0.8, id="light straight above: only the tilt dims"),
        pytest.param(
            135, {"angle": 315, "elevation": 30}, hand_gain(0.75 / 2**0.5, 0.75 / 2**0.5, 315, 30), id="diagonal"
        ),
        pytest.param(
            20,
            {"angle": 100, "elevation": 70, "height": 40},
            hand_gain(0.3 * np.sin(np.radians(20)), -0.3 * np.cos(np.radians(20)), 100, 70),
            id="arbitrary angles, a relief 40 px tall: slope 0.3 px per px",
        ),
    ],
)
def test_on_a_depth_ramp_every_channel_is_scaled_by_the_lambert_gain_worked_out_by_hand(
    photo: np.ndarray, direction: float, settings: dict[str, float], expected: float
) -> None:
    """One grey gain on every channel, so the colour ratios are kept."""
    options = {"height": 100.0, "elevation": 45.0, **settings}
    dim = photo * 0.5  # headroom: a gain of 1.4 clips nothing
    out = registry.apply(NAME, dim, depth=ramp(direction), **options)
    np.testing.assert_allclose(out[INSIDE], dim[INSIDE] * expected, atol=2e-5)


@pytest.mark.parametrize("angle", [0.0, 90.0, 200.0, 315.0])
def test_turning_the_light_half_way_round_turns_brightening_into_darkening(angle: float) -> None:
    """The tilt adds towards the light what it takes away from the light: the two gains sum to 2 / sqrt(1 + 3/4^2)."""
    grey = np.full((HEIGHT, WIDTH, 3), 0.5, np.float32)
    depth = ramp(angle + 180)  # rises away from the light: faces it
    facing = registry.apply(NAME, grey, depth=depth, angle=angle, height=100)[INSIDE] / grey[INSIDE]
    behind = registry.apply(NAME, grey, depth=depth, angle=(angle + 180) % 360, height=100)[INSIDE] / grey[INSIDE]
    assert (facing > 1).all()
    assert (behind < 1).all()
    np.testing.assert_allclose(facing + behind, 2 / 1.25, atol=2e-5)


@pytest.mark.parametrize("angle", [0.0, 90.0, 200.0, 315.0])
def test_a_near_disc_is_lit_on_the_side_facing_the_light_and_shadowed_on_the_other(angle: float) -> None:
    grey = np.full((HEIGHT, WIDTH, 3), 0.5, np.float32)
    gain = registry.apply(NAME, grey, depth=disc(), angle=angle, height=20)[..., 0] / 0.5
    lit_side, dark_side = gain[toward(angle)], gain[~toward(angle)]
    assert lit_side.max() > 1.05
    assert dark_side.min() < 0.95
    assert lit_side.mean() > dark_side.mean() + 0.05


def test_blur_smooths_the_depth_so_the_lighting_varies_less() -> None:
    grey = np.full((HEIGHT, WIDTH, 3), 0.5, np.float32)
    noisy = np.random.default_rng(2).random((HEIGHT, WIDTH), dtype=np.float32)
    spread = [float(np.ptp(registry.apply(NAME, grey, depth=noisy, height=4, blur=blur))) for blur in (0, 1, 3)]
    assert spread[0] > spread[1] > spread[2] > 0


def test_a_grazing_light_on_a_tall_relief_clips_the_lit_side_to_white_and_the_far_side_to_black(
    photo: np.ndarray,
) -> None:
    out = registry.apply(NAME, photo, depth=disc(), angle=90, elevation=1, height=1000)
    assert out.max() == 1.0
    assert out.min() == 0.0


def test_without_a_depth_the_picture_is_an_unchanged_copy(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, height=500, elevation=5)
    np.testing.assert_array_equal(out, photo)
    assert out is not photo
