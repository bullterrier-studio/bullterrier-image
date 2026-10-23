"""Behaviour of the `mirror` effect: one half of the picture kept, the other replaced by its reflection."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "mirror"
SIZES = [(96, 128), (13, 29), (61, 37)]  # even by even, and two odd ones: the middle row or column is its own mirror


def row(*values: float) -> np.ndarray:
    """A grey picture one pixel high whose columns have `values`."""
    return np.broadcast_to(np.array(values, np.float32)[None, :, None], (1, len(values), 3)).copy()


@pytest.mark.parametrize("keep", ["first", "last"])
@pytest.mark.parametrize("axis", ["vertical", "horizontal", "both"])
@pytest.mark.parametrize("size", SIZES, ids=lambda size: f"{size[0]}x{size[1]}")
def test_the_result_is_symmetric_about_each_mirror_line(axis: str, keep: str, size: tuple[int, int], photo) -> None:
    out = registry.apply(NAME, photo[: size[0], : size[1]], axis=axis, keep=keep)
    if axis in ("vertical", "both"):
        np.testing.assert_array_equal(out, np.flip(out, axis=1))
    if axis in ("horizontal", "both"):
        np.testing.assert_array_equal(out, np.flip(out, axis=0))


@pytest.mark.parametrize("size", SIZES, ids=lambda size: f"{size[0]}x{size[1]}")
def test_the_kept_half_is_untouched_and_the_other_is_its_reflection(size: tuple[int, int], photo) -> None:
    """Rows and columns count from 0; an odd picture keeps its middle line with either half."""
    height, width = size
    rgb = photo[:height, :width]
    first = registry.apply(NAME, rgb, axis="both", keep="first")
    last = registry.apply(NAME, rgb, axis="both", keep="last")
    top, left = (height + 1) // 2, (width + 1) // 2  # the first half: this many rows and columns
    np.testing.assert_array_equal(first[:top, :left], rgb[:top, :left])
    np.testing.assert_array_equal(last[height // 2 :, width // 2 :], rgb[height // 2 :, width // 2 :])
    # the opposite corner, read backwards in rows and columns, is the kept quarter
    np.testing.assert_array_equal(first[height // 2 :, width // 2 :][::-1, ::-1], rgb[:top, :left])
    np.testing.assert_array_equal(
        last[: height - height // 2, : width - width // 2][::-1, ::-1], rgb[height // 2 :, width // 2 :]
    )


@pytest.mark.parametrize(
    ("columns", "keep", "expected"),
    [
        ((1, 2, 3, 4), "first", (1, 2, 2, 1)),
        ((1, 2, 3, 4), "last", (4, 3, 3, 4)),
        ((1, 2, 3), "first", (1, 2, 1)),
        ((1, 2, 3), "last", (3, 2, 3)),
        ((7,), "first", (7,)),
    ],
    ids=["even, first half", "even, last half", "odd, first half", "odd, last half", "a single column"],
)
def test_a_row_keeps_its_first_or_last_half_and_reflects_it(columns, keep: str, expected) -> None:
    out = registry.apply(NAME, row(*columns) / 10, axis="vertical", keep=keep)
    np.testing.assert_allclose(out, row(*expected) / 10, atol=1e-7)


def test_the_horizontal_axis_reflects_top_onto_bottom_and_leaves_each_row_as_it_is() -> None:
    rows = np.array([1, 2, 3, 4], np.float32)[:, None, None] / 10 + np.zeros((4, 3, 3), np.float32)
    rows += np.array([0.0, 0.01, 0.02], np.float32)[None, :, None]  # distinct columns, to see them stay in place
    out = registry.apply(NAME, rows, axis="horizontal", keep="first")
    np.testing.assert_array_equal(out, rows[[0, 1, 1, 0]])


@pytest.mark.parametrize(
    ("setting", "refused"),
    [
        pytest.param({"axis": "diagonal"}, True, id="an axis that is not offered is refused"),
        pytest.param({"keep": "middle"}, True, id="a half that is not offered is refused"),
        pytest.param({"axis": "both", "keep": "last"}, False, id="every offered choice works"),
    ],
)
def test_only_the_documented_choices_are_accepted(setting: dict, refused: bool, photo) -> None:
    if refused:
        with pytest.raises(ValueError, match=next(iter(setting))):
            registry.apply(NAME, photo, **setting)
    else:
        registry.apply(NAME, photo, **setting)
