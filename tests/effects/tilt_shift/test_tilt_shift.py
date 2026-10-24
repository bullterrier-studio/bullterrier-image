"""Behaviour of the `tilt_shift` effect: a sharp band across the picture, blurred and more colourful away from it."""

import numpy as np
import pytest
from scipy import ndimage

from photo_fx.core import registry

NAME = "tilt_shift"
HEIGHT, WIDTH = 100, 140


@pytest.fixture
def stripes() -> np.ndarray:
    """Vertical stripes 6 px wide, black and white, the same in every row: a blur shows as lost contrast."""
    columns = ((np.arange(WIDTH) // 3) % 2).astype(np.float32)
    return np.tile(columns[None, :, None], (HEIGHT, 1, 3))


def contrast(rows: np.ndarray) -> np.ndarray:
    """The standard deviation along each row of a picture's first channel: how much of the stripes is left."""
    return rows[..., 0].std(axis=1)


def test_rows_inside_the_focus_band_are_untouched(photo: np.ndarray) -> None:
    """Row r lies at (r - 49.5) / 100 from the middle of the picture as a fraction of its height; the band is `width`
    0.3 wide, so rows within 0.15 of it, about 30 of them, are in focus, saturation boost included."""
    noise = np.random.default_rng(1).random((HEIGHT, WIDTH, 3), dtype=np.float32)
    out = registry.apply(NAME, noise, focus=0.5, width=0.3, feather=0.2, radius=6.0, saturation=0.5)
    inside = np.abs((np.arange(HEIGHT) - 49.5) / HEIGHT) <= 0.15
    assert inside.sum() >= 28
    np.testing.assert_array_equal(out[inside], noise[inside])
    assert np.abs(out[~inside] - noise[~inside]).max() > 0.05  # and the rest is changed


@pytest.mark.parametrize("focus", [0.5, 0.25, 0.8])
def test_the_blur_grows_with_the_distance_from_the_band(stripes: np.ndarray, focus: float) -> None:
    """Stripe contrast per row never rises moving away from the band, either way, and is far lower at the picture's
    edge than at the band's."""
    out = registry.apply(NAME, stripes, focus=focus, width=0.1, feather=0.5, radius=4.0, saturation=0.0, levels=6)
    left = contrast(out)
    centre = focus * HEIGHT
    below, above = left[int(centre) :], left[: int(centre) + 1][::-1]
    for walk in (below, above):
        assert (np.diff(walk) <= 1e-6).all()
    assert left[0] < 0.5 * left[int(centre)] or left[-1] < 0.5 * left[int(centre)]


def test_the_blur_is_the_same_the_same_distance_either_side_of_the_band(stripes: np.ndarray) -> None:
    out = registry.apply(NAME, stripes, focus=0.5, width=0.2, feather=0.4, radius=3.0, saturation=0.0)
    rows = np.arange(HEIGHT) - 49.5
    for distance in (15.0, 30.0, 45.0):
        pair = np.abs(rows) == distance
        above, below = np.flatnonzero(pair & (rows < 0)), np.flatnonzero(pair & (rows > 0))
        np.testing.assert_allclose(contrast(out[above]), contrast(out[below]), atol=1e-6)


@pytest.mark.parametrize(
    ("levels", "mix"),
    [
        (1, [(0.0, 0.75), (8.0, 0.25)]),
        (2, [(0.0, 0.5), (4.0, 0.5)]),
        (4, [(2.0, 1.0)]),
        (3, [(0.0, 0.25), (8 / 3, 0.75)]),
    ],
    ids=[
        "1 level: sharp and sigma 8",
        "2 levels: sharp and sigma 4",
        "4 levels: sigma 2",
        "3 levels: a quarter sharp, three of sigma 8/3",
    ],
)
def test_levels_are_blurs_of_growing_sigma_blended_by_distance(levels: int, mix: list[tuple[float, float]]) -> None:
    """Focus 0.505 puts row 70 (0.205 from the middle of the picture, as a fraction of its height) 0.2 from the focus;
    the band is 0.2 wide, so the row is 0.1 outside it, a quarter of the way through a feather of 0.4: a quarter of
    the way up a stack of `levels` blurs with sigma radius x k / levels (radius 8), k = 1..levels, on top of the sharp
    picture. Worked out by hand: at a quarter, 1 level mixes 3/4 sharp with 1/4 of sigma 8; 2 levels half of sharp
    and half of sigma 4; 4 levels land exactly on sigma 2; 3 levels mix a quarter of the sharp picture with three
    quarters of sigma 8 / 3. The oracle is scipy's own filter."""
    noise = np.random.default_rng(2).random((HEIGHT, WIDTH, 3), dtype=np.float32)
    out = registry.apply(
        NAME, noise, focus=0.505, width=0.2, feather=0.4, radius=8.0, levels=levels, saturation=0.0, angle=0.0
    )
    expected = np.sum(
        [
            weight * (noise if sigma == 0 else ndimage.gaussian_filter(noise, (sigma, sigma, 0)))
            for sigma, weight in mix
        ],
        axis=0,
    )
    np.testing.assert_allclose(out[70], expected[70], atol=2e-5)


def test_far_from_the_band_the_picture_is_blurred_by_radius() -> None:
    noise = np.random.default_rng(3).random((HEIGHT, WIDTH, 3), dtype=np.float32)
    out = registry.apply(NAME, noise, focus=0.0, width=0.1, feather=0.3, radius=5.0, saturation=0.0)
    blurred = ndimage.gaussian_filter(noise, (5.0, 5.0, 0))
    far = slice(60, HEIGHT)  # more than width / 2 + feather = 0.35 of the height from the focus at the top edge
    np.testing.assert_allclose(out[far], blurred[far], atol=2e-5)


def test_a_vertical_band_is_the_horizontal_one_turned() -> None:
    """angle 90 makes the band vertical, its focus counted from the left: it gives the transpose of what angle 0
    gives on the transposed picture."""
    noise = np.random.default_rng(4).random((HEIGHT, WIDTH, 3), dtype=np.float32)
    settings = {"focus": 0.3, "width": 0.15, "feather": 0.3, "radius": 4.0, "saturation": 0.4}
    vertical = registry.apply(NAME, noise, angle=90.0, **settings)
    horizontal = registry.apply(NAME, np.ascontiguousarray(noise.transpose(1, 0, 2)), angle=0.0, **settings)
    np.testing.assert_allclose(vertical, horizontal.transpose(1, 0, 2), atol=2e-6)


def test_angle_180_counts_the_focus_from_the_bottom() -> None:
    noise = np.random.default_rng(5).random((HEIGHT, WIDTH, 3), dtype=np.float32)
    settings = {"focus": 0.3, "width": 0.15, "feather": 0.3, "radius": 4.0, "saturation": 0.4}
    turned = registry.apply(NAME, noise, angle=180.0, **settings)
    flipped = registry.apply(NAME, np.ascontiguousarray(noise[::-1]), angle=0.0, **settings)
    np.testing.assert_allclose(turned, flipped[::-1], atol=2e-6)


def test_a_tilted_band_keeps_its_own_line_sharp() -> None:
    """At 45 degrees the band runs from the lower left to the upper right through the middle: the pixels on that line
    (row + column constant, for a picture as wide as it is tall) are untouched, those far from it are not."""
    noise = np.random.default_rng(6).random((120, 120, 3), dtype=np.float32)
    out = registry.apply(NAME, noise, angle=45.0, focus=0.5, width=0.1, feather=0.2, radius=5.0, saturation=0.0)
    rows, cols = np.mgrid[0:120, 0:120]
    on_the_line = np.abs(rows + cols - 119) <= 4
    far = np.abs(rows + cols - 119) >= 60
    np.testing.assert_array_equal(out[on_the_line], noise[on_the_line])
    assert np.abs(out[far] - noise[far]).max() > 0.1


def test_saturation_boosts_colour_only_where_the_picture_is_blurred() -> None:
    """A flat (0.8, 0.4, 0.2) has luma 0.2126 x 0.8 + 0.7152 x 0.4 + 0.0722 x 0.2 = 0.4706 (Rec. 709); a boost of
    0.5 at full distance pushes each channel 1.5 times as far from it: 0.9647, 0.3647, 0.0647. Nothing in the band."""
    flat = np.tile(np.array([0.8, 0.4, 0.2], np.float32), (HEIGHT, WIDTH, 1))
    out = registry.apply(NAME, flat, focus=0.0, width=0.1, feather=0.2, radius=3.0, saturation=0.5)
    np.testing.assert_allclose(out[-1, 5], [0.9647, 0.3647, 0.0647], atol=1e-4)
    np.testing.assert_array_equal(out[0], flat[0])


def test_no_radius_and_no_saturation_change_nothing(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, radius=0.0, saturation=0.0), photo)


def test_the_boost_never_pushes_a_colour_out_of_range() -> None:
    vivid = np.tile(np.array([1.0, 0.0, 0.3], np.float32), (HEIGHT, WIDTH, 1))
    out = registry.apply(NAME, vivid, focus=0.0, width=0.1, feather=0.1, radius=3.0, saturation=2.0)
    assert out.min() >= 0.0
    assert out.max() <= 1.0
