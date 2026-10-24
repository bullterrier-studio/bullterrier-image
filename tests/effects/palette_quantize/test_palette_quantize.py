"""Behaviour of the `palette_quantize` effect."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "palette_quantize"

# The palettes as their authors publish them: Nintendo Game Boy (DMG) LCD greens, IBM CGA mode 4 palette 1 (high
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


def patches(colours: list[tuple[float, float, float]], side: int = 8) -> np.ndarray:
    """A picture of flat squares of the given colours side by side, `side` x `side` pixels each."""
    return np.concatenate([np.full((side, side, 3), c, np.float32) for c in colours], axis=1)


def distinct(picture: np.ndarray) -> int:
    return len(np.unique(picture.reshape(-1, 3), axis=0))


# --- named palettes -------------------------------------------------------------------------------------------


@pytest.mark.parametrize("palette", PALETTES)
def test_a_picture_made_of_the_palettes_own_colours_comes_back_unchanged(palette: str) -> None:
    own = rgb_of(PALETTES[palette])[None]
    np.testing.assert_allclose(registry.apply(NAME, own, palette=palette), own, atol=1e-6)


@pytest.mark.parametrize("palette", ["gameboy", "cga", "ega", "pico8"])
def test_a_photo_is_repainted_in_the_palette_colour_nearest_in_oklab(photo: np.ndarray, palette: str) -> None:
    small = photo[::8, ::8]
    colours = rgb_of(PALETTES[palette])
    out = registry.apply(NAME, small, palette=palette, colors=2)  # `colors` is for palette 'photo' only
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
    out = registry.apply(NAME, np.array([[colour]], np.float32), palette=palette)
    np.testing.assert_allclose(out[0, 0], rgb_of([expected])[0], atol=1e-6)


# --- colours found in the picture -------------------------------------------------------------------------------

FOUR = [(0.9, 0.1, 0.1), (0.1, 0.8, 0.2), (0.2, 0.2, 0.9), (0.95, 0.9, 0.1)]  # red, green, blue, yellow


@pytest.mark.parametrize("init", ["k-means++", "random"])
@pytest.mark.parametrize("seed", [0, 1, 7])
@pytest.mark.parametrize("colors", [4, 9], ids=["exactly as many colours as the picture has", "more than it has"])
def test_a_picture_that_already_has_few_colours_keeps_them(init: str, seed: int, colors: int) -> None:
    flat = patches(FOUR)
    out = registry.apply(NAME, flat, colors=colors, init=init, seed=seed, iterations=30)
    np.testing.assert_allclose(out, flat, atol=1e-6)


def test_each_colour_found_is_the_average_of_the_pixels_it_replaces() -> None:
    # two dark patches (greys 0.10 and 0.14) and two light ones (0.80 and 0.90), the same number of pixels each
    shades = [(0.10, 0.10, 0.10), (0.14, 0.14, 0.14), (0.80, 0.80, 0.80), (0.90, 0.90, 0.90)]
    out = registry.apply(NAME, patches(shades), colors=2)
    np.testing.assert_allclose(out[:, :16], 0.12, atol=1e-6)  # (0.10 + 0.14) / 2
    np.testing.assert_allclose(out[:, 16:], 0.85, atol=1e-6)  # (0.80 + 0.90) / 2


@pytest.mark.parametrize("colors", [2, 5, 16, 40])
def test_a_photo_ends_up_with_at_most_the_colours_asked_for(photo: np.ndarray, colors: int) -> None:
    out = registry.apply(NAME, photo, colors=colors)
    assert 1 < distinct(out) <= colors
    assert distinct(photo) > colors  # the photo really had more, so the reduction happened


def test_quantising_to_n_colours_keeps_the_picture_recognisable(photo: np.ndarray) -> None:
    """Fewer colours lose detail, but the mean colour of the picture is kept (every colour is a mean of its pixels)."""
    out = registry.apply(NAME, photo, colors=8)
    np.testing.assert_allclose(out.mean(axis=(0, 1)), photo.mean(axis=(0, 1)), atol=1e-4)
    assert np.abs(out - photo).mean() < np.abs(registry.apply(NAME, photo, colors=2) - photo).mean()


@pytest.mark.parametrize("sample", [300, 1_000_000], ids=["on a sample of the pixels", "on all the pixels"])
def test_the_same_seed_gives_the_same_picture_and_another_seed_another_start(photo: np.ndarray, sample: int) -> None:
    settings = {"colors": 6, "init": "random", "sample": sample, "iterations": 1}
    first = registry.apply(NAME, photo, seed=1, **settings)
    np.testing.assert_array_equal(registry.apply(NAME, photo, seed=1, **settings), first)
    assert any(not np.array_equal(registry.apply(NAME, photo, seed=s, **settings), first) for s in (2, 3, 4))


def test_colours_are_found_on_a_sample_but_every_pixel_is_repainted() -> None:
    flat = patches(FOUR, side=32)  # 4096 pixels
    out = registry.apply(NAME, flat, colors=4, sample=200)
    np.testing.assert_allclose(out, flat, atol=1e-6)  # 200 sampled pixels see all four patches; all 4096 are assigned


def test_a_sample_of_one_pixel_finds_one_colour_which_then_paints_everything() -> None:
    flat = patches(FOUR)
    out = registry.apply(NAME, flat, colors=4, sample=1)
    average = np.broadcast_to(flat.mean(axis=(0, 1)), flat.shape)
    np.testing.assert_allclose(out, average, atol=1e-6)  # the average colour of the whole picture


def test_rounds_tolerance_and_start_each_change_how_the_colours_are_searched(photo: np.ndarray) -> None:
    base = {"colors": 8, "seed": 3}
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


def test_a_named_palette_ignores_the_settings_for_finding_colours(photo: np.ndarray) -> None:
    first = registry.apply(NAME, photo, palette="cga", colors=2, init="random", sample=10, seed=5)
    other = registry.apply(NAME, photo, palette="cga", colors=200, init="k-means++", sample=5000, seed=9)
    np.testing.assert_array_equal(first, other)
