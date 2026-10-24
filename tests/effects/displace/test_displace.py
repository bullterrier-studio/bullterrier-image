"""Behaviour of the `displace` effect: every pixel shows the picture at a point moved by two independent noise fields,
the largest move being `amount` px."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "displace"
HEIGHT, WIDTH = 200, 300
MARGIN = 20  # px from each border inside which no pixel can read beyond the picture (amount 8 and bicubic's reach)
SETTINGS = {"amount": 8.0, "scale": 12.0, "octaves": 2, "seed": 3}


def ramps() -> np.ndarray:
    """A picture whose red is its row / (height - 1) and green its column / (width - 1): linear, so bilinear and bicubic
    interpolation read it exactly, and a pixel's two values tell where in the picture it was read."""
    rows, cols = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float64)
    return np.stack([rows / (HEIGHT - 1), cols / (WIDTH - 1), np.full_like(rows, 0.5)], axis=-1).astype(np.float32)


def offsets(out: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """The (row, column) offsets in px, shape (height, width), at which each pixel of `out` (a displaced `ramps()`)
    read the picture, away from its borders."""
    rows, cols = np.mgrid[0:HEIGHT, 0:WIDTH]
    return out[..., 0] * (HEIGHT - 1) - rows, out[..., 1] * (WIDTH - 1) - cols


def interior(field: np.ndarray) -> np.ndarray:
    """`field` without its MARGIN px border."""
    return field[MARGIN:-MARGIN, MARGIN:-MARGIN]


@pytest.mark.parametrize("interpolation", ["nearest", "bilinear", "bicubic"])
def test_a_displacement_of_no_amount_leaves_the_picture_unchanged(interpolation: str, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, amount=0.0, interpolation=interpolation)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)  # a new picture, not the input handed back


def test_the_largest_move_anywhere_is_amount_and_every_other_is_smaller() -> None:
    shift_rows, shift_cols = (interior(o) for o in offsets(registry.apply(NAME, ramps(), **SETTINGS)))
    longest = np.hypot(shift_rows, shift_cols).max()
    assert longest <= 8.0 + 1e-3
    assert longest > 6.0  # the largest of the whole picture is 8 px, and the interior holds most of it


def test_doubling_the_amount_doubles_every_move() -> None:
    once = offsets(registry.apply(NAME, ramps(), **SETTINGS))
    twice = offsets(registry.apply(NAME, ramps(), **{**SETTINGS, "amount": 16.0}))
    np.testing.assert_allclose(interior(twice[0]), 2 * interior(once[0]), atol=2e-3)
    np.testing.assert_allclose(interior(twice[1]), 2 * interior(once[1]), atol=2e-3)


def test_rows_and_columns_are_moved_by_two_independent_fields() -> None:
    shift_rows, shift_cols = (interior(o) for o in offsets(registry.apply(NAME, ramps(), **SETTINGS)))
    assert shift_rows.std() > 1.0
    assert shift_cols.std() > 1.0  # both directions really move
    assert abs(np.corrcoef(shift_rows.ravel(), shift_cols.ravel())[0, 1]) < 0.25


def test_a_larger_scale_moves_neighbouring_pixels_more_alike() -> None:
    def roughness(scale: float) -> float:
        shift_cols = offsets(registry.apply(NAME, ramps(), **{**SETTINGS, "scale": scale}))[1]
        return float(np.abs(np.diff(interior(shift_cols), axis=1)).mean())  # px between neighbours' moves

    assert roughness(6.0) > 2 * roughness(24.0)


@pytest.mark.parametrize(
    ("seed", "other", "same"),
    [(3, 3, True), (3, 4, False), (0, 1, False)],
    ids=["the same seed repeats", "another seed differs", "neighbouring seeds differ"],
)
def test_the_seed_picks_the_displacement(seed: int, other: int, same: bool) -> None:
    first = registry.apply(NAME, ramps(), **{**SETTINGS, "seed": seed})
    second = registry.apply(NAME, ramps(), **{**SETTINGS, "seed": other})
    assert np.array_equal(first, second) is same
    assert same or np.abs(first - second).max() > 0.01


def test_no_persistence_leaves_only_the_coarsest_octave() -> None:
    plain = registry.apply(NAME, ramps(), **{**SETTINGS, "octaves": 1})
    flat = registry.apply(NAME, ramps(), **{**SETTINGS, "octaves": 4, "persistence": 0.0})
    np.testing.assert_allclose(flat, plain, atol=1e-6)
    rich = registry.apply(NAME, ramps(), **{**SETTINGS, "octaves": 4, "persistence": 0.8})
    assert np.abs(rich - plain).max() > 0.01  # and with persistence the finer octaves do show


def test_lacunarity_sets_how_much_finer_each_octave_is() -> None:
    fine = registry.apply(NAME, ramps(), **{**SETTINGS, "octaves": 3, "lacunarity": 3.0})
    coarse = registry.apply(NAME, ramps(), **{**SETTINGS, "octaves": 3, "lacunarity": 1.0})
    assert np.abs(fine - coarse).max() > 0.01


def test_a_lone_bright_pixel_never_lands_farther_than_amount() -> None:
    """'nearest' puts each pixel on the closest pixel of the picture it reads, so a lit pixel can be half a pixel
    (0.71 diagonally) farther than the 8 px of the move."""
    dot = np.zeros((101, 101, 3), np.float32)
    dot[50, 50] = 1.0
    out = registry.apply(NAME, dot, **{**SETTINGS, "scale": 30.0}, interpolation="nearest")[..., 0]
    lit = np.argwhere(out == 1.0)
    assert len(lit) >= 1
    assert np.hypot(*(lit - 50).T).max() <= 8.0 + 0.71
    assert np.hypot(*(lit - 50).T).max() > 0.0  # and the dot has really moved


@pytest.mark.parametrize("edge", ["nearest", "mirror", "reflect", "wrap"])
def test_the_edge_rule_only_matters_within_amount_of_the_border(edge: str) -> None:
    reference = registry.apply(NAME, ramps(), **SETTINGS, edge="reflect")
    out = registry.apply(NAME, ramps(), **SETTINGS, edge=edge)
    np.testing.assert_allclose(interior(out), interior(reference), atol=1e-5)
    assert edge == "reflect" or np.abs(out - reference).max() > 1e-3  # a ramp changes 0.005 per px
