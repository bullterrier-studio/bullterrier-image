"""Behaviour of the `dither` effect."""

import numpy as np
import pytest

from bullterrier_image import Recipe, fx, render
from bullterrier_image.core import registry
from bullterrier_image.errors import RecipeError

NAME = "dither"
METHODS = ["floyd_steinberg", "atkinson", "bayer", "blue_noise"]
DIFFUSING = ["floyd_steinberg", "atkinson"]
BLACK_WHITE = {"palette": ["#000000", "#ffffff"]}
ONE_PX_ON_64 = 1 / 64  # a dot one pixel wide on a 64 px picture

# The named palettes as their authors publish them: Nintendo Game Boy (DMG) LCD greens, IBM CGA mode 4 palette 1 (high
# intensity), IBM EGA default 16, PICO-8 16.
PALETTES = {
    "gameboy": ["#0f380f", "#306230", "#8bac0f", "#9bbc0f"],
    "cga": ["#000000", "#55ffff", "#ff55ff", "#ffffff"],
    "ega": [
        "#000000", "#0000aa", "#00aa00", "#00aaaa", "#aa0000", "#aa00aa", "#aa5500", "#aaaaaa",
        "#555555", "#5555ff", "#55ff55", "#55ffff", "#ff5555", "#ff55ff", "#ffff55", "#ffffff",
    ],
    "pico8": [
        "#000000", "#1d2b53", "#7e2553", "#008751", "#ab5236", "#5f574f", "#c2c3c7", "#fff1e8",
        "#ff004d", "#ffa300", "#ffec27", "#00e436", "#29adff", "#83769c", "#ff77a8", "#ffccaa",
    ],
}  # fmt: skip


def rgb_of(hex_colours: list[str]) -> np.ndarray:
    return np.array([[int(h[i : i + 2], 16) / 255 for i in (1, 3, 5)] for h in hex_colours], np.float32)


def oklab(rgb: np.ndarray) -> np.ndarray:
    """Björn Ottosson's Oklab (2020) of sRGB rows, in float64, written out here as the reference."""
    c = rgb.astype(np.float64)
    linear = np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)  # IEC 61966-2-1
    to_lms = np.array(
        [[0.4122214708, 0.5363325363, 0.0514459929], [0.2119034982, 0.6806995451, 0.1073969566],
         [0.0883024619, 0.2817188376, 0.6299787005]]
    )  # fmt: skip
    to_lab = np.array(
        [[0.2104542553, 0.7936177850, -0.0040720468], [1.9779984951, -2.4285922050, 0.4505937099],
         [0.0259040371, 0.7827717662, -0.8086757660]]
    )  # fmt: skip
    return np.cbrt(linear @ to_lms.T) @ to_lab.T


def closest(points: np.ndarray, palette: np.ndarray, space) -> np.ndarray:
    """Each row of `points` replaced by the nearest palette colour, with the full distance matrix."""
    gaps = space(points)[:, None, :] - space(palette)[None, :, :]
    return palette[np.argmin((gaps**2).sum(axis=2), axis=1)]


def flat(value: float, size: int = 64) -> np.ndarray:
    return np.full((size, size, 3), value, np.float32)


def patches(colours: list[tuple[float, float, float]], side: int = 8) -> np.ndarray:
    """A picture of flat squares of the given colours side by side, `side` x `side` pixels each."""
    return np.concatenate([np.full((side, side, 3), c, np.float32) for c in colours], axis=1)


def colours_of(rgb: np.ndarray) -> np.ndarray:
    """The distinct colours in a picture, one row each."""
    return np.unique(rgb.reshape(-1, 3), axis=0)


def lightness(rgb: np.ndarray) -> float:
    """The average Oklab lightness of a picture."""
    return float(oklab(rgb.reshape(-1, 3))[:, 0].mean())


# --- the colours: named, listed, or found in the photo --------------------------------------------------------------


@pytest.mark.parametrize("method", METHODS)
@pytest.mark.parametrize("palette", PALETTES)
def test_a_picture_made_of_a_named_palettes_own_colours_comes_back_unchanged(palette: str, method: str) -> None:
    own = rgb_of(PALETTES[palette])[None]
    np.testing.assert_allclose(registry.apply(NAME, own, palette=palette, method=method), own, atol=1e-6)


