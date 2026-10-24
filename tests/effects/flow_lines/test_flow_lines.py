"""Behaviour of the `flow_lines` effect: thin streamlines through a smooth noise flow field, in place of the picture."""

import numpy as np
import pytest

from photo_fx.core import registry
from photo_fx.ops.noise import fractal

NAME = "flow_lines"
LINE, BACKGROUND = np.array([1.0, 128 / 255, 0.0]), np.array([0.0, 0.0, 64 / 255])


def render(shape: tuple[int, int] = (20, 50), **settings: object) -> np.ndarray:
    """Lines over a picture of `shape` (h, w), orange on dark blue; `settings` override."""
    settings = {"line": "#ff8000", "background": "#000040", "seed": 0, **settings}
    return registry.apply(NAME, np.zeros((*shape, 3), np.float32), **settings)


def coverage(out: np.ndarray) -> np.ndarray:
    """(h, w) 0..1: how much each pixel is line colour rather than background (read off the red channel)."""
    return out[..., 0] / LINE[0]


def reference_coverage(
    shape: tuple[int, int], *, density: float, seed: int, flow: dict, **drawing: float
) -> np.ndarray:
    """Brute force, one line and one point at a time. The lines start at default_rng(seed).random((lines, 2)) as
    fractions of the width and height, lines = round(density x pixels). A line stamps a soft disc of diameter
    `thickness` at its start and at each of the next `length` - 1 points, `step` px apart along the flow: the
    direction at a point is radians(twist) x the fractal noise (`flow`: scale, octaves, persistence, lacunarity,
    seed [`seed`, 1]) at the pixel containing it. A line ends at the first point outside the picture, which is not
    stamped. A disc covers a pixel by clip(radius - distance from the pixel centre to the point + 1/2, 0, 1) and a
    pixel takes the strongest disc over it."""
    height, width = shape
    heading = np.radians(drawing["twist"]) * fractal(shape, seed=[seed, 1], **flow)
    starts = np.random.default_rng(seed).random((round(density * height * width), 2)) * (width, height)
    rows, cols = np.mgrid[0:height, 0:width] + 0.5
    out = np.zeros(shape)
    for x, y in starts:
        for _ in range(int(drawing["length"])):
            if not (0 <= x < width and 0 <= y < height):
                break
            reach = drawing["thickness"] / 2 - np.hypot(cols - x, rows - y) + 0.5
            out = np.maximum(out, np.clip(reach, 0, 1))
            angle = heading[int(y), int(x)]
            x, y = x + drawing["step"] * np.cos(angle), y + drawing["step"] * np.sin(angle)
    return out


FLOW = {"scale": 12.0, "octaves": 2, "persistence": 0.5, "lacunarity": 2.0}


@pytest.mark.parametrize(
    ("settings", "seed"),
    [
        pytest.param(
            {"twist": 0, "length": 10, "step": 1.0, "thickness": 1.0, "density": 0.001}, 0, id="one short straight line"
        ),
        pytest.param(
            {"twist": 0, "length": 12, "step": 2.5, "thickness": 3.5, "density": 0.001},
            1,
            id="a thick line, step wider than the radius",
        ),
        pytest.param(
            {"twist": 0, "length": 200, "step": 3.0, "thickness": 1.5, "density": 0.001},
            2,
            id="a straight line runs off the right edge and stops",
        ),
        pytest.param(
            {"twist": 180, "length": 40, "step": 1.0, "thickness": 1.5, "density": 0.01},
            3,
            id="curving lines, half a turn per sigma",
        ),
        pytest.param(
            {"twist": 720, "length": 60, "step": 1.5, "thickness": 1.0, "density": 0.02},
            4,
            id="tightly twisted lines leave through every edge",
        ),
        pytest.param(
            {"twist": 2, "length": 40, "step": 1.0, "thickness": 1.0, "density": 0.01},
            5,
            id="a slight twist is in degrees, not radians",
        ),
        pytest.param(
            {"twist": 720, "length": 80, "step": 1.5, "thickness": 1.0, "density": 0.03},
            6,
            id="many twisted lines, some turn back through the left and top edges",
        ),
    ],
)
def test_lines_follow_the_noise_flow_as_a_brute_force_tracing_does(settings: dict, seed: int) -> None:
    shape = (30, 40)
    out = render(shape, seed=seed, **FLOW, **settings)
    expected = reference_coverage(shape, seed=seed, flow=FLOW, **settings)
    np.testing.assert_allclose(coverage(out), expected, atol=1e-3)
    assert expected.max() > 0.4  # something was drawn
    background = out[expected == 0]
    np.testing.assert_allclose(background, np.tile(BACKGROUND, (len(background), 1)), atol=1e-6)


def test_no_density_draws_no_lines_and_more_density_draws_more() -> None:
    np.testing.assert_allclose(render(density=0), np.tile(BACKGROUND, (20, 50, 1)), atol=1e-6)
    lit = [(coverage(render((60, 80), density=d, seed=3)) > 0.5).mean() for d in (0.001, 0.003, 0.01)]
    assert 0 < lit[0] < lit[1] < lit[2]


def test_the_seed_decides_the_lines() -> None:
    np.testing.assert_array_equal(render(seed=4), render(seed=4))
    assert not np.array_equal(render(seed=4), render(seed=5))


def test_the_picture_only_gives_the_size() -> None:
    busy = np.random.default_rng(0).random((20, 50, 3), dtype=np.float32)
    np.testing.assert_array_equal(registry.apply(NAME, busy), registry.apply(NAME, np.zeros_like(busy)))
