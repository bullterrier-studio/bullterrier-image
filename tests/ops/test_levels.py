"""`ops/levels`: how a position between evenly spaced levels is shared by the two levels either side of it."""

import numpy as np
import pytest

from bullterrier_image.ops.levels import level_weight


@pytest.mark.parametrize(
    ("position", "level", "expected"),
    [
        pytest.param(2.0, 2, 1.0, id="on a level the level has it all"),
        pytest.param(2.0, 1, 0.0, id="on a level the level before has none"),
        pytest.param(2.0, 3, 0.0, id="on a level the level after has none"),
        pytest.param(2.25, 2, 0.75, id="a quarter past: the near level keeps three quarters"),
        pytest.param(2.25, 3, 0.25, id="a quarter past: the next level has the quarter"),
        pytest.param(2.75, 3, 0.75, id="three quarters past: the next level has three quarters"),
        pytest.param(2.75, 2, 0.25, id="three quarters past: the near level keeps the quarter"),
        pytest.param(0.0, 0, 1.0, id="the first level at position 0"),
        pytest.param(0.5, 4, 0.0, id="a level more than one away has none"),
        pytest.param(5.0, 5, 1.0, id="the last level at the top position"),
    ],
)
def test_a_level_has_the_tent_weight_of_its_distance_from_the_position(
    position: float, level: int, expected: float
) -> None:
    got = level_weight(np.full((2, 3), position, np.float32), level)

    assert got.shape == (2, 3)
    assert got.dtype == np.float32
    np.testing.assert_allclose(got, expected, atol=1e-7)


def test_the_levels_of_every_position_add_up_to_one() -> None:
    position = np.linspace(0, 6, 61, dtype=np.float32).reshape(1, -1)

    total = sum(level_weight(position, level) for level in range(7))

    np.testing.assert_allclose(total, 1.0, atol=1e-6)
