"""Behaviour of the `reaction_diffusion` effect: a Gray-Scott pattern grown from the bright parts of the picture."""

from typing import Literal

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "reaction_diffusion"
DU, DV = 0.2097, 0.105  # the diffusion rates of the model, as the effect documents them (Pearson 1993)


def render(rgb: np.ndarray, **settings: object) -> np.ndarray:
    """The effect with a black-to-white ramp, one simulation cell per pixel and exact reads unless `settings` differ."""
    settings = {"colors": ["#000000", "#ffffff"], "cell": 1, "resampling": "nearest", **settings}
    return registry.apply(NAME, rgb, **settings)


def grey_square(size: int = 24, low: float = 0.1, high: float = 0.9) -> np.ndarray:
    """A `low` grey picture with a `high` grey square in its middle (8 px for size 24) and a 2 px one in the top-left
    corner, so that what lies beyond the borders matters."""
    rgb = np.full((size, size, 3), low, np.float32)
    rgb[size // 3 : 2 * size // 3, size // 3 : 2 * size // 3] = high
    rgb[:2, :2] = high
    return rgb


PAD_MODES: dict[str, Literal["wrap", "symmetric", "edge", "reflect"]] = {
    "wrap": "wrap",
    "reflect": "symmetric",
    "nearest": "edge",
    "mirror": "reflect",
}


def gray_scott(seeded: np.ndarray, *, feed: float, kill: float, steps: int, edge: str) -> np.ndarray:
    """The Gray-Scott model written out with numpy's own padding, not scipy's filters: u = 1 and v = 0 everywhere,
    u = 1/2 and v = 1/4 where `seeded`; every step u += Du lap(u) - u v^2 + feed (1 - u) and
    v += Dv lap(v) + u v^2 - (feed + kill) v with the five-point Laplacian. Returns 1 - u."""
    pad_mode = PAD_MODES[edge]

    def laplacian(field: np.ndarray) -> np.ndarray:
        p = np.pad(field, 1, mode=pad_mode)
        return p[:-2, 1:-1] + p[2:, 1:-1] + p[1:-1, :-2] + p[1:-1, 2:] - 4 * field

    u = np.where(seeded, 0.5, 1.0)
    v = np.where(seeded, 0.25, 0.0)
    for _ in range(steps):
        reaction = u * v * v
        u, v = (
            u + DU * laplacian(u) - reaction + feed * (1 - u),
            v + DV * laplacian(v) + reaction - (feed + kill) * v,
        )
    return 1 - u


def test_with_no_steps_the_picture_is_just_the_seed() -> None:
    """Chemical u falls to 1/2 where the picture is brighter than `threshold`, so 1 - u is 1/2 there and 0 elsewhere."""
    rgb = grey_square()
    out = render(rgb, steps=0, threshold=0.5)
    np.testing.assert_allclose(out[..., 0], np.where(rgb[..., 0] > 0.5, 0.5, 0.0), atol=1e-6)


@pytest.mark.parametrize("threshold", [0.05, 0.5, 0.95])
def test_the_threshold_decides_which_pixels_seed_the_pattern(threshold: float) -> None:
    ramp = np.linspace(0, 1, 40, dtype=np.float32)[None, :, None].repeat(3, axis=2).repeat(4, axis=0)
    seeded = render(ramp, steps=0, threshold=threshold)[..., 0] > 0.25
    np.testing.assert_array_equal(seeded, ramp[..., 0] > threshold)


def test_a_picture_with_nothing_above_the_threshold_stays_flat_however_long_it_runs() -> None:
    """u = 1, v = 0 everywhere is a fixed point of the model: nothing is there to react."""
    out = render(np.full((20, 30, 3), 0.3, np.float32), steps=300, threshold=0.5)
    np.testing.assert_array_equal(out, 0.0)


@pytest.mark.parametrize("edge", ["wrap", "reflect", "nearest", "mirror"])
@pytest.mark.parametrize(
    ("feed", "kill", "steps"),
    [(0.037, 0.06, 1), (0.037, 0.06, 40), (0.055, 0.062, 25), (0.02, 0.05, 25)],
)
def test_the_pattern_is_the_gray_scott_model_stepped_from_the_seed(
    edge: str, feed: float, kill: float, steps: int
) -> None:
    rgb = grey_square()
    out = render(rgb, feed=feed, kill=kill, steps=steps, edge=edge, threshold=0.5)
    expected = gray_scott(rgb[..., 0] > 0.5, feed=feed, kill=kill, steps=steps, edge=edge)
    np.testing.assert_allclose(out[..., 0], expected, atol=2e-4)
    assert expected.max() > 0.3  # the model really moved: a flat result would pass the comparison above trivially


def test_the_pattern_spreads_beyond_the_seed_over_time() -> None:
    rgb = grey_square(48)
    reached = [(render(rgb, steps=n, threshold=0.5)[..., 0] > 0.05).sum() for n in (0, 200, 600)]
    assert reached[0] < reached[1] < reached[2]


def test_a_coarse_cell_simulates_on_a_grid_of_cells_and_reads_each_back_over_its_pixels() -> None:
    """With cell 4 and nearest reading, every 4 x 4 block of pixels shows one simulated cell."""
    rgb = grey_square(48)
    out = render(rgb, cell=4, steps=60, threshold=0.5)[..., 0]
    blocks = out.reshape(12, 4, 12, 4)
    np.testing.assert_array_equal(blocks, blocks[:, :1, :, :1].repeat(4, axis=1).repeat(4, axis=3))
    assert len(np.unique(blocks)) > 20  # many different values, not only the seed's two


def test_the_picture_is_read_at_the_centre_of_each_cell() -> None:
    """With cells of 3 px the first cell's centre is pixel 1, the next pixel 4... A picture bright only on those
    pixels seeds every cell; reading at a cell's corner (pixel 0, 3...) would see only darkness."""
    rgb = np.zeros((12, 15, 3), np.float32)
    rgb[1::3, 1::3] = 1.0
    out = render(rgb, cell=3, steps=0, threshold=0.5)
    np.testing.assert_allclose(out, 0.5, atol=1e-6)


def test_colors_are_applied_along_how_far_the_chemical_has_dropped() -> None:
    rgb = grey_square()
    out = render(rgb, steps=0, threshold=0.5, colors=["#000000", "#ff0000", "#0000ff"])  # 1 - u = 1/2 is the middle one
    np.testing.assert_allclose(out[12, 12], [1, 0, 0], atol=1e-6)  # inside the middle square
    np.testing.assert_allclose(out[0, 12], [0, 0, 0], atol=1e-6)  # outside both squares
