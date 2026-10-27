"""Behaviour of the `slice_shift` effect: horizontal strips of the picture slid sideways, wrapping around."""

import itertools

import numpy as np
import pytest

from bullterrier_image import RecipeError, fx
from bullterrier_image.core import registry
from bullterrier_image.recipe import Recipe, render

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
@pytest.mark.parametrize(("slices", "amount"), [(1, 38), (12, 13), (96, 128)])
def test_every_row_is_a_wrapped_slide_of_the_original_row_and_keeps_its_pixels(slices, amount, seed, photo) -> None:
    out = registry.apply(NAME, photo, slices=slices, amount=amount, seed=seed)
    shifts = row_shifts(photo, out)  # raises when a row is not a roll of its original
    assert max(abs(s) for s in shifts) <= amount
    assert np.array_equal(np.sort(out, axis=1), np.sort(photo, axis=1))


@pytest.mark.parametrize("seed", [0, 1, 2, 3])
@pytest.mark.parametrize(("slices", "fewest_runs"), [(1, 1), (5, 3), (20, 10)])
def test_the_rows_move_in_at_most_as_many_strips_as_asked_and_nearly_that_many(
    slices, fewest_runs, seed, photo
) -> None:
    """Strips are told apart by their shift. Two neighbours that happen to draw the same shift read as one run, so the
    runs of rows with an equal shift are at most `slices`, and at amount 128 (257 shifts) far more than half."""
    shifts = row_shifts(photo, registry.apply(NAME, photo, slices=slices, amount=128, seed=seed))
    runs = 1 + sum(a != b for a, b in itertools.pairwise(shifts))
    assert fewest_runs <= runs <= slices


def test_one_slice_slides_the_whole_picture_as_one(photo) -> None:
    shifts = row_shifts(photo, registry.apply(NAME, photo, slices=1, amount=38, seed=2))
    assert len(set(shifts)) == 1
    assert shifts[0] != 0


def test_slides_go_both_ways(photo) -> None:
    shifts = row_shifts(photo, registry.apply(NAME, photo, slices=96, amount=64, seed=4))
    assert min(shifts) < 0 < max(shifts)


@pytest.mark.parametrize("seed", range(10))
def test_more_strips_than_rows_gives_every_row_its_own_slide(seed: int) -> None:
    """A wide noisy picture of five rows, so two rows drawing the same slide is a one in two thousand chance."""
    noise = np.random.default_rng(0).random((5, 1024, 3), dtype=np.float32)
    shifts = row_shifts(noise, registry.apply(NAME, noise, slices=500, amount=1024, seed=seed))
    assert len(set(shifts)) == 5


def test_the_slides_use_the_whole_range_up_to_amount(photo) -> None:
    """With a strip per row, the 96 draws from -26..26 px come near the ends."""
    shifts = row_shifts(photo, registry.apply(NAME, photo, slices=96, amount=26, seed=2))
    assert 0.8 * 26 < max(abs(s) for s in shifts) <= 26


# --- bands: a few strips at random places, as in a corrupted video frame (`slices=0` leaves the tear off) ---

BANDS_ONLY = {"slices": 0}


def changed_rows(a: np.ndarray, b: np.ndarray) -> np.ndarray:
    return np.flatnonzero((a != b).any(axis=(1, 2)))


@pytest.mark.parametrize(
    "settings",
    [
        pytest.param({"bands": 0, "amount": 128}, id="no bands"),
        pytest.param({"bands": 12, "amount": 0}, id="amount 0"),
    ],
)
def test_without_tear_and_without_slide_nothing_moves(settings, photo) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, **BANDS_ONLY, seed=2, **settings), photo)


@pytest.mark.parametrize("seed", [0, 1, 2])
def test_bands_move_rows_sideways_and_keep_their_pixels(seed: int, photo) -> None:
    out = registry.apply(NAME, photo, **BANDS_ONLY, bands=7, amount=38, seed=seed)
    assert not np.array_equal(out, photo)
    row_shifts(photo, out)  # raises when a row is not a wrapped slide of its original
    assert np.array_equal(np.sort(out, axis=1), np.sort(photo, axis=1))


