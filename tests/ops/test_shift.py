"""Tests of `ops/shift.py`: a picture or one channel moved by whole or fractional pixels, with a rule for the edge.

The oracle is a brute-force reference written here: pad the picture with numpy's own edge modes (`np.pad`), then read
each output pixel bilinearly at the point it came from.
"""

from typing import Literal

import numpy as np
import pytest

from photo_fx.ops.shift import Edge, shift

EDGES: list[Edge] = ["wrap", "nearest", "reflect", "mirror"]
# numpy's names for the same rules: its 'symmetric' repeats the edge pixel (our 'reflect'), its 'reflect' does not
NUMPY_PAD: dict[Edge, Literal["wrap", "edge", "symmetric", "reflect"]] = {
    "wrap": "wrap",
    "nearest": "edge",
    "reflect": "symmetric",
    "mirror": "reflect",
}


def noise(shape: tuple[int, ...], seed: int = 0) -> np.ndarray:
    return np.random.default_rng(seed).random(shape, dtype=np.float32)


def reference(image: np.ndarray, dy: float, dx: float, edge: Edge) -> np.ndarray:
    """Output (i, j) is the picture at (i - dy, j - dx), bilinear between the four pixels around that point, with the
    picture continued beyond its edge by numpy's padding."""
    margin = int(max(abs(dy), abs(dx))) + 3
    padding = [(margin, margin), (margin, margin)] + [(0, 0)] * (image.ndim - 2)
    padded = np.pad(image.astype(np.float64), padding, mode=NUMPY_PAD[edge])
    out = np.zeros(image.shape, np.float64)
    for i in range(image.shape[0]):
        for j in range(image.shape[1]):
            y, x = i - dy + margin, j - dx + margin
            y0, x0 = int(np.floor(y)), int(np.floor(x))
            wy, wx = y - y0, x - x0
            out[i, j] = (
                (1 - wy) * (1 - wx) * padded[y0, x0]
                + (1 - wy) * wx * padded[y0, x0 + 1]
                + wy * (1 - wx) * padded[y0 + 1, x0]
                + wy * wx * padded[y0 + 1, x0 + 1]
            )
    return out


@pytest.mark.parametrize("edge", EDGES)
@pytest.mark.parametrize(
    ("dy", "dx"),
    [(0, 0), (2, 0), (0, -3), (3, 2), (-1, 4), (1.5, 0), (0, -2.25), (2.75, -1.5), (13, -17), (-40.5, 33.25)],
    ids=lambda value: str(value),
)
@pytest.mark.parametrize("shape", [(6, 7, 3), (6, 7), (1, 5, 3), (4, 1)], ids=["picture", "channel", "row", "column"])
def test_shift_agrees_with_a_brute_force_bilinear_read_of_the_padded_picture(shape, dy, dx, edge) -> None:
    image = noise(shape)
    np.testing.assert_allclose(shift(image, dy, dx, edge=edge), reference(image, dy, dx, edge), atol=2e-6)


@pytest.mark.parametrize(
    ("dy", "dx", "lit"),
    [
        pytest.param(2, 3, {(5, 7): 1.0}, id="whole pixels: down and right"),
        pytest.param(-2, -3, {(1, 1): 1.0}, id="negative: up and left"),
        pytest.param(1.5, 2.25, {(4, 6): 0.375, (4, 7): 0.125, (5, 6): 0.375, (5, 7): 0.125}, id="fractions: bilinear"),
    ],
)
def test_a_pixel_moves_by_the_distance_and_a_fraction_is_shared_by_the_pixels_it_lies_between(dy, dx, lit) -> None:
    """A white pixel at row 3, column 4 of a black 10 x 12 picture, hand-worked: 1.5 down gives half to each of two
    rows, 2.25 right three quarters to column 6 and a quarter to column 7, and the weights multiply."""
    image = np.zeros((10, 12), np.float32)
    image[3, 4] = 1.0
    out = shift(image, dy, dx, edge="nearest")
    assert {(int(r), int(c)): float(out[r, c]) for r, c in zip(*np.nonzero(out), strict=True)} == pytest.approx(lit)


@pytest.mark.parametrize("edge", EDGES)
def test_no_shift_returns_an_equal_picture_that_is_not_the_input(edge: Edge) -> None:
    image = noise((5, 6, 3))
    out = shift(image, 0, 0, edge=edge)
    np.testing.assert_array_equal(out, image)
    assert not np.shares_memory(out, image)


@pytest.mark.parametrize("edge", EDGES)
def test_the_input_is_never_modified(edge: Edge) -> None:
    image = noise((5, 6, 3))
    before = image.copy()
    shift(image, 2.5, -1.25, edge=edge)
    np.testing.assert_array_equal(image, before)


@pytest.mark.parametrize(("dy", "dx"), [(0.3, 0.0), (-2.5, 4.75), (11.0, -23.0), (0.0, 7.9)])
def test_wrapping_loses_nothing_so_the_total_of_the_picture_is_kept(dy: float, dx: float) -> None:
    image = noise((9, 11, 3))
    assert shift(image, dy, dx, edge="wrap").sum(dtype=np.float64) == pytest.approx(
        image.sum(dtype=np.float64), rel=1e-5
    )


def test_a_picture_wrapped_by_whole_pixels_and_back_is_the_picture() -> None:
    image = noise((9, 11, 3))
    np.testing.assert_array_equal(shift(shift(image, 4, -7, edge="wrap"), -4, 7, edge="wrap"), image)


def test_the_output_keeps_the_dtype_of_the_input() -> None:
    assert shift(noise((4, 5, 3)), 1.5, 0.5, edge="nearest").dtype == np.float32


@pytest.mark.parametrize(
    ("edge", "known"),
    [pytest.param("wrap", True), pytest.param("clamp", False, id="an unknown rule is refused")],
)
def test_only_the_documented_edge_rules_are_accepted(edge: str, known: bool) -> None:
    if known:
        shift(noise((3, 3)), 1, 1, edge=edge)  # type: ignore[arg-type]
    else:
        with pytest.raises(ValueError, match="edge"):
            shift(noise((3, 3)), 1, 1, edge=edge)  # type: ignore[arg-type]
