"""Behaviour of the `equalize` effect."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "equalize"
NO_CLIP = 256  # `limit` at its maximum: no bin of at most 256 can reach 256 times the flat height, so nothing clips


def value_of(rgb: np.ndarray) -> np.ndarray:
    """The brightness `equalize` works on, from its documented definition: the largest of red, green and blue."""
    return rgb.max(axis=-1)


def random_colours(seed: int, size: int = 16) -> np.ndarray:
    return np.random.default_rng(seed).uniform(0.05, 1.0, (size, size, 3)).astype(np.float32)


def grey_from_bins(bin_of_pixel: np.ndarray, bins: int) -> np.ndarray:
    """A grey picture whose pixels sit in the middle of the given histogram bins (never on a bin edge)."""
    return np.repeat(((bin_of_pixel + 0.5) / bins).astype(np.float32)[..., None], 3, axis=-1)


def classic_equalisation(value: np.ndarray, bins: int) -> np.ndarray:
    """Histogram equalisation by its definition (Gonzalez and Woods): a pixel goes to the share of all pixels whose
    bin is not above its own. Counted pair by pair, not with a cumulative histogram."""
    index = np.minimum((value * bins).astype(int), bins - 1).ravel()
    share = (index[None, :] <= index[:, None]).mean(axis=1)
    return share.reshape(value.shape).astype(np.float32)


@pytest.mark.parametrize("bins", [256, 32, 5], ids=["256 bins", "32 bins", "5 bins"])
@pytest.mark.parametrize("seed", [0, 1], ids=["picture 0", "picture 1"])
def test_one_tile_without_a_clip_limit_is_classic_histogram_equalisation(seed: int, bins: int) -> None:
    rgb = random_colours(seed)
    new_value = classic_equalisation(value_of(rgb), bins)
    expected = rgb * (new_value / value_of(rgb))[..., None]  # the colour is scaled, so hue and saturation stay
    out = registry.apply(NAME, rgb, tiles=1, limit=NO_CLIP, bins=bins)
    np.testing.assert_allclose(out, expected, atol=1e-5)


@pytest.mark.parametrize("bins", [8, 20, 256])
def test_a_clip_limit_of_one_makes_the_histogram_flat_so_every_pixel_goes_one_bin_up_the_uniform_ramp(
    bins: int,
) -> None:
    """Clipped at the uniform height, every bin is exactly that tall: the cumulative histogram is (k + 1) / bins."""
    bin_of_pixel = np.random.default_rng(3).integers(0, bins, (12, 12))
    out = registry.apply(NAME, grey_from_bins(bin_of_pixel, bins), tiles=1, limit=1.0, bins=bins)
    np.testing.assert_allclose(out[..., 0], (bin_of_pixel + 1) / bins, atol=1e-5)


@pytest.mark.parametrize("limit", [1.5, 2.0, 4.0])
def test_the_clip_limit_bounds_how_steeply_the_tones_are_pulled_apart(limit: float) -> None:
    """One bin holds 600 of 660 pixels and every other bin 4. Without a limit the output jumps by 600/660 at the
    crowded bin; with one, no step between neighbouring bins is above `limit` times the uniform step 1 / bins."""
    bins = 16
    bin_of_pixel = np.concatenate([np.repeat(np.arange(bins), 4), np.full(600 - 4, 5)])
    grey = grey_from_bins(bin_of_pixel[None, :], bins)  # one row of 660 pixels

    def levels(clip: float) -> np.ndarray:
        out = registry.apply(NAME, grey, tiles=1, limit=clip, bins=bins)[0, :, 0]
        return np.array([out[bin_of_pixel == b][0] for b in range(bins)])

    unclipped, clipped = levels(NO_CLIP), levels(limit)
    assert np.diff(unclipped).max() > 0.8  # the crowded bin takes most of the range ...
    assert np.diff(clipped).max() <= limit / bins + 1e-5  # ... until the limit stops it
    assert clipped[-1] == pytest.approx(1.0, abs=1e-5)  # the clipped mass is redistributed, not lost
    assert np.diff(clipped).min() > 0  # and nothing is flattened


def test_hue_and_saturation_stay_while_the_brightness_changes() -> None:
    rgb = random_colours(5)
    out = registry.apply(NAME, rgb, tiles=1, limit=NO_CLIP)
    assert not np.allclose(out, rgb, atol=0.05)
    np.testing.assert_allclose(out / value_of(out)[..., None], rgb / value_of(rgb)[..., None], atol=1e-5)


def test_black_stays_black() -> None:
    rgb = random_colours(6)
    rgb[3, 4] = 0.0
    assert (registry.apply(NAME, rgb, tiles=1, limit=NO_CLIP)[3, 4] == 0.0).all()


def test_bins_sets_how_many_distinct_brightnesses_come_out() -> None:
    rgb = np.repeat(np.linspace(0.01, 0.99, 500, dtype=np.float32)[None, :, None], 3, axis=-1)
    for bins in (2, 7, 64):
        out = registry.apply(NAME, rgb, tiles=1, limit=NO_CLIP, bins=bins)
        assert len(np.unique(out[..., 0].round(4))) == bins  # rounded: float32 noise


def test_tiles_equalise_each_region_on_its_own() -> None:
    """A dark left half (a ramp over columns 0..63, 1.8 bins apart) and a bright right half. Over all pixels column x
    of the dark half has (x + 1) / 128 of the pixels at or below it; over the left tiles alone (x + 1) / 64. Columns
    0..31 lie before the left tiles' centre (column 31.5), where no neighbouring tile is blended in."""
    ramp = np.linspace(0.01, 0.45, 64, dtype=np.float32)
    picture = np.repeat(
        np.concatenate([np.tile(ramp, (32, 1)), 0.5 + np.tile(ramp, (32, 1))], axis=1)[..., None], 3, -1
    )
    one = registry.apply(NAME, picture, tiles=1, limit=NO_CLIP, bins=256)
    two = registry.apply(NAME, picture, tiles=2, limit=NO_CLIP, bins=256)
    columns = np.arange(32)
    for row in (0, 15, 31):
        np.testing.assert_allclose(one[row, :32, 0], (columns + 1) / 128, atol=1e-5)
        np.testing.assert_allclose(two[row, :32, 0], (columns + 1) / 64, atol=1e-5)


