"""Behaviour of the `relight` effect: the picture lit from the shape of a field (its depth, its brightness, ...)."""

import numpy as np
import pytest

from bullterrier_image.core import registry, runner
from bullterrier_image.core.steps import Recipe, Step

NAME = "relight"
HEIGHT, WIDTH = 96, 128  # the `photo` fixture's size
RISE = 0.005  # field gained per px along a test ramp: 0.75 px per px under a relief 150 px tall
INSIDE = (slice(12, -12), slice(12, -12))  # beyond the reach of the edge rule through a blur of 2 px


def ramp(direction: float) -> np.ndarray:
    """A field of 0.5 at the picture's middle that rises by RISE per px towards `direction` (degrees clockwise from
    pointing right, on screen)."""
    y, x = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float64)
    a = np.radians(direction)
    return (0.5 + RISE * ((x - WIDTH / 2) * np.cos(a) + (y - HEIGHT / 2) * np.sin(a))).astype(np.float32)


def hand_gain(slope_x: float, slope_y: float, angle: float, elevation: float) -> float:
    """The gain of a surface rising `slope_x` px per px to the right and `slope_y` down, from the geometry: its unit
    normal (-slope_x, -slope_y, 1) / length dotted with the unit vector towards the light (x right, y down, z up),
    over what a flat surface (normal (0, 0, 1)) catches."""
    a, e = np.radians(angle), np.radians(elevation)
    light = np.array([np.cos(e) * np.cos(a), np.cos(e) * np.sin(a), np.sin(e)])
    normal = np.array([-slope_x, -slope_y, 1.0]) / np.sqrt(1 + slope_x**2 + slope_y**2)
    return float(normal @ light / light[2])


def disc() -> np.ndarray:
    """A field: a high disc (1) of radius 25 px with a soft rim, on a low background (0)."""
    y, x = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float32)
    return np.clip((25 - np.hypot(x - WIDTH / 2, y - HEIGHT / 2)) / 6, 0, 1).astype(np.float32)


def toward(angle: float) -> np.ndarray:
    """The half of the picture on the light's side of the middle, for a light from `angle`."""
    y, x = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float64)
    a = np.radians(angle)
    return np.asarray((x - WIDTH / 2) * np.cos(a) + (y - HEIGHT / 2) * np.sin(a) > 0)


@pytest.mark.parametrize(
    ("field", "settings"),
    [
        pytest.param(np.full((HEIGHT, WIDTH), 0.4, np.float32), {}, id="a constant field is flat"),
        pytest.param(np.ones((HEIGHT, WIDTH), np.float32), {"angle": 10, "elevation": 1}, id="flat, grazing light"),
        pytest.param(np.zeros((HEIGHT, WIDTH), np.float32), {"height": 1000, "blur": 0}, id="flat, tallest relief"),
        pytest.param(disc(), {"height": 0}, id="height 0 flattens any field"),
        pytest.param(ramp(30), {"height": 0, "elevation": 10}, id="height 0, low light on a ramp"),
    ],
)
def test_a_flat_relief_leaves_the_picture_unchanged(
    photo: np.ndarray, field: np.ndarray, settings: dict[str, float]
) -> None:
    """The photo's own brightness varies: with the field given, the lighting does not come from it."""
    np.testing.assert_allclose(registry.apply(NAME, photo, field=field, **settings), photo, atol=1e-6)


