"""Behaviour of the `pixel_sort` effect: runs of pixels between two thresholds sorted along rows or columns."""

import colorsys
import itertools

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "pixel_sort"


def brightness(rgb: np.ndarray) -> np.ndarray:
    """Rec. 709 luma of every pixel, in float64, written out here as the standard states it."""
    return 0.2126 * rgb[..., 0].astype(float) + 0.7152 * rgb[..., 1] + 0.0722 * rgb[..., 2]


def hue(rgb: np.ndarray) -> np.ndarray:
    """The hue 0..1 of every pixel by Python's own `colorsys`, one pixel at a time."""
    flat = rgb.reshape(-1, 3)
    return np.array([colorsys.rgb_to_hsv(*pixel)[0] for pixel in flat]).reshape(rgb.shape[:2])


KEYS = {"brightness": brightness, "hue": hue}


def lines(rgb: np.ndarray, axis: str) -> np.ndarray:
    """The picture as (lines, pixels along a line, 3): rows for 'horizontal', columns for 'vertical'."""
    return rgb if axis == "horizontal" else rgb.transpose(1, 0, 2)


@pytest.mark.parametrize("order", ["ascending", "descending"])
@pytest.mark.parametrize("axis", ["horizontal", "vertical"])
@pytest.mark.parametrize("by", ["brightness", "hue"])
def test_each_run_between_the_thresholds_is_sorted_by_the_key_keeping_its_own_pixels_and_the_rest_stays(
    photo, by, axis, order
) -> None:
    low, high = 0.3, 0.7
    out = registry.apply(NAME, photo, by=by, low=low, high=high, axis=axis, order=order)
    key_in, key_out = (lines(KEYS[by](picture)[..., None], axis)[..., 0] for picture in (photo, out))
    pixels_in, pixels_out = lines(photo, axis), lines(out, axis)
    sign = 1 if order == "ascending" else -1
    sorted_somewhere = False
    for line in range(len(pixels_in)):
        inside = (key_in[line] >= low) & (key_in[line] <= high)
        # pixels outside the thresholds do not move
        assert np.array_equal(pixels_out[line][~inside], pixels_in[line][~inside])
        # a run is a stretch of consecutive pixels inside the thresholds
        position = 0
        for is_inside, group in itertools.groupby(inside):
            size = len(list(group))
            run = slice(position, position + size)
            position += size
            if not is_inside:
                continue
            # the run keeps exactly its own pixels, and they are now in order of the key
            before = sorted(map(tuple, pixels_in[line][run]))
            after = sorted(map(tuple, pixels_out[line][run]))
            assert before == after
            assert (sign * np.diff(key_out[line][run]) >= -1e-6).all()
            sorted_somewhere |= not np.array_equal(pixels_in[line][run], pixels_out[line][run])
    assert sorted_somewhere  # the picture has runs that were not in order already


@pytest.mark.parametrize(
    ("by", "low", "high", "order", "row", "expected"),
    [
        ("brightness", 0.2, 0.8, "ascending", [0.1, 0.7, 0.5, 0.3, 0.9, 0.4, 0.6], [0.1, 0.3, 0.5, 0.7, 0.9, 0.4, 0.6]),
        (
            "brightness",
            0.2,
            0.8,
            "descending",
            [0.1, 0.7, 0.5, 0.3, 0.9, 0.4, 0.6],
            [0.1, 0.7, 0.5, 0.3, 0.9, 0.6, 0.4],
        ),
        ("brightness", 0.8, 0.2, "ascending", [0.1, 0.7, 0.5, 0.3, 0.9, 0.4, 0.6], [0.1, 0.7, 0.5, 0.3, 0.9, 0.4, 0.6]),
        ("brightness", 0.5, 0.5, "ascending", [0.7, 0.5, 0.5, 0.3, 0.5], [0.7, 0.5, 0.5, 0.3, 0.5]),
        ("brightness", 0.25, 0.75, "ascending", [0.9, 0.75, 0.25, 0.5], [0.9, 0.25, 0.5, 0.75]),
    ],
    ids=[
        "ascending",
        "descending",
        "low above high sorts nothing",
        "a run of equal pixels stays as it is",
        "both thresholds are inclusive",
    ],
)
def test_a_row_of_greys_is_sorted_between_the_thresholds_as_worked_out_by_hand(by, low, high, order, row, expected):
    """Greys have the brightness of their value, so the runs and their order can be read off the list."""
    grey = lambda values: np.repeat(np.array([values], np.float32)[..., None], 3, axis=-1)  # noqa: E731
    out = registry.apply(NAME, grey(row), by=by, low=low, high=high, axis="horizontal", order=order)
    np.testing.assert_allclose(out, grey(expected), atol=1e-6)


def test_hue_runs_go_red_yellow_green_blue_and_a_column_is_sorted_the_same_way() -> None:
    blue, red, green, yellow = [0, 0, 1], [1, 0, 0], [0, 1, 0], [1, 1, 0]
    row = np.array([[blue, red, green, yellow]], np.float32)  # hues 2/3, 0, 1/3, 1/6
    expected = np.array([[red, yellow, green, blue]], np.float32)
    assert np.array_equal(registry.apply(NAME, row, by="hue", low=0.0, high=1.0, axis="horizontal"), expected)
    column = row.transpose(1, 0, 2)
    out = registry.apply(NAME, column, by="hue", low=0.0, high=1.0, axis="vertical")
    assert np.array_equal(out, expected.transpose(1, 0, 2))
