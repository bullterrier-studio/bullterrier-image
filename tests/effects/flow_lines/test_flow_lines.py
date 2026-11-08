"""Behaviour of the `flow_lines` effect: thin streamlines through a smooth noise flow field, in place of the picture."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "flow_lines"
LINE, BACKGROUND = np.array([1.0, 128 / 255, 0.0]), np.array([0.0, 0.0, 64 / 255])


def render(shape: tuple[int, int] = (20, 50), **settings: object) -> np.ndarray:
    """Lines over a picture of `shape` (h, w), orange on dark blue; `settings` override."""
    settings = {"line": "#ff8000", "background": "#000040", "seed": 0, **settings}
    return registry.apply(NAME, np.zeros((*shape, 3), np.float32), **settings)


def coverage(out: np.ndarray) -> np.ndarray:
    """(h, w) 0..1: how much each pixel is line colour rather than background (read off the red channel)."""
    return out[..., 0] / LINE[0]


def test_no_density_draws_no_lines_and_more_density_draws_more() -> None:
    np.testing.assert_allclose(render(density=0), np.tile(BACKGROUND, (20, 50, 1)), atol=1e-6)
    lit = [(coverage(render((60, 80), density=d, seed=3)) > 0.5).mean() for d in (1, 3, 10)]
    assert 0 < lit[0] < lit[1] < lit[2]


@pytest.mark.parametrize("step", [0.5, 1.0, 4.0])
def test_a_line_is_as_long_as_length_whatever_the_step(step: float) -> None:
    """One straight line on a wide picture: it reaches `length` px (plus its own thickness), not a number of steps."""
    out = render((10, 400), density=0.25, twist=0, thickness=4.0, length=20.0, step=step, seed=4)  # 1 line starts
    lit = np.flatnonzero((coverage(out) > 0.5).any(axis=0))
    assert lit.max() - lit.min() + 1 == pytest.approx(20 + 4, abs=1)
