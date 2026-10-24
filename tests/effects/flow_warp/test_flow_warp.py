"""Behaviour of the `flow_warp` effect: the picture carried along a noise-driven flow field, `amount` px in `steps`
steps, so it smears into streaks where the flow converges."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "flow_warp"
HEIGHT, WIDTH = 200, 300
MARGIN = 20  # px from each border inside which nothing reads beyond the picture (amount 8 and bicubic's reach)
SETTINGS = {"amount": 8.0, "steps": 4, "scale": 12.0, "octaves": 2, "variation": 90.0, "seed": 3}


def ramps() -> np.ndarray:
    """A picture whose red is its row / (height - 1) and green its column / (width - 1): linear, so bilinear and bicubic
    interpolation read it exactly however often it is resampled, and a pixel's two values tell where it was read."""
    rows, cols = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float64)
    return np.stack([rows / (HEIGHT - 1), cols / (WIDTH - 1), np.full_like(rows, 0.5)], axis=-1).astype(np.float32)


def offsets(out: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """The (row, column) offsets in px, away from the borders, at which each pixel of `out` (a warped `ramps()`) read
    the picture."""
    rows, cols = np.mgrid[0:HEIGHT, 0:WIDTH]
    return interior(out[..., 0] * (HEIGHT - 1) - rows), interior(out[..., 1] * (WIDTH - 1) - cols)


def interior(field: np.ndarray) -> np.ndarray:
    """`field` without its MARGIN px border."""
    return field[MARGIN:-MARGIN, MARGIN:-MARGIN]


@pytest.mark.parametrize("interpolation", ["nearest", "bilinear", "bicubic"])
def test_a_flow_of_no_amount_leaves_the_picture_unchanged(interpolation: str, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, amount=0.0, interpolation=interpolation)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)  # a new picture, not the input handed back


@pytest.mark.parametrize("steps", [1, 7])
@pytest.mark.parametrize(
    ("angle", "moved_by"),
    [(0.0, (0.0, 10.0)), (90.0, (10.0, 0.0)), (180.0, (0.0, -10.0)), (-90.0, (-10.0, 0.0)), (30.0, (5.0, 8.660254))],
    ids=["right", "down", "left", "up", "30 degrees clockwise from right"],
)
def test_with_no_variation_the_whole_picture_is_carried_amount_px_along_the_angle(
    steps: int, angle: float, moved_by: tuple[float, float]
) -> None:
    """The flow is the same everywhere, so after any number of steps a pixel shows what was `amount` = 10 px behind
    it: moved_by is the (rows, columns) the picture travels, from the angle measured clockwise from pointing right
    (rows run down), by hand: 10 x (sin angle, cos angle)."""
    out = registry.apply(NAME, ramps(), amount=10.0, steps=steps, angle=angle, variation=0.0, interpolation="bilinear")
    shift_rows, shift_cols = offsets(out)
    np.testing.assert_allclose(shift_rows, -moved_by[0], atol=2e-3)  # a pixel reads where the content came from
    np.testing.assert_allclose(shift_cols, -moved_by[1], atol=2e-3)


@pytest.mark.parametrize(
    ("angle", "lands_at"),
    [(0.0, (50, 62)), (90.0, (62, 50)), (180.0, (50, 38)), (-90.0, (38, 50))],
    ids=["right", "down", "left", "up"],
)
def test_a_lone_bright_pixel_travels_along_the_flow_by_amount(angle: float, lands_at: tuple[int, int]) -> None:
    dot = np.zeros((101, 101, 3), np.float32)
    dot[50, 50] = 1.0
    out = registry.apply(NAME, dot, amount=12.0, steps=4, angle=angle, variation=0.0, interpolation="nearest")[..., 0]
    assert np.argwhere(out == 1.0).tolist() == [list(lands_at)]
    assert out.sum() == 1.0


@pytest.mark.parametrize("steps", [1, 3, 8])
def test_a_bright_square_in_a_turning_flow_never_lands_farther_than_amount(steps: int) -> None:
    """The path is `steps` steps of `amount` / steps px, so content ends at most `amount` from where it began: a 5 x 5
    white square, its corners 2.83 px from its middle, shows no pixel farther than 8 + 2.83 px from that middle;
    'nearest' puts each step on a pixel, half a pixel (0.71 diagonally) off at most per step."""
    square = np.zeros((101, 101, 3), np.float32)
    square[48:53, 48:53] = 1.0
    out = registry.apply(
        NAME, square, **{**SETTINGS, "steps": steps, "variation": 200.0, "scale": 30.0}, interpolation="nearest"
    )[..., 0]
    lit = np.argwhere(out == 1.0)
    assert len(lit) >= 1
    assert np.hypot(*(lit - 50).T).max() <= 8.0 + 2.83 + 0.71 * steps