@pytest.mark.parametrize("palette", PALETTES)
@pytest.mark.parametrize("method", METHODS)
def test_without_the_pattern_a_photo_is_repainted_in_the_palette_colour_nearest_in_oklab(
    photo: np.ndarray, palette: str, method: str
) -> None:
    small = photo[::8, ::8]
    colours = rgb_of(PALETTES[palette])
    out = registry.apply(NAME, small, palette=palette, method=method, pattern=0.0)
    np.testing.assert_allclose(out.reshape(-1, 3), closest(small.reshape(-1, 3), colours, oklab), atol=1e-6)


def test_the_check_above_can_tell_oklab_from_plain_rgb_distance(photo: np.ndarray) -> None:
    """Guard on the oracle: on this photo and palette Oklab and RGB distance pick different colours somewhere."""
    small, colours = photo[::8, ::8].reshape(-1, 3), rgb_of(PALETTES["ega"])
    assert (closest(small, colours, oklab) != closest(small, colours, lambda x: x.astype(np.float64))).any()


@pytest.mark.parametrize(
    ("palette", "colour", "expected"),
    [
        pytest.param("cga", (0.95, 0.95, 0.95), "#ffffff", id="near white goes to CGA white"),
        pytest.param("cga", (0.05, 0.02, 0.04), "#000000", id="near black goes to CGA black"),
        pytest.param("gameboy", (0.0, 0.0, 0.0), "#0f380f", id="black goes to the darkest Game Boy green"),
        pytest.param("gameboy", (1.0, 1.0, 1.0), "#9bbc0f", id="white goes to the lightest Game Boy green"),
        pytest.param("pico8", (1.0, 0.0, 0.3), "#ff004d", id="a vivid red goes to PICO-8 red"),
    ],
)
def test_obvious_colours_land_on_the_obvious_palette_entry(
    palette: str, colour: tuple[float, float, float], expected: str
) -> None:
    out = registry.apply(NAME, np.array([[colour]], np.float32), palette=palette, pattern=0.0)
    np.testing.assert_allclose(out[0, 0], rgb_of([expected])[0], atol=1e-6)


@pytest.mark.parametrize("method", METHODS)
def test_every_pixel_is_one_of_the_colours_listed(method: str, photo: np.ndarray) -> None:
    palette = ["#102030", "#e04020", "#f0e0a0", "#3060c0"]
    out = registry.apply(NAME, photo, method=method, palette=palette)
    wanted = rgb_of(palette)
    found = colours_of(out)
    for colour in found:
        assert np.isclose(colour, wanted, atol=1e-6).all(axis=1).any(), colour
    assert len(found) > 1


@pytest.mark.parametrize("method", METHODS)
def test_the_order_the_palette_is_listed_in_does_not_change_the_picture(method: str, photo: np.ndarray) -> None:
    palette = ["#102030", "#e04020", "#f0e0a0", "#3060c0"]
    first = registry.apply(NAME, photo, method=method, palette=palette)
    again = registry.apply(NAME, photo, method=method, palette=palette[::-1])
    np.testing.assert_array_equal(first, again)  # a tie would go by listing order; a photo's noise has none


@pytest.mark.parametrize("palette", [["#000000"], []], ids=["one colour", "none"])
def test_a_palette_of_fewer_than_two_colours_is_refused(palette: list[str]) -> None:
    with pytest.raises(RecipeError, match="palette"):
        fx.dither(palette=palette)


FOUR = [(0.9, 0.1, 0.1), (0.1, 0.8, 0.2), (0.2, 0.2, 0.9), (0.95, 0.9, 0.1)]  # red, green, blue, yellow


@pytest.mark.parametrize("init", ["k-means++", "random"])
@pytest.mark.parametrize("seed", [0, 1, 7])
@pytest.mark.parametrize("colors", [4, 9], ids=["exactly as many colours as the picture has", "more than it has"])
def test_a_picture_that_already_has_few_colours_keeps_them(init: str, seed: int, colors: int) -> None:
    few = patches(FOUR)
    out = registry.apply(NAME, few, colors=colors, init=init, seed=seed, iterations=30)
    np.testing.assert_allclose(out, few, atol=1e-6)


def test_each_colour_found_is_the_average_of_the_pixels_it_replaces() -> None:
    # two dark patches (greys 0.10 and 0.14) and two light ones (0.80 and 0.90), the same number of pixels each
    shades = [(0.10, 0.10, 0.10), (0.14, 0.14, 0.14), (0.80, 0.80, 0.80), (0.90, 0.90, 0.90)]
    out = registry.apply(NAME, patches(shades), colors=2, pattern=0.0)
    np.testing.assert_allclose(out[:, :16], 0.12, atol=1e-6)  # (0.10 + 0.14) / 2
    np.testing.assert_allclose(out[:, 16:], 0.85, atol=1e-6)  # (0.80 + 0.90) / 2


