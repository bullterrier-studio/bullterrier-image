"""Behaviour of the `threshold` effect."""

from itertools import pairwise

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "threshold"
BLACK, WHITE = np.zeros(3, np.float32), np.ones(3, np.float32)


def pixel(r: float, g: float, b: float) -> np.ndarray:
    return np.array([[[r, g, b]]], np.float32)


@pytest.mark.parametrize(
    ("rgb", "level", "is_light"),
    [
        pytest.param((0.25, 0.25, 0.25), 0.5, False, id="a grey below the level is dark"),
        pytest.param((0.75, 0.75, 0.75), 0.5, True, id="a grey above the level is light"),
        pytest.param((0.75, 0.75, 0.75), 0.9, False, id="raising the level darkens the same grey"),
        pytest.param((0.25, 0.25, 0.25), 0.1, True, id="lowering the level lightens the same grey"),
        pytest.param((0.0, 1.0, 0.0), 0.5, True, id="pure green has luminance 0.7152, so it is light"),
        pytest.param((1.0, 0.0, 0.0), 0.5, False, id="pure red has luminance 0.2126, so it is dark"),
        pytest.param((0.0, 0.0, 1.0), 0.5, False, id="pure blue has luminance 0.0722, so it is dark"),
        pytest.param((0.0, 0.0, 0.0), 0.0, True, id="level 0 makes even black light"),
    ],
)
def test_a_pixel_is_light_when_its_luminance_reaches_the_level(
    rgb: tuple[float, float, float], level: float, is_light: bool
) -> None:
    out = registry.apply(NAME, pixel(*rgb), level=level)
    np.testing.assert_array_equal(out[0, 0], WHITE if is_light else BLACK)


def test_the_two_colours_given_are_the_only_ones_in_the_result(photo: np.ndarray) -> None:
    dark, light = "#102030", "#f0e0d0"
    out = registry.apply(NAME, photo, level=0.5, dark=dark, light=light)
    colours = {tuple(c) for c in np.unique(out.reshape(-1, 3), axis=0)}
    assert colours == {
        (np.float32(0x10 / 255), np.float32(0x20 / 255), np.float32(0x30 / 255)),
        (np.float32(0xF0 / 255), np.float32(0xE0 / 255), np.float32(0xD0 / 255)),
    }


def test_each_pixel_matches_a_per_pixel_reference_using_rec709_luminance(photo: np.ndarray) -> None:
    small = photo[::8, ::8]
    level = 0.45
    out = registry.apply(NAME, small, level=level, dark=[0.1, 0.2, 0.3], light=[0.9, 0.8, 0.7])
    for y in range(small.shape[0]):
        for x in range(small.shape[1]):
            r, g, b = (float(v) for v in small[y, x])
            lum = 0.2126 * r + 0.7152 * g + 0.0722 * b  # ITU-R BT.709
            want = [0.9, 0.8, 0.7] if lum >= level else [0.1, 0.2, 0.3]
            np.testing.assert_allclose(out[y, x], want, atol=1e-6)


def test_raising_the_level_never_turns_a_dark_pixel_light(photo: np.ndarray) -> None:
    light_at = [registry.apply(NAME, photo, level=level)[..., 0] == 1 for level in (0.1, 0.3, 0.5, 0.7, 0.9)]
    for lower, higher in pairwise(light_at):
        assert (higher <= lower).all()  # what is light at the higher level was already light at the lower
        assert higher.sum() < lower.sum()  # and the photo has pixels between the levels, so some really changed
