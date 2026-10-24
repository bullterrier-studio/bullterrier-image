"""Behaviour of the `scanlines` effect."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "scanlines"
GREY = 0.6


def row_gains(**params: float) -> np.ndarray:
    """What scanlines does to each row of an 8-row grey picture, as output / input, one value per row."""
    grey = np.full((8, 5, 3), GREY, np.float32)
    out = registry.apply(NAME, grey, **params)
    assert (out == out[:, :1, :1]).all()  # a row is one grey, whatever the column or channel
    return out[:, 0, 0] / GREY


@pytest.mark.parametrize(
    ("params", "gains"),
    [
        pytest.param(
            {"spacing": 4, "thickness": 0.5, "darkness": 0.5, "offset": 0},
            [0.5, 0.5, 1, 1, 0.5, 0.5, 1, 1],
            id="two dark rows in every four, at half darkness",
        ),
        pytest.param(
            {"spacing": 4, "thickness": 0.25, "darkness": 1, "offset": 1},
            [1, 0, 1, 1, 1, 0, 1, 1],
            id="offset 1 moves the single black row down one",
        ),
        pytest.param(
            {"spacing": 4, "thickness": 0.25, "darkness": 1, "offset": 5},
            [1, 0, 1, 1, 1, 0, 1, 1],
            id="offset 5 with spacing 4 is offset 1",
        ),
        pytest.param(
            {"spacing": 4, "thickness": 0.375, "darkness": 0.4, "offset": 0},
            [0.6, 0.8, 1, 1, 0.6, 0.8, 1, 1],
            id="a 1.5 px line covers one row and half of the next",
        ),
        pytest.param(
            {"spacing": 4, "thickness": 0.5, "darkness": 0, "offset": 0},
            [1] * 8,
            id="darkness 0 changes nothing",
        ),
        pytest.param(
            {"spacing": 4, "thickness": 0, "darkness": 1, "offset": 0},
            [1] * 8,
            id="thickness 0 changes nothing",
        ),
        pytest.param(
            {"spacing": 4, "thickness": 1, "darkness": 0.3, "offset": 0},
            [0.7] * 8,
            id="thickness 1 darkens every row alike",
        ),
    ],
)
def test_each_row_is_darkened_by_the_share_of_it_a_line_covers(params: dict, gains: list[float]) -> None:
    np.testing.assert_allclose(row_gains(**params), gains, atol=1e-6)


def test_rows_not_under_a_line_are_unchanged_to_the_bit(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, spacing=4, thickness=0.5, darkness=0.7)
    np.testing.assert_array_equal(out[2::4], photo[2::4])
    np.testing.assert_array_equal(out[3::4], photo[3::4])
    assert (out[0::4] < photo[0::4]).all()


def test_lines_darken_every_column_and_channel_by_the_same_ratio(photo: np.ndarray) -> None:
    ratio = registry.apply(NAME, photo, spacing=3, darkness=0.5) / photo
    np.testing.assert_allclose(ratio, ratio[:, :1, :1] * np.ones_like(ratio), atol=1e-5)