@pytest.mark.parametrize("colors", [2, 5, 16, 40])
@pytest.mark.parametrize("method", METHODS)
def test_a_photo_ends_up_with_at_most_the_colours_asked_for(photo: np.ndarray, colors: int, method: str) -> None:
    out = registry.apply(NAME, photo, colors=colors, method=method)
    assert 1 < len(colours_of(out)) <= colors
    assert len(colours_of(photo)) > colors  # the photo really had more, so the reduction happened


def test_without_the_pattern_the_picture_keeps_its_average_colour_and_more_colours_follow_it_closer(
    photo: np.ndarray,
) -> None:
    """Every colour is about the mean of the pixels it replaces, so the average is nearly kept; eight colours fit
    better than two."""
    out = registry.apply(NAME, photo, colors=8, pattern=0.0)
    np.testing.assert_allclose(out.mean(axis=(0, 1)), photo.mean(axis=(0, 1)), atol=0.01)
    assert np.abs(out - photo).mean() < np.abs(registry.apply(NAME, photo, colors=2, pattern=0.0) - photo).mean()


@pytest.mark.parametrize("sample", [300, 1_000_000], ids=["on a sample of the pixels", "on all the pixels"])
def test_the_same_seed_gives_the_same_picture_and_another_seed_another_start(photo: np.ndarray, sample: int) -> None:
    settings = {"colors": 6, "init": "random", "sample": sample, "iterations": 1}
    first = registry.apply(NAME, photo, seed=1, **settings)
    np.testing.assert_array_equal(registry.apply(NAME, photo, seed=1, **settings), first)
    assert any(not np.array_equal(registry.apply(NAME, photo, seed=s, **settings), first) for s in (2, 3, 4))


def test_the_seed_also_decides_which_dots_the_colours_are_searched_on(photo: np.ndarray) -> None:
    """Two colours searched to convergence settle on the same pair from any start, so only the sample can differ."""
    settings = {"colors": 2, "sample": 300, "iterations": 100, "tolerance": 0.0, "pattern": 0.0}
    first, second = (registry.apply(NAME, photo, seed=seed, **settings) for seed in (1, 2))
    assert not np.array_equal(first, second)


def test_colours_are_found_on_a_sample_but_every_pixel_is_repainted() -> None:
    four = patches(FOUR, side=32)  # 4096 pixels
    out = registry.apply(NAME, four, colors=4, sample=200)
    np.testing.assert_allclose(out, four, atol=1e-6)  # 200 sampled pixels see all four patches; all 4096 are assigned


def test_a_sample_of_one_pixel_finds_one_colour_which_then_paints_everything() -> None:
    four = patches(FOUR)
    out = registry.apply(NAME, four, colors=4, sample=1, pattern=0.0)
    np.testing.assert_allclose(out, np.broadcast_to(four.mean(axis=(0, 1)), four.shape), atol=1e-6)


def test_rounds_tolerance_and_start_each_change_how_the_colours_are_searched(photo: np.ndarray) -> None:
    base = {"colors": 8, "seed": 3, "pattern": 0.0}
    one_round = registry.apply(NAME, photo, iterations=1, tolerance=0.0, **base)
    settled = registry.apply(NAME, photo, iterations=100, tolerance=0.0, **base)
    assert not np.array_equal(one_round, settled)  # more rounds move the colours
    stopped_early = registry.apply(NAME, photo, iterations=100, tolerance=1.0, **base)  # no colour moves that far
    np.testing.assert_array_equal(stopped_early, one_round)
    random_start = registry.apply(NAME, photo, iterations=1, tolerance=0.0, init="random", **base)
    assert not np.array_equal(random_start, one_round)


def test_a_sample_larger_than_the_picture_uses_the_whole_picture(photo: np.ndarray) -> None:
    whole = registry.apply(NAME, photo, colors=5, sample=photo.shape[0] * photo.shape[1])
    np.testing.assert_array_equal(registry.apply(NAME, photo, colors=5, sample=1_000_000), whole)


def test_a_given_palette_ignores_the_settings_for_finding_colours(photo: np.ndarray) -> None:
    first = registry.apply(NAME, photo, palette="cga", colors=2, init="random", sample=10, seed=5)
    other = registry.apply(NAME, photo, palette="cga", colors=200, init="k-means++", sample=5000, seed=9)
    np.testing.assert_array_equal(first, other)


# --- the dots ---------------------------------------------------------------------------------------------------


