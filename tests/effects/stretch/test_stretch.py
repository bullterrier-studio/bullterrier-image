"""Behaviour of the `stretch` effect: one row or column of the picture smeared outward over its neighbours."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "stretch"


@pytest.fixture
def noise() -> np.ndarray:
    """41 rows and 61 columns of noise, so the middle row is 20 and the middle column 30 and no two lines agree."""
    return np.random.default_rng(3).random((41, 61, 3), dtype=np.float32)


def expected_rows(source: np.ndarray, line: int, smeared: range) -> np.ndarray:
    """`source` with the rows in `smeared` replaced by the row `line`, written out row by row."""
    out = source.copy()
    for row in smeared:
        out[row] = source[line]
    return out


@pytest.mark.parametrize(
    ("at", "span", "side", "line", "smeared"),
    [
        (0.5, 0.25, "both", 20, range(10, 31)),  # 0.25 of 41 rows is 10 rows each way
        (0.5, 0.25, "after", 20, range(20, 31)),
        (0.5, 0.25, "before", 20, range(10, 21)),
        (0.0, 0.5, "both", 0, range(21)),  # the first row has nothing before it
        (1.0, 0.5, "both", 40, range(20, 41)),  # the last row has nothing after it
        (0.25, 0.0, "both", 10, range(0)),  # a span of 0 smears nothing
    ],
    ids=["both ways", "after only", "before only", "at the top edge", "at the bottom edge", "span 0"],
)
def test_vertical_stretch_repeats_the_chosen_row_over_the_rows_beside_it_and_leaves_the_rest(
    noise, at, span, side, line, smeared
) -> None:
    out = registry.apply(NAME, noise, axis="vertical", at=at, span=span, side=side)
    assert np.array_equal(out, expected_rows(noise, line, smeared))


def test_a_span_of_one_smears_the_line_across_the_whole_picture(noise) -> None:
    out = registry.apply(NAME, noise, axis="vertical", at=0.5, span=1.0, side="both")
    assert np.array_equal(out, np.broadcast_to(noise[20], noise.shape))


def test_horizontal_stretch_is_the_vertical_one_of_the_turned_picture(noise) -> None:
    sideways = registry.apply(NAME, noise, axis="horizontal", at=0.5, span=0.2, side="after")
    turned = registry.apply(NAME, noise.transpose(1, 0, 2), axis="vertical", at=0.5, span=0.2, side="after")
    assert np.array_equal(sideways, turned.transpose(1, 0, 2))
    assert np.array_equal(sideways[:, 30:43], np.broadcast_to(noise[:, 30:31], (41, 13, 3)))