def test_only_bands_many_strips_move_the_rest_of_the_picture_stays(photo) -> None:
    """Seven bands of 4 px move at most 28 rows of 96."""
    out = registry.apply(NAME, photo, **BANDS_ONLY, bands=7, amount=64, band_min_height=4, band_max_height=4, seed=1)
    assert 0 < len(changed_rows(out, photo)) <= 28


def test_a_band_is_at_least_one_row_tall_whatever_its_height_limits(photo) -> None:
    rows = [
        len(
            changed_rows(
                registry.apply(
                    NAME, photo, **BANDS_ONLY, bands=1, amount=128, band_min_height=0, band_max_height=0, seed=s
                ),
                photo,
            )
        )
        for s in range(20)
    ]
    assert max(rows) == 1


def test_a_band_is_its_height_in_px_tall(photo) -> None:
    # one band, exactly 24 px (a quarter of the 96 rows), slid by up to half the width
    out = registry.apply(NAME, photo, **BANDS_ONLY, bands=1, amount=64, band_min_height=24, band_max_height=24, seed=1)
    changed = changed_rows(out, photo)
    assert len(changed) == 24
    assert changed[-1] - changed[0] == 23  # one unbroken strip


def test_band_heights_spread_between_their_limits(photo) -> None:
    # one band of 24 to 48 px, under 20 seeds: always 24..48 rows, and both short and tall ones
    heights = [
        len(
            changed_rows(
                registry.apply(
                    NAME, photo, **BANDS_ONLY, bands=1, amount=64, band_min_height=24, band_max_height=48, seed=s
                ),
                photo,
            )
        )
        for s in range(20)
    ]
    assert 24 <= min(heights) < 36 < max(heights) <= 48


def test_a_band_slides_sideways_up_to_amount_either_way(photo) -> None:
    limit = 13  # px
    slides = []
    for seed in range(20):
        out = registry.apply(
            NAME, photo, **BANDS_ONLY, bands=1, amount=limit, band_min_height=10, band_max_height=10, seed=seed
        )
        rows = changed_rows(out, photo)
        slides.append(row_shifts(photo[rows], out[rows])[0] if len(rows) else 0)
    assert all(abs(d) <= limit for d in slides)
    assert min(slides) < 0 < max(slides)  # both directions
    assert max(abs(d) for d in slides) > 0.75 * limit  # and the slides reach most of the way to the limit


def test_bands_come_on_top_of_the_tear_and_leave_the_rows_outside_them_as_the_tear_made_them(photo) -> None:
    tear = registry.apply(NAME, photo, slices=10, amount=38, seed=4)
    both = registry.apply(NAME, photo, slices=10, bands=1, amount=38, band_min_height=24, band_max_height=24, seed=4)
    changed = changed_rows(both, tear)
    assert 0 < len(changed) <= 24
    assert changed[-1] - changed[0] < 24


def test_the_glitch_preset_is_a_few_bands_not_a_tear(photo) -> None:
    out = render(photo, Recipe("t", [fx.slice_shift(preset="glitch")], size=None))
    assert 0 < len(changed_rows(out, photo)) <= 0.5 * photo.shape[0]


def test_a_band_taller_than_the_picture_is_the_whole_picture_sliding_as_one(photo) -> None:
    out = registry.apply(
        NAME, photo, **BANDS_ONLY, bands=1, amount=64, band_min_height=500, band_max_height=500, seed=1
    )
    shifts = row_shifts(photo, out)
    assert len(set(shifts)) == 1
    assert shifts[0] != 0


def test_band_height_limits_given_the_wrong_way_round_are_refused_naming_both() -> None:
    with pytest.raises(RecipeError, match="band_min_height") as raised:
        fx.slice_shift(band_min_height=20, band_max_height=5)
    assert "band_max_height" in str(raised.value)
    assert fx.slice_shift(band_min_height=20, band_max_height=20).params["band_min_height"] == 20  # equal is a range
