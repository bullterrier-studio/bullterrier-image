"""Tests of `ops/dither.py`: error diffusion against the textbook loop, and the threshold tiles against their defining
properties (every rank once, an even spread of the points)."""

from typing import Literal

import numpy as np
import pytest

from bullterrier_image.ops.dither import diffuse, ordered, threshold_tile

Kernel = Literal["floyd_steinberg", "atkinson"]
Tile = Literal["bayer", "blue_noise"]

# The classic kernels, written out here: (dx, dy, weight) of the pixels that receive the error, and the divisor.
FLOYD_STEINBERG = ([(1, 0, 7), (-1, 1, 3), (0, 1, 5), (1, 1, 1)], 16)
ATKINSON = ([(1, 0, 1), (2, 0, 1), (-1, 1, 1), (0, 1, 1), (1, 1, 1), (0, 2, 1)], 8)
BLACK_WHITE = np.array([[0, 0, 0], [1, 1, 1]], np.float32)
KERNELS = {"floyd_steinberg": FLOYD_STEINBERG, "atkinson": ATKINSON}


def textbook(values: np.ndarray, palette: np.ndarray, kernel: Kernel, amount: float) -> np.ndarray:
    """Error diffusion as every description gives it: pixels in reading order, each snapped to the closest palette
    colour, its error passed on to the pixels that follow."""
    taps, divisor = KERNELS[kernel]
    work = values.astype(np.float64).copy()
    height, width = work.shape[:2]
    out = np.zeros((height, width), np.intp)
    for y in range(height):
        for x in range(width):
            chosen = int(np.argmin(((palette - work[y, x]) ** 2).sum(axis=1)))
            out[y, x] = chosen
            error = (work[y, x] - palette[chosen]) * amount
            for dx, dy, weight in taps:
                if 0 <= x + dx < width and y + dy < height:
                    work[y + dy, x + dx] += error * weight / divisor
    return out


@pytest.mark.parametrize("kernel", ["floyd_steinberg", "atkinson"])
@pytest.mark.parametrize("shape", [(1, 1), (1, 9), (9, 1), (7, 5), (24, 31)], ids=lambda s: f"{s[0]}x{s[1]}")
@pytest.mark.parametrize(
    "palette",
    [
        np.array([[0, 0, 0], [1, 1, 1]], np.float32),
        np.array([[0, 0, 0], [1, 0, 0], [0.2, 0.8, 0.4], [1, 1, 1]], np.float32),
    ],
    ids=["black and white", "four colours"],
)
def test_diffusion_gives_what_the_textbook_loop_gives(
    kernel: Kernel, shape: tuple[int, int], palette: np.ndarray
) -> None:
    values = np.random.default_rng(1).random((*shape, 3), dtype=np.float32)
    np.testing.assert_array_equal(
        diffuse(values, palette, method=kernel, amount=1.0), textbook(values, palette, kernel, 1.0)
    )


@pytest.mark.parametrize("kernel", ["floyd_steinberg", "atkinson"])
@pytest.mark.parametrize("amount", [0.0, 0.4, 0.75])
def test_diffusion_passing_on_only_a_share_of_the_error_matches_the_textbook_loop(
    kernel: Kernel, amount: float
) -> None:
    values = np.random.default_rng(2).random((16, 20, 3), dtype=np.float32)
    palette = np.array([[0, 0, 0], [1, 1, 1]], np.float32)
    np.testing.assert_array_equal(
        diffuse(values, palette, method=kernel, amount=amount), textbook(values, palette, kernel, amount)
    )


def test_diffusion_does_not_modify_the_picture() -> None:
    values = np.random.default_rng(3).random((10, 10, 3), dtype=np.float32)
    before = values.copy()
    diffuse(values, np.array([[0, 0, 0], [1, 1, 1]], np.float32), method="atkinson", amount=1.0)
    np.testing.assert_array_equal(values, before)


@pytest.mark.parametrize("method", ["bayer", "blue_noise"])
def test_a_threshold_tile_holds_every_level_once(method: Tile) -> None:
    """The tile is a ranking of its own pixels, so a flat tone lights exactly its share of them."""
    tile = threshold_tile(method)
    count = tile.size
    np.testing.assert_allclose(np.sort(tile.ravel()), (np.arange(count) + 0.5) / count, atol=1e-6)