@pytest.mark.parametrize(
    ("direction", "settings", "expected"),
    [
        pytest.param(180, {"angle": 0}, 1.4, id="rising away from a light at the right faces it: (1 + 3/4) / (5/4)"),
        pytest.param(0, {"angle": 0}, 0.2, id="rising towards a light at the right faces away: (1 - 3/4) / (5/4)"),
        pytest.param(90, {"angle": 270}, 1.4, id="nearer downwards, light from above: faces it"),
        pytest.param(0, {"angle": 270}, 0.8, id="light across the slope: only the tilt dims, 1 / (5/4)"),
        pytest.param(0, {"angle": 0, "elevation": 90}, 0.8, id="light straight above: only the tilt dims"),
        pytest.param(
            45, {"angle": 225, "elevation": 30}, hand_gain(0.75 / 2**0.5, 0.75 / 2**0.5, 225, 30), id="diagonal"
        ),
        pytest.param(
            -70,
            {"angle": 10, "elevation": 70, "height": 60},
            hand_gain(0.3 * np.cos(np.radians(-70)), 0.3 * np.sin(np.radians(-70)), 10, 70),
            id="arbitrary angles, a relief 60 px tall: slope 0.3 px per px",
        ),
    ],
)
def test_on_a_ramp_every_channel_is_scaled_by_the_lambert_gain_worked_out_by_hand(
    photo: np.ndarray, direction: float, settings: dict[str, float], expected: float
) -> None:
    """One grey gain on every channel, so the colour ratios are kept."""
    options = {"height": 150.0, "elevation": 45.0, **settings}
    dim = photo * 0.5  # headroom: a gain of 1.4 clips nothing
    out = registry.apply(NAME, dim, field=ramp(direction), **options)
    np.testing.assert_allclose(out[INSIDE], dim[INSIDE] * expected, atol=2e-5)


@pytest.mark.parametrize("angle", [0.0, 90.0, 200.0, 315.0])
def test_turning_the_light_half_way_round_turns_brightening_into_darkening(angle: float) -> None:
    """The tilt adds towards the light what it takes away from the light: the two gains sum to 2 / sqrt(1 + 3/4^2)."""
    grey = np.full((HEIGHT, WIDTH, 3), 0.5, np.float32)
    field = ramp(angle + 180)  # rises away from the light: faces it
    facing = registry.apply(NAME, grey, field=field, angle=angle, height=150)[INSIDE] / grey[INSIDE]
    behind = registry.apply(NAME, grey, field=field, angle=(angle + 180) % 360, height=150)[INSIDE] / grey[INSIDE]
    assert (facing > 1).all()
    assert (behind < 1).all()
    np.testing.assert_allclose(facing + behind, 2 / 1.25, atol=2e-5)


@pytest.mark.parametrize("angle", [0.0, 90.0, 200.0, 315.0])
def test_a_high_disc_is_lit_on_the_side_facing_the_light_and_shadowed_on_the_other(angle: float) -> None:
    grey = np.full((HEIGHT, WIDTH, 3), 0.5, np.float32)
    gain = registry.apply(NAME, grey, field=disc(), angle=angle, height=20)[..., 0] / 0.5
    lit_side, dark_side = gain[toward(angle)], gain[~toward(angle)]
    assert lit_side.max() > 1.05
    assert dark_side.min() < 0.95
    assert lit_side.mean() > dark_side.mean() + 0.05


def test_blur_smooths_the_field_so_the_lighting_varies_less() -> None:
    grey = np.full((HEIGHT, WIDTH, 3), 0.5, np.float32)
    noisy = np.random.default_rng(2).random((HEIGHT, WIDTH), dtype=np.float32)
    spread = [float(np.ptp(registry.apply(NAME, grey, field=noisy, height=4, blur=blur))) for blur in (0, 1, 3)]
    assert spread[0] > spread[1] > spread[2] > 0


