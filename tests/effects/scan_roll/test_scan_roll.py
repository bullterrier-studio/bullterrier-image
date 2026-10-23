"""Behaviour of the `scan_roll` effect: the picture rolled vertically or horizontally, as a slipped sync does."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "scan_roll"


def impulse(height: int, width: int, row: int, column: int) -> np.ndarray:
    """A black picture with one white pixel."""
    rgb = np.zeros((height, width, 3), np.float32)
    rgb[row, column] = 1.0
    return rgb


def lit(rgb: np.ndarray) -> dict[tuple[int, int], float]:
    """The pixels of a grey-or-black picture that are not black, by (row, column), with their value."""
    return {(int(r), int(c)): float(rgb[r, c, 0]) for r, c in zip(*np.nonzero(rgb[..., 0]), strict=True)}


@pytest.mark.parametrize("axis", ["vertical", "horizontal"])
@pytest.mark.parametrize("offset", [0.0, 1.0, -1.0])
def test_no_offset_and_a_whole_picture_roll_leave_the_picture_unchanged(axis: str, offset: float, photo) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, axis=axis, offset=offset), photo)


@pytest.mark.parametrize(
    ("axis", "offset", "where"),
    [
        pytest.param("vertical", 0.25, (7, 4), id="vertical, a quarter of 20 rows is 5: down"),
        pytest.param("vertical", -0.25, (17, 4), id="vertical, negative goes up"),
        pytest.param("vertical", 0.5, (12, 4), id="vertical, wraps from the bottom to the top"),
        pytest.param("horizontal", 0.5, (2, 14), id="horizontal, half of 20 columns is 10: right"),
        pytest.param("horizontal", -0.1, (2, 2), id="horizontal, negative goes left"),
        pytest.param("horizontal", 0.95, (2, 3), id="horizontal, wraps past the right edge"),
    ],
)
def test_a_pixel_moves_by_the_offset_times_the_size_and_wraps_around(axis: str, offset: float, where) -> None:
    """The picture is 20 rows by 20 columns; the pixel starts at row 2, column 4 (rows count down, columns across).
    Offsets are fractions of the height (vertical) or width (horizontal)."""
    out = registry.apply(NAME, impulse(20, 20, 2, 4), axis=axis, offset=offset)
    assert lit(out) == {where: 1.0}


@pytest.mark.parametrize(
    ("offset", "blend"),
    [
        pytest.param(0.125, {(1, 6): 0.5, (1, 7): 0.5}, id="2.5 px: half on each of two pixels"),
        pytest.param(0.0625, {(1, 5): 0.75, (1, 6): 0.25}, id="1.25 px: three quarters on the nearer pixel"),
        pytest.param(0.0875, {(1, 5): 0.25, (1, 6): 0.75}, id="1.75 px: three quarters on the farther pixel"),
        pytest.param(-0.0625, {(1, 2): 0.25, (1, 3): 0.75}, id="-1.25 px: leftwards, a quarter on the farther pixel"),
    ],
)
def test_a_roll_that_ends_between_pixels_shares_the_pixel_between_the_two_it_lies_between(offset, blend) -> None:
    """On 20 columns the offset is 20 x offset px; a pixel at column 4 lands at 4 + that, at the fraction beyond a
    whole pixel the weight of the farther one (linear interpolation, worked out by hand)."""
    out = registry.apply(NAME, impulse(3, 20, 1, 4), axis="horizontal", offset=offset)
    assert lit(out) == pytest.approx(blend)


@pytest.mark.parametrize(
    ("edge", "expected"),
    [
        pytest.param("wrap", [3, 4, 1, 2], id="wrap: what leaves at the bottom comes back at the top"),
        pytest.param("nearest", [1, 1, 1, 2], id="nearest: the first row is held"),
        pytest.param("reflect", [2, 1, 1, 2], id="reflect: the first row repeats, then the picture mirrors"),
        pytest.param("mirror", [3, 2, 1, 2], id="mirror: the picture mirrors about its first row"),
    ],
)
def test_edge_decides_what_fills_the_gap_a_roll_opens(edge: str, expected: list[int]) -> None:
    """Four rows with the values 1..4 rolled down by two rows: the top two rows are the ones the roll opens."""
    rows = np.broadcast_to(np.array([1, 2, 3, 4], np.float32)[:, None, None] / 4, (4, 5, 3)).copy()
    out = registry.apply(NAME, rows, axis="vertical", offset=0.5, edge=edge)
    np.testing.assert_allclose(out[:, 2, 0], np.array(expected) / 4, atol=1e-6)


@pytest.mark.parametrize(
    ("edge", "expected"),
    [
        pytest.param("wrap", [3, 4, 1, 2], id="wrap"),
        pytest.param("nearest", [3, 4, 4, 4], id="nearest: the last row is held"),
        pytest.param("reflect", [3, 4, 4, 3], id="reflect: the last row repeats, then the picture mirrors"),
        pytest.param("mirror", [3, 4, 3, 2], id="mirror: the picture mirrors about its last row"),
    ],
)
def test_a_roll_the_other_way_opens_the_gap_at_the_far_end(edge: str, expected: list[int]) -> None:
    """The same four rows 1..4 rolled up by two rows: the bottom two rows are the ones the roll opens."""
    rows = np.broadcast_to(np.array([1, 2, 3, 4], np.float32)[:, None, None] / 4, (4, 5, 3)).copy()
    out = registry.apply(NAME, rows, axis="vertical", offset=-0.5, edge=edge)
    np.testing.assert_allclose(out[:, 2, 0], np.array(expected) / 4, atol=1e-6)


def test_edge_rejects_a_rule_that_is_not_offered(photo) -> None:
    with pytest.raises(ValueError, match="edge"):
        registry.apply(NAME, photo, edge="constant")


@pytest.mark.parametrize("edge", ["wrap", "nearest", "reflect", "mirror"])
def test_a_flat_picture_stays_flat(edge: str) -> None:
    out = registry.apply(NAME, np.full((9, 11, 3), 0.3, np.float32), offset=0.37, edge=edge)
    np.testing.assert_allclose(out, 0.3, atol=1e-6)
