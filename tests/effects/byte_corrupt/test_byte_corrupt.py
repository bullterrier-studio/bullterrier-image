"""Behaviour of the `byte_corrupt` effect: damaged coefficients in the picture's JPEG, as a corrupted file shows."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "byte_corrupt"
HEIGHT, WIDTH = 64, 128
SEEDS = range(6)


@pytest.fixture
def smooth() -> np.ndarray:
    """A gentle colour gradient well inside 0..1, so that damage never runs into the ends of the range and hides."""
    y, x = np.mgrid[0:HEIGHT, 0:WIDTH].astype(np.float32)
    return 0.3 + 0.4 * np.stack([x / WIDTH, y / HEIGHT, (x + y) / (WIDTH + HEIGHT)], axis=-1)


def differing(before: np.ndarray, after: np.ndarray) -> np.ndarray:
    return (before != after).any(axis=-1)


@pytest.mark.parametrize("quality", [10, 50, 90])
def test_no_hits_is_the_picture_a_plain_jpeg_at_that_quality_gives(photo, quality) -> None:
    np.testing.assert_array_equal(
        registry.apply(NAME, photo, hits=0, quality=quality), registry.apply("jpeg_crush", photo, quality=quality)
    )


@pytest.mark.parametrize("seed", SEEDS)
def test_a_dc_hit_carries_on_to_every_later_block_of_its_row_and_to_no_other_row(smooth, seed) -> None:
    """DC values are coded as differences from the block before, so damage to one moves all that follow in the row:
    the changed pixels run, from the first changed column on, to the picture's right edge, within one band of rows (a
    luma block is 8 rows tall, a chroma block, covering 2 x 2 luma blocks, 16)."""
    clean = registry.apply("jpeg_crush", smooth, quality=30)
    out = registry.apply(NAME, smooth, quality=30, hits=1, dc_share=1.0, seed=seed)
    changed = differing(clean, out)
    columns, rows = np.flatnonzero(changed.any(axis=0)), np.flatnonzero(changed.any(axis=1))
    assert np.array_equal(columns, np.arange(columns[0], WIDTH))  # from the first changed column to the last
    assert rows[0] // 16 == rows[-1] // 16  # inside one aligned band of 16 rows


@pytest.mark.parametrize("seed", SEEDS)
def test_an_ac_hit_stays_inside_one_block_of_the_8_px_grid(smooth, seed) -> None:
    """A hit on any other coefficient changes only its own block: 8 x 8 pixels of luma, or 16 x 16 for the colour
    planes, whose blocks cover four luma blocks. Either way the changed pixels fit in one aligned 16 x 16 cell."""
    clean = registry.apply("jpeg_crush", smooth, quality=30)
    out = registry.apply(NAME, smooth, quality=30, hits=1, dc_share=0.0, seed=seed)
    rows, columns = np.nonzero(differing(clean, out))
    assert len(rows) > 0
    assert rows.min() // 16 == rows.max() // 16
    assert columns.min() // 16 == columns.max() // 16


def test_more_hits_damage_more_of_the_picture(smooth) -> None:
    clean = registry.apply("jpeg_crush", smooth, quality=30)
    few = differing(clean, registry.apply(NAME, smooth, quality=30, hits=3, seed=1)).sum()
    many = differing(clean, registry.apply(NAME, smooth, quality=30, hits=60, seed=1)).sum()
    assert 0 < few < many


def test_a_larger_magnitude_changes_the_pixels_further(smooth) -> None:
    clean = registry.apply("jpeg_crush", smooth, quality=30)
    small = np.abs(clean - registry.apply(NAME, smooth, quality=30, hits=1, magnitude=1, dc_share=1.0, seed=2))
    large = np.abs(clean - registry.apply(NAME, smooth, quality=30, hits=1, magnitude=40, dc_share=1.0, seed=2))
    assert 0 < small.max() < large.max()


def test_detail_hits_never_move_the_average_of_a_block_and_dc_hits_do_in_both_directions(smooth) -> None:
    """The mean of the picture is the DC values' business alone: hits on the other 63 coefficients of each block change
    its pattern and leave its mean, so the picture's mean, where nothing is clamped (small steps, a mid-range picture).
    A DC hit shifts it up or down, by the sign of the hit."""
    clean = registry.apply("jpeg_crush", smooth, quality=90)
    detail = registry.apply(NAME, smooth, quality=90, hits=300, magnitude=1, dc_share=0.0, seed=1)
    assert not np.array_equal(detail, clean)
    np.testing.assert_allclose((detail - clean).mean(), 0, atol=1e-6)
    moves = [
        (registry.apply(NAME, smooth, quality=90, hits=1, dc_share=1.0, seed=seed) - clean).sum() for seed in range(12)
    ]
    assert min(moves) < 0 < max(moves)