def test_with_one_step_every_pixel_moves_exactly_amount_in_the_direction_of_its_own_flow() -> None:
    shift_rows, shift_cols = offsets(registry.apply(NAME, ramps(), **{**SETTINGS, "steps": 1, "variation": 180.0}))
    np.testing.assert_allclose(np.hypot(shift_rows, shift_cols), 8.0, atol=2e-3)
    assert shift_rows.std() > 2.0
    assert shift_cols.std() > 2.0  # and the directions differ all over the picture


def test_in_a_turning_flow_more_steps_follow_the_curve_and_end_nearer_than_amount() -> None:
    chord = np.hypot(
        *offsets(registry.apply(NAME, ramps(), **{**SETTINGS, "steps": 1, "variation": 360.0, "amount": 16.0}))
    )
    curved = np.hypot(
        *offsets(registry.apply(NAME, ramps(), **{**SETTINGS, "steps": 16, "variation": 360.0, "amount": 16.0}))
    )
    assert curved.max() <= 16.0 + 2e-3  # the path is 16 px long, so no pixel ends farther away
    assert curved.mean() < 0.9 * chord.mean()  # and the turning flow bends it back


def test_a_nearly_straight_flow_carries_every_pixel_far_where_a_displacement_moves_most_of_them_little() -> None:
    """The difference from `displace`: there the move is as long as the noise says (mostly well under `amount`); here
    every pixel is carried the full `amount` along the flow."""
    gentle = {**SETTINGS, "variation": 10.0}
    flowing = np.hypot(*offsets(registry.apply(NAME, ramps(), **gentle)))
    displaced = np.hypot(*offsets(registry.apply("displace", ramps(), amount=8.0, scale=12.0, octaves=2, seed=3)))
    assert flowing.min() > 0.9 * 8.0
    assert np.percentile(displaced, 20) < 0.5 * 8.0


def test_a_larger_scale_turns_the_flow_more_slowly_from_pixel_to_pixel() -> None:
    def roughness(scale: float) -> float:
        out = registry.apply(NAME, ramps(), **{**SETTINGS, "steps": 1, "variation": 180.0, "scale": scale})
        return float(np.abs(np.diff(offsets(out)[1], axis=1)).mean())  # px between neighbours' column moves

    assert roughness(6.0) > 2 * roughness(24.0)


@pytest.mark.parametrize(
    ("seed", "other", "same"),
    [(3, 3, True), (3, 4, False), (0, 1, False)],
    ids=["the same seed repeats", "another seed differs", "neighbouring seeds differ"],
)
def test_the_seed_picks_the_flow(seed: int, other: int, same: bool) -> None:
    first = registry.apply(NAME, ramps(), **{**SETTINGS, "seed": seed})
    second = registry.apply(NAME, ramps(), **{**SETTINGS, "seed": other})
    assert np.array_equal(first, second) is same
    assert same or np.abs(first - second).max() > 0.01


def test_no_persistence_leaves_only_the_coarsest_octave_of_the_flow() -> None:
    plain = registry.apply(NAME, ramps(), **{**SETTINGS, "octaves": 1})
    flat = registry.apply(NAME, ramps(), **{**SETTINGS, "octaves": 4, "persistence": 0.0})
    np.testing.assert_allclose(flat, plain, atol=1e-6)
    rich = registry.apply(NAME, ramps(), **{**SETTINGS, "octaves": 4, "persistence": 0.8})
    assert np.abs(rich - plain).max() > 0.01


def test_lacunarity_sets_how_much_finer_each_octave_of_the_flow_is() -> None:
    fine = registry.apply(NAME, ramps(), **{**SETTINGS, "octaves": 3, "lacunarity": 3.0})
    coarse = registry.apply(NAME, ramps(), **{**SETTINGS, "octaves": 3, "lacunarity": 1.0})
    assert np.abs(fine - coarse).max() > 0.01


@pytest.mark.parametrize("edge", ["nearest", "mirror", "reflect", "wrap"])
def test_the_edge_rule_only_matters_within_amount_of_the_border(edge: str) -> None:
    reference = registry.apply(NAME, ramps(), **SETTINGS, edge="reflect")
    out = registry.apply(NAME, ramps(), **SETTINGS, edge=edge)
    np.testing.assert_allclose(interior(out[..., :2]), interior(reference[..., :2]), atol=1e-5)
    assert edge == "reflect" or np.abs(out - reference).max() > 1e-3  # a ramp changes 0.005 per px
