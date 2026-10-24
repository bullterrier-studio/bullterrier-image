"""Behaviour of the `slice_shift` effect: horizontal strips of the picture slid sideways, wrapping around."""

import itertools

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "slice_shift"


def row_shifts(before: np.ndarray, after: np.ndarray) -> list[int]:
    """How far each row of `after` is rolled to the right from the same row of `before`, found by trying every roll.
    The picture must be noisy enough that no two rolls of a row agree."""
    width = before.shape[1]
    shifts = []
    for row_before, row_after in zip(before, after, strict=True):
        matching = [s for s in range(width) if np.array_equal(np.roll(row_before, s, axis=0), row_after)]
        assert len(matching) == 1, "the row is not a roll of the original"
        shifts.append(matching[0] if matching[0] <= width // 2 else matching[0] - width)
    return shifts


@pytest.mark.parametrize("seed", [0, 1, 7])
@pytest.mark.parametrize(("slices", "amount"), [(1, 0.3), (12, 0.1), (96, 1.0)])
def test_every_row_is_a_wrapped_slide_of_the_original_row_and_keeps_its_pixels(slices, amount, seed, photo) -> None:
    out = registry.apply(NAME, photo, slices=slices, amount=amount, seed=seed)
    shifts = row_shifts(photo, out)  # raises when a row is not a roll of its original
    assert max(abs(s) for s in shifts) <= round(amount * 128)
    assert np.array_equal(np.sort(out, axis=1), np.sort(photo, axis=1))


@pytest.mark.parametrize("seed", [0, 1, 2, 3])
@pytest.mark.parametrize(("slices", "fewest_runs"), [(1, 1), (5, 3), (20, 10)])
def test_the_rows_move_in_at_most_as_many_strips_as_asked_and_nearly_that_many(
    slices, fewest_runs, seed, photo
) -> None:
    """Strips are told apart by their shift. Two neighbours that happen to draw the same shift read as one run, so the
    runs of rows with an equal shift are at most `slices`, and at amount 1 (255 possible shifts) far more than half."""
    shifts = row_shifts(photo, registry.apply(NAME, photo, slices=slices, amount=1.0, seed=seed))
    runs = 1 + sum(a != b for a, b in itertools.pairwise(shifts))
    assert fewest_runs <= runs <= slices


def test_slides_go_both_ways(photo) -> None:
    shifts = row_shifts(photo, registry.apply(NAME, photo, slices=96, amount=0.5, seed=4))
    assert min(shifts) < 0 < max(shifts)


def test_the_seed_decides_the_strips_and_the_slides(photo) -> None:
    first = registry.apply(NAME, photo, slices=30, amount=0.2, seed=5)
    np.testing.assert_array_equal(first, registry.apply(NAME, photo, slices=30, amount=0.2, seed=5))
    assert not np.array_equal(first, registry.apply(NAME, photo, slices=30, amount=0.2, seed=6))


@pytest.mark.parametrize("seed", range(10))
def test_more_strips_than_rows_gives_every_row_its_own_slide(seed: int) -> None:
    """A wide noisy picture of five rows, so two rows drawing the same slide is a one in two thousand chance."""
    noise = np.random.default_rng(0).random((5, 1024, 3), dtype=np.float32)
    shifts = row_shifts(noise, registry.apply(NAME, noise, slices=500, amount=1.0, seed=seed))
    assert len(set(shifts)) == 5


def test_the_slides_use_the_whole_range_up_to_amount(photo) -> None:
    """With a strip per row, the 96 draws from -26..26 px (0.2 of 128, rounded) come near the ends."""
    shifts = row_shifts(photo, registry.apply(NAME, photo, slices=96, amount=0.2, seed=2))
    assert 0.8 * 26 < max(abs(s) for s in shifts) <= 26