@pytest.mark.parametrize("method", METHODS)
@pytest.mark.parametrize("value", [0.0, 1.0], ids=["black stays black", "white stays white"])
@pytest.mark.parametrize(
    ("size", "cell"), [(64, 0.0005), (42, 0.1)], ids=["one pixel dots", "4 px dots, the last ones 2 px"]
)
def test_a_picture_already_in_the_palette_comes_back_unchanged(
    method: str, value: float, size: int, cell: float
) -> None:
    """Also with dots that run off the edge: the short ones at the edge are averaged over their own pixels only."""
    out = registry.apply(NAME, flat(value, size), method=method, cell=cell, **BLACK_WHITE)
    np.testing.assert_array_equal(out, flat(value, size))


@pytest.mark.parametrize("method", METHODS)
def test_dots_are_hard_squares_of_the_asked_size(method: str, photo: np.ndarray) -> None:
    """On a 128 x 96 picture a `cell` of 8 / 128 is 8 px: every aligned 8 x 8 block is one colour."""
    out = registry.apply(NAME, photo, method=method, cell=8 / 128, **BLACK_WHITE)
    blocks = out.reshape(12, 8, 16, 8, 3)
    assert (blocks == blocks[:, :1, :, :1]).all()


@pytest.mark.parametrize("method", METHODS)
def test_a_size_that_is_not_whole_pixels_is_rounded_and_the_last_dots_are_cut_short(method: str) -> None:
    """0.090625 of a 128 px side is 11.6 px, so 12 px dots: ten whole ones across 128 px (120 px) and a last of 8."""
    noise = np.random.default_rng(4).random((96, 128, 3), dtype=np.float32)  # mid tones everywhere: dots keep changing
    out = registry.apply(NAME, noise, method=method, cell=0.090625, **BLACK_WHITE)
    changes = np.flatnonzero((out[:, 1:] != out[:, :-1]).any(axis=(0, 2))) + 1  # the columns where a new dot starts
    assert changes.tolist() == list(range(12, 128, 12))
    changes = np.flatnonzero((out[1:] != out[:-1]).any(axis=(1, 2))) + 1
    assert changes.tolist() == list(range(12, 96, 12))


def test_the_dots_are_the_dithered_average_of_each_square(photo: np.ndarray) -> None:
    """Dithering with 8 px dots is dithering the picture of the squares' averages, one pixel per square."""
    squares = photo.reshape(12, 8, 16, 8, 3).mean(axis=(1, 3)).astype(np.float32)
    big = registry.apply(NAME, photo, cell=8 / 128, **BLACK_WHITE)
    small = registry.apply(NAME, squares, cell=1 / 16, **BLACK_WHITE)  # one px of a 16 px long side
    np.testing.assert_array_equal(big[::8, ::8], small)


# --- the pattern ------------------------------------------------------------------------------------------------


@pytest.mark.parametrize("method", ["floyd_steinberg", "bayer", "blue_noise"])  # Atkinson drops error: see below
@pytest.mark.parametrize("tone", [0.2, 0.5, 0.8])
def test_a_flat_grey_lights_the_share_of_white_dots_its_oklab_lightness_asks_for(method: str, tone: float) -> None:
    """Tones are kept in Oklab: dots of black (lightness 0) and white (lightness 1) average to a grey's own lightness
    when the share of white is that lightness. Ordered patterns mix in sixteenths and error diffusion drops what falls
    off the edge, so a sixteenth is the tolerance."""
    wanted = float(oklab(np.full((1, 3), tone, np.float32))[0, 0])
    out = registry.apply(NAME, flat(tone, 128), method=method, cell=1 / 128, **BLACK_WHITE)
    assert out[..., 0].mean() == pytest.approx(wanted, abs=1 / 16)


@pytest.mark.parametrize("tone", [0.2, 0.5, 0.8])
def test_floyd_steinberg_keeps_the_average_lightness(tone: float, photo: np.ndarray) -> None:
    """All of the error is passed on, so what is lost is the little that falls off the picture's edge."""
    picture = (photo * 0.4 + tone * 0.6).astype(np.float32)
    out = registry.apply(NAME, picture, method="floyd_steinberg", **BLACK_WHITE)
    assert lightness(out) == pytest.approx(lightness(picture), abs=0.03)


def test_atkinson_clears_the_highlights_floyd_steinberg_keeps_as_dots() -> None:
    """A quarter of Atkinson's error is dropped, so near white it gives plain white where Floyd-Steinberg still scatters
    dark dots."""
    near_white = flat(0.97, 128)
    atkinson = registry.apply(NAME, near_white, method="atkinson", cell=1 / 128, **BLACK_WHITE)[..., 0].mean()
    floyd = registry.apply(NAME, near_white, method="floyd_steinberg", cell=1 / 128, **BLACK_WHITE)[..., 0].mean()
    assert atkinson > floyd