def test_the_bayer_tile_is_the_published_8_by_8_matrix() -> None:
    published = np.array(
        [
            [0, 32, 8, 40, 2, 34, 10, 42],
            [48, 16, 56, 24, 50, 18, 58, 26],
            [12, 44, 4, 36, 14, 46, 6, 38],
            [60, 28, 52, 20, 62, 30, 54, 22],
            [3, 35, 11, 43, 1, 33, 9, 41],
            [51, 19, 59, 27, 49, 17, 57, 25],
            [15, 47, 7, 39, 13, 45, 5, 37],
            [63, 31, 55, 23, 61, 29, 53, 21],
        ]
    )
    np.testing.assert_allclose(threshold_tile("bayer"), (published + 0.5) / 64, atol=1e-6)


@pytest.mark.parametrize("level", [0.1, 0.5], ids=["sparse points", "half the points"])
def test_the_blue_noise_tile_has_almost_no_low_frequency_energy_unlike_white_noise(level: float) -> None:
    """The defining property of blue noise: the points of any prefix of the ranking carry almost no energy at low
    spatial frequencies (no blotches), where a random ranking's carry plenty. Measured as the share of the pattern's
    spectrum within 0.08 cycles per px of the origin, the origin itself (the average) left out."""
    size = threshold_tile("blue_noise").shape[0]
    radius = np.hypot(np.fft.fftfreq(size)[:, None], np.fft.fftfreq(size)[None, :])
    low = (radius > 0) & (radius < 0.08)

    def low_share(tile: np.ndarray) -> float:
        points = (tile < level).astype(np.float64)
        power = np.abs(np.fft.fft2(points - points.mean())) ** 2
        return float(power[low].sum() / power.sum())

    white = np.random.default_rng(3).permutation(size * size).reshape(size, size) / (size * size)
    assert low_share(threshold_tile("blue_noise")) < 0.1 * low_share(white)


def test_the_tiles_are_read_only_so_nobody_can_change_the_pattern_for_everyone() -> None:
    for method in ("bayer", "blue_noise"):
        assert not threshold_tile(method).flags.writeable


def test_ordered_dithering_of_a_flat_tone_lights_its_share_of_the_pixels() -> None:
    """Grey 0.25 against the black and white palette: a quarter of the pixels of a whole number of tiles go white."""
    tile = threshold_tile("bayer")
    flat = np.full((tile.shape[0] * 3, tile.shape[1] * 2, 3), 0.25, np.float32)
    chosen = ordered(flat, np.array([[0, 0, 0], [1, 1, 1]], np.float32), tile, amount=1.0)
    assert chosen.mean() == pytest.approx(0.25, abs=1e-6)


def naive_ordered(values: np.ndarray, palette: np.ndarray, tile: np.ndarray, amount: float = 1.0) -> np.ndarray:
    """The documented rule pixel by pixel: 1 + round(15 `amount`) candidates, each closest to the value plus what the
    candidates so far owe it, sorted by their first channel, one picked by the tile's level at the pixel."""
    height, width = values.shape[:2]
    n = tile.shape[0]
    brightness = palette[:, 0]
    out = np.zeros((height, width), np.intp)
    for y in range(height):
        for x in range(width):
            owed, candidates = np.zeros(3), []
            steps = 1 + round(amount * 15)
            for _ in range(steps):
                chosen = int(np.argmin(((palette - (values[y, x] + owed)) ** 2).sum(axis=1)))
                candidates.append(chosen)
                owed = owed + values[y, x] - palette[chosen]
            candidates.sort(key=lambda index: (brightness[index], index))
            out[y, x] = candidates[int(tile[y % n, x % n] * steps)]
    return out


PALETTES = {
    "black and white": np.array([[0, 0, 0], [1, 1, 1]], np.float32),
    "primaries, no brightness order": np.array([[0, 0, 1], [1, 0, 0], [0, 1, 0]], np.float32),
    "four colours": np.array([[0.1, 0.1, 0.3], [0.9, 0.4, 0.1], [0.95, 0.9, 0.7], [0.2, 0.6, 0.5]], np.float32),
}


@pytest.mark.parametrize("palette", PALETTES.values(), ids=PALETTES.keys())
@pytest.mark.parametrize("method", ["bayer", "blue_noise"])
def test_ordered_dithering_gives_what_the_documented_rule_gives(palette: np.ndarray, method: Tile) -> None:
    tile = threshold_tile(method)
    values = np.random.default_rng(14).random((9, 13, 3), dtype=np.float32)
    np.testing.assert_array_equal(
        ordered(values, palette, tile, amount=1.0), naive_ordered(values, palette, tile, amount=1.0)
    )