@pytest.mark.parametrize("transposed", [False, True], ids=["tiles side by side", "tiles one above the other"])
def test_neighbouring_tiles_blend_with_the_distance_to_their_centres(transposed: bool) -> None:
    """A picture 16 rows by 32 columns, 2 x 2 tiles of 8 x 16 pixels: the left half all one dark grey `a`, the right
    half all one bright grey `c`. The left tiles map every `a` to 1 and the right tiles map `a` (below all their
    pixels) to 0, so a left pixel at column x is 1 - w with w the share of the way from the left tile's centre
    (column 7.5) to the right tile's (column 23.5): (x - 7.5) / 16, held at 0 and 1 beyond the centres. Right pixels
    map to 1 in both tiles. The same holds with the picture turned on its side."""
    dark, bright = (64 + 0.5) / 256, (192 + 0.5) / 256
    picture = np.full((16, 32, 3), bright, np.float32)
    picture[:, :16] = dark
    expected = np.ones((16, 32))
    expected[:, :16] = 1.0 - np.clip((np.arange(16) - 7.5) / 16, 0, 1)
    if transposed:
        picture, expected = picture.transpose(1, 0, 2), expected.T
    out = registry.apply(NAME, np.ascontiguousarray(picture), tiles=2, limit=NO_CLIP, bins=256)[..., 0]
    np.testing.assert_allclose(out, expected, atol=1e-5)