@pytest.mark.parametrize(
    ("edge", "first_column_rise"),
    [
        pytest.param("reflect", RISE / 2, id="reflect repeats the edge pixel: half the slope"),
        pytest.param("nearest", RISE / 2, id="nearest repeats the edge pixel: half the slope"),
        pytest.param("mirror", 0.0, id="mirror reflects about the edge pixel: no slope"),
        pytest.param("wrap", -RISE * (WIDTH - 2) / 2, id="wrap sees the far side: a steep drop"),
    ],
)
def test_edge_decides_the_slope_in_the_first_column_and_nowhere_else(edge: str, first_column_rise: float) -> None:
    grey = np.full((HEIGHT, WIDTH, 3), 0.5, np.float32)
    height = 2  # gentle enough that even the wrapped column is not clipped to black
    out = registry.apply(NAME, grey, field=ramp(0), angle=180, height=height, blur=0, edge=edge)
    for column, rise in [(0, first_column_rise), (1, RISE), (WIDTH // 2, RISE)]:
        gain = hand_gain(height * rise, 0, 180, 45)  # light from the left: rising right is away from it
        np.testing.assert_allclose(out[:, column], 0.5 * gain, atol=2e-5)


def test_truncate_sets_how_far_into_the_picture_the_edge_rule_reaches_through_the_blur() -> None:
    """A blur of sigma 2 cut at `truncate` sigmas reads that many x 2 px beyond the border (plus 1 for the slope)."""
    grey = np.full((HEIGHT, WIDTH, 3), 0.5, np.float32)
    columns = {}
    for truncate in (1.0, 4.0):
        reflect = registry.apply(NAME, grey, field=ramp(0), height=20, blur=2, truncate=truncate, edge="reflect")
        wrap = registry.apply(NAME, grey, field=ramp(0), height=20, blur=2, truncate=truncate, edge="wrap")
        differing = np.flatnonzero(np.abs(reflect - wrap).max(axis=(0, 2)) > 1e-6)
        columns[truncate] = differing[differing < WIDTH // 2]  # the left border's reach; the right one mirrors it
    assert columns[1.0].max() <= 3  # the blur reaches 2 px, the slope 1 more
    assert columns[4.0].max() >= 6  # a blur cut at 4 sigma reaches 8 px, its tail beyond 2 sigma small but not nil


def test_a_grazing_light_on_a_tall_relief_clips_the_lit_side_to_white_and_the_far_side_to_black(
    photo: np.ndarray,
) -> None:
    out = registry.apply(NAME, photo, field=disc(), angle=0, elevation=1, height=1000)
    assert out.max() == 1.0
    assert out.min() == 0.0


@pytest.mark.parametrize("saturation", [0.0, 0.25, 0.5, 1.0])
def test_saturation_mixes_the_pictures_colours_into_a_mid_grey_that_the_relief_lights(
    photo: np.ndarray, saturation: float
) -> None:
    """The surface lit is `saturation` of the picture over the rest of mid grey: gain x (s x picture + (1 - s) / 2)."""
    dim = photo * 0.5  # headroom: the gain of 1.4 clips nothing
    out = registry.apply(NAME, dim, field=ramp(180), angle=0, height=150, saturation=saturation)
    surface = saturation * dim + (1 - saturation) * 0.5
    np.testing.assert_allclose(out[INSIDE], surface[INSIDE] * 1.4, atol=2e-5)


def test_saturation_0_is_the_relief_alone_so_the_picture_does_not_show(photo: np.ndarray) -> None:
    field = disc()
    out = registry.apply(NAME, photo, field=field, saturation=0)
    other = registry.apply(NAME, photo[::-1, ::-1], field=field, saturation=0)
    np.testing.assert_array_equal(out, other)
    np.testing.assert_array_equal(out[..., 0], out[..., 1])  # grey
    flat = registry.apply(NAME, photo, field=np.zeros((HEIGHT, WIDTH), np.float32), saturation=0)
    np.testing.assert_allclose(flat, 0.5, atol=1e-6)  # nothing to light: mid grey


def test_a_grey_relief_from_a_grazing_light_spans_black_to_white(photo: np.ndarray) -> None:
    """The emboss look: the picture's brightness as the field, no colour, a low light."""
    recipe = Recipe(
        "emboss",
        [Step(NAME, {"field": "light", "saturation": 0, "elevation": 5, "height": 20, "blur": 0})],
        size=max(photo.shape[:2]),
    )
    out = runner.render(photo, recipe)
    np.testing.assert_array_equal(out[..., 0], out[..., 2])
    assert out.min() < 0.1
    assert out.max() > 0.9


def test_the_light_field_lights_the_pictures_own_brightness_as_a_relief() -> None:
    """A grey picture rising 0.005 per px to the right, lit from the left (0.005 x 100 px = a slope of 1/2)."""
    x = np.arange(40, dtype=np.float64)
    picture = np.broadcast_to((0.2 + 0.005 * x)[None, :, None], (40, 40, 3)).astype(np.float32)
    recipe = Recipe("lit", [Step(NAME, {"field": "light", "angle": 180, "height": 100, "blur": 0})], size=40)
    out = runner.render(picture, recipe)
    gain = hand_gain(0.5, 0, 180, 45)
    np.testing.assert_allclose(out[:, 2:-2], picture[:, 2:-2] * gain, atol=2e-4)