@pytest.mark.parametrize("tone", [0.25, 0.5, 0.75])
def test_ordered_dithering_of_a_flat_tone_lights_its_share_of_the_pixels_whatever_the_pattern(tone: float) -> None:
    flat = np.full((64, 64, 3), tone, np.float32)
    for method in ("bayer", "blue_noise"):
        assert ordered(flat, BLACK_WHITE, threshold_tile(method), amount=1.0).mean() == pytest.approx(tone, abs=1e-6)


def test_a_palette_without_a_brightness_order_still_mixes_to_the_colour() -> None:
    """(0.2, 0.3, 0.5) is 20 % red, 30 % green, 50 % blue of the three primaries, which sum to it: the pixels of a
    whole tile come out in those shares, to within the 1/16 steps the mix is made in."""
    primaries = np.array([[1, 0, 0], [0, 1, 0], [0, 0, 1]], np.float32)
    flat = np.broadcast_to(np.array([0.2, 0.3, 0.5], np.float32), (64, 64, 3))
    shares = (
        np.bincount(ordered(flat, primaries, threshold_tile("blue_noise"), amount=1.0).ravel(), minlength=3) / 64**2
    )
    np.testing.assert_allclose(shares, [0.2, 0.3, 0.5], atol=1 / 16)


def test_the_order_the_palette_is_listed_in_does_not_change_the_picture() -> None:
    palette = PALETTES["four colours"]
    values = np.random.default_rng(15).random((12, 12, 3), dtype=np.float32)
    tile = threshold_tile("bayer")
    shuffled = np.random.default_rng(16).permutation(len(palette))
    np.testing.assert_array_equal(
        palette[ordered(values, palette, tile, amount=1.0)],
        palette[shuffled][ordered(values, palette[shuffled], tile, amount=1.0)],
    )


@pytest.mark.parametrize("amount", [0.0, 0.3, 0.7, 1.0])
def test_a_weaker_pattern_pulls_each_level_towards_the_middle_as_documented(amount: float) -> None:
    tile = threshold_tile("blue_noise")
    palette = PALETTES["four colours"]
    values = np.random.default_rng(20).random((11, 14, 3), dtype=np.float32)
    np.testing.assert_array_equal(
        ordered(values, palette, tile, amount=amount), naive_ordered(values, palette, tile, amount=amount)
    )


@pytest.mark.parametrize("palette", PALETTES.values(), ids=PALETTES.keys())
@pytest.mark.parametrize("method", ["bayer", "blue_noise"])
def test_without_a_pattern_ordered_dithering_is_exactly_the_nearest_colour(palette: np.ndarray, method: Tile) -> None:
    values = np.random.default_rng(21).random((15, 17, 3), dtype=np.float32)
    nearest_colour = np.argmin(((values[:, :, None, :] - palette) ** 2).sum(axis=-1), axis=-1)
    np.testing.assert_array_equal(ordered(values, palette, threshold_tile(method), amount=0.0), nearest_colour)


def test_a_tall_picture_is_dithered_the_same_as_any_piece_of_it() -> None:
    """Every pixel is chosen from its own value and level alone, so a window cut at a multiple of the tile's height
    gives the same pixels as the whole picture does there, also past the rows a first batch of work covers (6 px wide:
    a batch is 10922 rows, which is no multiple of the tile's 8)."""
    tile = threshold_tile("bayer")
    palette = np.array([[0, 0, 0], [0.9, 0.2, 0.1], [1, 1, 1]], np.float32)
    values = np.random.default_rng(18).random((11000, 6, 3), dtype=np.float32)
    whole = ordered(values, palette, tile, amount=1.0)
    for top in (0, 8, 10920):  # the last window holds the rows on both sides of the batch's end
        np.testing.assert_array_equal(ordered(values[top : top + 16], palette, tile, amount=1.0), whole[top : top + 16])


def test_ordered_dithering_repeats_the_tile_across_the_picture() -> None:
    tile = threshold_tile("bayer")
    n = tile.shape[0]
    values = np.random.default_rng(8).random((n * 2, n * 2, 1), dtype=np.float32).repeat(3, axis=2)
    values = np.tile(values[:n, :n], (2, 2, 1))  # the same picture in every tile
    chosen = ordered(values, np.array([[0, 0, 0], [1, 1, 1]], np.float32), tile, amount=1.0)
    np.testing.assert_array_equal(chosen[:n, :n], chosen[n:, n:])
    np.testing.assert_array_equal(chosen[:n, :n], chosen[:n, n:])
