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


def sampled_row_gains(spacing: float, thickness: float, softness: float, darkness: float, offset: float) -> np.ndarray:
    """An independent oracle: the line profile written out as a function of the distance from the middle of the nearest
    line (1 inside the hard line shrunk by the fade, a raised cosine falling to 0 across the fade, 0 in the gap), its
    mean over 4000 samples across each of 8 rows."""
    half = thickness / 2  # in periods
    reach = softness * min(half, 0.5 - half)  # the fade extends this far either side of a hard edge

    def profile(x: np.ndarray) -> np.ndarray:
        turns = (x - offset) / spacing - half  # periods from the line's middle
        away = np.abs(turns - np.round(turns))
        inner, outer = half - reach, half + reach
        fall = np.clip((away - inner) / (outer - inner), 0, 1)
        return 0.5 * (1 + np.cos(np.pi * fall))

    samples = np.arange(8 * 4000) / 4000 + 0.5 / 4000
    return 1 - darkness * profile(samples).reshape(8, 4000).mean(axis=1)


@pytest.mark.parametrize(
    ("spacing", "thickness", "softness", "offset"),
    [
        pytest.param(4, 0.5, 1.0, 0.0, id="sin^2: half lines, fully soft"),
        pytest.param(4, 0.5, 0.5, 0.0, id="half soft"),
        pytest.param(5.5, 0.2, 1.0, 1.3, id="thin lines, a spacing off the rows, shifted"),
        pytest.param(5.5, 0.8, 1.0, 1.3, id="thick lines: the gap limits the fade"),
        pytest.param(3, 0.3, 0.7, 2.0, id="a fade shorter than a row"),
    ],
)
def test_soft_lines_darken_each_row_by_the_mean_of_the_profile_across_it(
    spacing: float, thickness: float, softness: float, offset: float
) -> None:
    got = row_gains(spacing=spacing, thickness=thickness, darkness=0.6, offset=offset, softness=softness)
    np.testing.assert_allclose(got, sampled_row_gains(spacing, thickness, softness, 0.6, offset), atol=2e-4)


def test_softness_1_with_thickness_half_is_the_sin_squared_of_a_crt() -> None:
    """The darkness is cos^2 (pi (x - c) / spacing) about the middle of a line, c = offset + spacing / 4. Its mean
    over the row from r to r + 1 is 1/2 + spacing sin (pi / spacing) cos (2 pi (r + 1/2 - c) / spacing) / (2 pi): with
    spacing 8 and offset 1 the rows' gains are 1 - 0.6 times that."""
    spacing, centre = 8, 1 + 8 / 4
    rows = np.arange(8)
    mean = 0.5 + spacing * np.sin(np.pi / spacing) * np.cos(2 * np.pi * (rows + 0.5 - centre) / spacing) / (2 * np.pi)
    got = row_gains(spacing=spacing, thickness=0.5, softness=1.0, darkness=0.6, offset=1.0)
    np.testing.assert_allclose(got, 1 - 0.6 * mean, atol=1e-6)


@pytest.mark.parametrize("thickness", [0.2, 0.5, 0.8])
@pytest.mark.parametrize("softness", [0.0, 0.4, 1.0])
def test_softness_keeps_the_average_darkening_over_whole_periods(thickness: float, softness: float) -> None:
    """Over 8 rows of spacing 4 (two periods) the picture is darkened by darkness x thickness on average, however the
    line's edges are shaped."""
    gains = row_gains(spacing=4, thickness=thickness, darkness=0.5, offset=0.7, softness=softness)
    assert gains.mean() == pytest.approx(1 - 0.5 * thickness, abs=1e-6)


def test_softness_leaves_the_hard_lines_as_they_were_at_0_and_moves_the_edges_at_more() -> None:
    hard = row_gains(spacing=4, thickness=0.5, darkness=0.5, softness=0.0)
    soft = row_gains(spacing=4, thickness=0.5, darkness=0.5, softness=0.6)
    np.testing.assert_allclose(hard, [0.5, 0.5, 1, 1, 0.5, 0.5, 1, 1], atol=1e-6)
    assert not np.allclose(hard, soft, atol=0.01)
    assert (soft[[0, 4]] > hard[[0, 4]]).all()  # the first row of a line is lighter once its edge fades


def test_a_soft_pattern_moves_down_with_the_offset() -> None:
    plain = row_gains(spacing=4, thickness=0.5, darkness=0.5, softness=1.0, offset=0)
    moved = row_gains(spacing=4, thickness=0.5, darkness=0.5, softness=1.0, offset=1)
    np.testing.assert_allclose(moved[1:], plain[:-1], atol=1e-6)
