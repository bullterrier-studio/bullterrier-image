"""Behaviour of the `signal_noise` effect: bands of static and short dropout streaks, as from a failing signal."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "signal_noise"
NONE = {"bands": 0, "dropouts": 0}  # neither damage: every other setting is then without effect


def changed_rows(before: np.ndarray, after: np.ndarray) -> np.ndarray:
    return np.flatnonzero((before != after).any(axis=(1, 2)))


def runs(rows: np.ndarray) -> list[np.ndarray]:
    """`rows` (sorted indices) cut into runs of consecutive indices."""
    return np.split(rows, np.flatnonzero(np.diff(rows) > 1) + 1)


@pytest.mark.parametrize(
    "settings",
    [NONE, {"bands": 0, "dropouts": 0, "static": 1.0}, {"bands": 5, "static": 0.0, "dropouts": 0}],
    ids=["no bands and no dropouts", "no damage whatever the static", "bands of no static"],
)
def test_no_damage_leaves_the_picture_as_it_is(photo, settings) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, **settings), photo)


@pytest.mark.parametrize("static", [0.25, 0.5, 1.0])
def test_a_band_mixes_grey_static_into_whole_rows_by_the_static_share(photo, static) -> None:
    """Whatever the band holds, `out = (1 - static) * photo + static * grey` with one grey value per pixel in 0..1, so
    `(out - (1 - static) * photo) / static` has equal channels inside 0..1; rows outside the band are the photo's."""
    out = registry.apply(NAME, photo, bands=1, band_height=0.25, static=static, dropouts=0, seed=3)
    rows = changed_rows(photo, out)
    assert len(runs(rows)) == 1  # one band: the grey is random, so every row of it differs
    inside = (out[rows] - (1 - static) * photo[rows]) / static
    np.testing.assert_allclose(inside, inside[..., :1].repeat(3, axis=-1), atol=1e-5)
    assert -1e-5 <= inside.min() < 0.01  # the grey is uniform over 0..1: it comes close to both ends
    assert 0.99 < inside.max() <= 1 + 1e-5
    assert 96 * 0.25 / 2 <= len(rows) <= 96 * 0.25  # between half the tallest band and the tallest


def test_every_band_is_its_own_run_of_rows_and_no_more_bands_than_asked(photo) -> None:
    out = registry.apply(NAME, photo, bands=4, band_height=0.04, static=1.0, dropouts=0, seed=1)
    assert 1 <= len(runs(changed_rows(photo, out))) <= 4


@pytest.mark.parametrize("color", [(1.0, 0.0, 0.0), (0.0, 0.25, 1.0)])
def test_a_dropout_is_a_single_streak_of_the_documented_colour(photo, color) -> None:
    out = registry.apply(NAME, photo, bands=0, dropouts=1, dropout_length=0.5, dropout_height=3, color=color, seed=2)
    changed = (photo != out).any(axis=2)
    rows, columns = np.flatnonzero(changed.any(axis=1)), np.flatnonzero(changed.any(axis=0))
    assert len(rows) == min(3, 96 - rows[0])  # three rows tall, unless cut by the bottom edge
    assert 128 * 0.5 / 2 <= len(columns) <= 128 * 0.5  # between half the longest streak and the longest
    assert np.array_equal(columns, np.arange(columns[0], columns[-1] + 1))
    np.testing.assert_array_equal(out[changed], np.broadcast_to(np.float32(color), (int(changed.sum()), 3)))


def test_more_dropouts_change_more_of_the_picture(photo) -> None:
    few = (photo != registry.apply(NAME, photo, bands=0, dropouts=3, seed=5)).any(axis=2).sum()
    many = (photo != registry.apply(NAME, photo, bands=0, dropouts=40, seed=5)).any(axis=2).sum()
    assert few < many


def test_the_dropouts_do_not_move_when_the_bands_change(photo) -> None:
    """Static and dropouts draw from their own streams of one seed: changing the number of bands moves no dropout."""
    with_bands = registry.apply(NAME, photo, bands=3, static=1.0, dropouts=6, color="#ff0000", seed=4)
    without = registry.apply(NAME, photo, bands=0, dropouts=6, color="#ff0000", seed=4)
    red = (without == np.array([1.0, 0.0, 0.0], np.float32)).all(axis=2)
    np.testing.assert_array_equal(with_bands[red], without[red])
