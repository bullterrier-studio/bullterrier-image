"""Behaviour of the `interlace` effect: the rows of one field slid sideways against those of the other."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "interlace"


def slid(rows: np.ndarray, offset: int, edge: str) -> np.ndarray:
    """`rows` slid right by a whole `offset` px, worked out by hand: output column j reads input column j - offset."""
    width = rows.shape[1]
    columns = np.arange(width) - offset
    columns = columns % width if edge == "wrap" else np.clip(columns, 0, width - 1)
    return rows[:, columns]


@pytest.mark.parametrize("edge", ["wrap", "nearest"])
@pytest.mark.parametrize("offset", [5, -3, 0])
@pytest.mark.parametrize(("field", "moved", "kept"), [("odd", 1, 0), ("even", 0, 1)])
def test_one_field_is_slid_by_the_offset_and_the_other_is_untouched(photo, field, moved, kept, offset, edge) -> None:
    out = registry.apply(NAME, photo, offset=offset, field=field, edge=edge)
    np.testing.assert_array_equal(out[kept::2], photo[kept::2])
    np.testing.assert_array_equal(out[moved::2], slid(photo[moved::2], offset, edge))


def test_a_fractional_offset_blends_the_two_whole_slides_it_lies_between(photo) -> None:
    out = registry.apply(NAME, photo, offset=2.25, edge="wrap")
    expected = 0.75 * slid(photo[1::2], 2, "wrap") + 0.25 * slid(photo[1::2], 3, "wrap")
    np.testing.assert_allclose(out[1::2], expected, atol=1e-6)
    np.testing.assert_array_equal(out[0::2], photo[0::2])


def test_a_picture_of_one_row_has_no_odd_field_to_slide(photo) -> None:
    row = photo[:1]
    np.testing.assert_array_equal(registry.apply(NAME, row, offset=7), row)
