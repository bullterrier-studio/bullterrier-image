"""Behaviour of the `melt` effect: bright parts of each column are pulled down, by an amount that varies by column."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "melt"
INTERPOLATIONS = ["nearest", "bilinear", "bicubic"]
EDGES = ["reflect", "nearest", "mirror", "wrap"]
HEIGHT, WIDTH = 120, 160


def ramp() -> np.ndarray:
    """A picture whose value is its row's brightness, row / (height - 1), in every column and channel."""
    column = np.linspace(0, 1, HEIGHT, dtype=np.float32)
    return np.broadcast_to(column[:, None, None], (HEIGHT, WIDTH, 3)).copy()


def pull(settings: dict) -> np.ndarray:
    """How many rows each pixel of the ramp picture was pulled down: its row minus the row its value came from,
    (h, w). The ramp's value is the row number / (height - 1), so the value names the source row."""
    out = registry.apply(NAME, ramp(), **settings)
    return np.arange(HEIGHT)[:, None] - out[..., 0].astype(np.float64) * (HEIGHT - 1)


@pytest.mark.parametrize("interpolation", INTERPOLATIONS)
def test_amount_zero_leaves_the_picture_unchanged(interpolation: str, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, amount=0.0, interpolation=interpolation)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)


@pytest.mark.parametrize("edge", EDGES)
@pytest.mark.parametrize("interpolation", INTERPOLATIONS)
def test_columns_never_mix(interpolation: str, edge: str, photo: np.ndarray) -> None:
    """Replacing one column of the picture changes that column of the result and no other."""
    other = photo.copy()
    other[:, 40] = 1 - photo[:, 40]
    settings = {"amount": 30.0, "scale": 6.0, "seed": 2, "interpolation": interpolation, "edge": edge}
    before, after = registry.apply(NAME, photo, **settings), registry.apply(NAME, other, **settings)
    np.testing.assert_allclose(np.delete(before, 40, axis=1), np.delete(after, 40, axis=1), atol=1e-6)
    assert np.abs(before[:, 40] - after[:, 40]).max() > 0.1


def test_black_pixels_stay_black(photo: np.ndarray) -> None:
    """A pixel of no brightness is not pulled at all, so it keeps its place and stays black."""
    dark = photo.copy()
    black = np.random.default_rng(1).random(photo.shape[:2]) < 0.3
    dark[black] = 0.0
    out = registry.apply(NAME, dark, amount=25.0, interpolation="bilinear", seed=3)
    np.testing.assert_allclose(out[black], 0.0, atol=1e-6)
    assert np.abs(out[~black] - dark[~black]).max() > 0.05  # and the rest does move


@pytest.mark.parametrize("amount", [1.0, 10.0, 40.0])
def test_every_pixel_reads_from_its_own_column_at_or_above_its_row_and_at_most_amount_px_up(amount: float) -> None:
    moved = pull({"amount": amount, "scale": 8.0, "seed": 1, "interpolation": "nearest"})
    assert moved.min() > -1e-4  # never from below (the slack is float32 rounding of the ramp's values)
    assert moved.max() <= amount + 0.5 + 1e-4  # and never further than `amount` (the half is nearest-pixel rounding)
    assert moved.max() > 0.8 * amount  # some drip runs nearly the full amount


def test_the_pull_is_proportional_to_the_brightness_in_each_column() -> None:
    """On a smooth ramp read bilinearly the pull of a pixel is its column's drip length times its brightness, so
    pull / brightness is the same all the way down a column, whatever the drip: checked in rows clear of the border."""
    moved = pull({"amount": 40.0, "scale": 8.0, "seed": 1, "interpolation": "bilinear"})
    brightness = (np.arange(HEIGHT) / (HEIGHT - 1))[:, None]
    per_column = moved[40:100] / brightness[40:100]
    np.testing.assert_allclose(per_column, np.broadcast_to(per_column[:1], per_column.shape), atol=1e-4)
    assert per_column.max() > 20.0  # a drip of real length (a short column's would still pass the check above)
    assert per_column.min() < per_column.max() / 2  # and columns differ


@pytest.mark.parametrize(
    ("channel", "weight"),
    [(0, 0.2126), (1, 0.7152), (2, 0.0722)],
    ids=["red", "green", "blue"],
)
def test_brightness_is_the_rec_709_luminance_so_a_colour_is_pulled_by_its_share_of_white(
    channel: int, weight: float
) -> None:
    """A ramp in one channel only has the luminance weight x its value, so it is pulled by that share of what the grey
    ramp is pulled (same drips, bilinear, rows clear of the border)."""
    settings = {"amount": 40.0, "scale": 8.0, "seed": 1, "interpolation": "bilinear"}
    coloured = np.zeros((HEIGHT, WIDTH, 3), np.float32)
    coloured[..., channel] = ramp()[..., 0]
    out = registry.apply(NAME, coloured, **settings)[..., channel].astype(np.float64)
    moved = np.arange(HEIGHT)[:, None] - out * (HEIGHT - 1)
    np.testing.assert_allclose(moved[40:100], weight * pull(settings)[40:100], atol=1e-3)


def test_a_larger_scale_makes_wider_drips() -> None:
    """The drip length of a column is the pull in the brightest row; neighbouring columns' lengths agree more as
    `scale` (the drip width) grows."""

    def neighbour_correlation(scale: float) -> float:
        lengths = pull({"amount": 40.0, "scale": scale, "seed": 1, "interpolation": "bilinear"})[-1]
        return float(np.corrcoef(lengths[:-1], lengths[1:])[0, 1])

    assert neighbour_correlation(1.5) < neighbour_correlation(8.0) < neighbour_correlation(40.0)
    assert neighbour_correlation(40.0) > 0.95


def test_edge_matters_only_in_the_rows_pulled_from_above_the_picture() -> None:
    """Pixels in the top `amount` rows may read from above the picture, where `edge` decides what there is."""
    photo = np.random.default_rng(4).random((HEIGHT, WIDTH, 3), dtype=np.float32)
    settings = {"amount": 30.0, "scale": 8.0, "seed": 1, "interpolation": "bilinear"}
    reflect, wrap = (registry.apply(NAME, photo, edge=edge, **settings) for edge in ("reflect", "wrap"))
    np.testing.assert_allclose(reflect[30:], wrap[30:], atol=1e-6)
    assert np.abs(reflect[:30] - wrap[:30]).max() > 0.05


def test_interpolation_decides_how_a_pull_between_pixels_is_read() -> None:
    photo = np.random.default_rng(4).random((HEIGHT, WIDTH, 3), dtype=np.float32)
    nearest, bilinear = (
        registry.apply(NAME, photo, amount=30.0, seed=1, interpolation=kind) for kind in ("nearest", "bilinear")
    )
    assert np.abs(nearest - bilinear).max() > 0.05
    assert set(np.unique(nearest)) <= set(np.unique(photo))  # nearest only ever copies pixels


def test_the_same_seed_gives_the_same_drips_and_another_seed_other_drips(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, seed=1), registry.apply(NAME, photo, seed=1))
    assert np.abs(registry.apply(NAME, photo, seed=1) - registry.apply(NAME, photo, seed=2)).max() > 0.05