@pytest.mark.parametrize("method", METHODS)
@pytest.mark.parametrize("palette", [BLACK_WHITE["palette"], PALETTES["pico8"]], ids=["two colours", "sixteen colours"])
def test_without_any_pattern_every_method_gives_the_nearest_colour_exactly(
    method: str, palette: list[str], photo: np.ndarray
) -> None:
    flat_out = registry.apply(NAME, photo, method=method, pattern=0.0, palette=palette)
    nearest_colour = closest(photo.reshape(-1, 3), rgb_of(palette), oklab).reshape(photo.shape)
    np.testing.assert_array_equal(flat_out, nearest_colour)


@pytest.mark.parametrize("method", METHODS)
def test_a_weaker_pattern_leaves_fewer_dots_that_differ_from_the_nearest_colour(method: str, photo: np.ndarray) -> None:
    nearest_colour = closest(photo.reshape(-1, 3), rgb_of(BLACK_WHITE["palette"]), oklab).reshape(photo.shape)
    differs = [
        float(
            (registry.apply(NAME, photo, method=method, pattern=a, **BLACK_WHITE) != nearest_colour).any(axis=-1).mean()
        )
        for a in (1.0, 0.5, 0.0)
    ]
    assert differs[0] > differs[1] >= differs[2]
    assert differs[0] > 0.05  # the full pattern really does put dots against the nearest colour


@pytest.mark.parametrize("method", METHODS)
def test_a_ramp_goes_through_every_colour_of_a_three_colour_palette_from_dark_to_light(method: str) -> None:
    ramp = np.broadcast_to(np.linspace(0, 1, 96, dtype=np.float32)[None, :, None], (32, 96, 3)).copy()
    out = registry.apply(NAME, ramp, method=method, palette=["#000000", "#808080", "#ffffff"], cell=1 / 96)
    assert len(colours_of(out)) == 3
    thirds = out[..., 0].reshape(32, 3, 32).mean(axis=(0, 2))
    assert thirds[0] < thirds[1] < thirds[2]  # darker on the left, lighter on the right


@pytest.mark.parametrize("method", METHODS)
@pytest.mark.parametrize("shape", [(1, 1), (1, 40), (40, 1), (3, 5)], ids=lambda s: f"{s[0]}x{s[1]}")
def test_tiny_pictures_still_come_out_in_palette_colours(method: str, shape: tuple[int, int]) -> None:
    rgb = np.random.default_rng(6).random((*shape, 3), dtype=np.float32)
    out = registry.apply(NAME, rgb, method=method, **BLACK_WHITE)
    assert out.shape == rgb.shape
    assert set(map(tuple, colours_of(out).tolist())) <= {(0.0, 0.0, 0.0), (1.0, 1.0, 1.0)}


def test_a_hue_only_palette_dithers_a_grey_into_all_of_its_colours() -> None:
    """Red, green and blue are no brightness ramp; a grey must still come out mixed, not as one colour."""
    for method in ("bayer", "blue_noise", "floyd_steinberg"):
        out = registry.apply(NAME, flat(0.33), method=method, palette=["#ff0000", "#00ff00", "#0000ff"], cell=1 / 64)
        assert len(colours_of(out)) == 3, method


# --- the preset `posterize`: a few flat colours found in the photo --------------------------------------------------


def with_preset(photo: np.ndarray, *steps) -> np.ndarray:
    """The photo through the given steps as one recipe, at its own size."""
    return render(photo, Recipe("t", size=None, steps=list(steps)))


def test_the_posterize_preset_repaints_a_photo_in_a_handful_of_flat_colours(photo: np.ndarray) -> None:
    """Flat: each pixel is the nearest (in Oklab) of the few colours found, with no dot pattern between them."""
    out = with_preset(photo, fx.dither(preset="posterize"))
    found = colours_of(out)
    assert 2 <= len(found) <= 8
    assert len(colours_of(photo)) > 1000  # the photo really had far more
    np.testing.assert_array_equal(out.reshape(-1, 3), closest(photo.reshape(-1, 3), found, oklab))


@pytest.mark.parametrize("colors", [3, 12])
def test_the_posterize_preset_takes_its_number_of_colours_from_the_step(photo: np.ndarray, colors: int) -> None:
    out = with_preset(photo, fx.dither(preset="posterize", colors=colors))
    assert 2 <= len(colours_of(out)) <= colors
