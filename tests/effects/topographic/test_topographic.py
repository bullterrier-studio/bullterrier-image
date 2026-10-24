"""Behaviour of the `topographic` effect: contour lines where the picture's luminance crosses a level."""

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image.core import registry

NAME = "topographic"
BLACK_LINES = {"line": "#000000", "smoothing": 0.0, "fill": "none"}


def ramp_picture(low: float = 0.0, high: float = 1.0, width: int = 101, height: int = 6) -> np.ndarray:
    """A grey picture whose luminance runs evenly from `low` at its first column to `high` at its last one."""
    return np.broadcast_to(np.linspace(low, high, width, dtype=np.float32)[None, :, None], (height, width, 3)).copy()


def line_coverage(picture: np.ndarray, settings: dict) -> np.ndarray:
    """How much of each column of a grey picture the black lines cover, 0..1 (out = picture x (1 - coverage))."""
    out = registry.apply(NAME, picture, **{**BLACK_LINES, **settings})
    return 1 - out[0, :, 0] / np.maximum(picture[0, :, 0], 1e-6)


@pytest.mark.parametrize("thickness", [1.0, 2.0, 3.0, 4.5])
@pytest.mark.parametrize(("low", "high"), [(0.0, 1.0), (0.25, 0.75)], ids=["steep", "shallow"])
def test_each_line_covers_thickness_pixels_across_it_whatever_the_slope_of_the_luminance(
    thickness: float, low: float, high: float
) -> None:
    """On a ramp the contours of 8 levels are at luminance 1/8, ..., 7/8: columns 12.5, 25, ... for the steep ramp; the
    shallow ramp (0.25 to 0.75) has them at columns 25, 50, 75 inside it. Each is a line of `thickness` px: the
    coverage across it adds up to exactly that, as pixel values with partial coverage at the ends."""
    coverage = line_coverage(ramp_picture(low, high), {"levels": 8, "thickness": thickness})
    window = slice(8, 93)  # keeps clear of the ramp's two ends, where a contour may lie half outside
    centres = [25, 50, 75] if low > 0 else [12.5, 25, 37.5, 50, 62.5, 75, 87.5]
    for centre in centres:
        near = coverage[int(centre - thickness - 2) : int(centre + thickness + 3)]
        assert near.sum() == pytest.approx(thickness, abs=0.01)
    assert coverage[window].sum() == pytest.approx(thickness * len(centres), abs=0.02)


def test_a_ramp_running_down_the_picture_gets_the_same_lines_as_one_running_across() -> None:
    across = registry.apply(NAME, ramp_picture(), levels=4, thickness=3.0, **BLACK_LINES)
    down = registry.apply(NAME, ramp_picture().transpose(1, 0, 2), levels=4, thickness=3.0, **BLACK_LINES)
    np.testing.assert_allclose(down.transpose(1, 0, 2)[:, 1:-1], across[:, 1:-1], atol=1e-6)


def test_lines_are_drawn_only_near_a_contour_and_the_picture_is_untouched_elsewhere() -> None:
    picture = ramp_picture()
    out = registry.apply(NAME, picture, levels=4, thickness=3.0, **BLACK_LINES)
    untouched = np.ones(101, bool)
    for centre in (25, 50, 75):  # a 3 px line has coverage out to 2 px each side of its centre
        untouched[centre - 2 : centre + 3] = False
    np.testing.assert_array_equal(out[:, untouched], picture[:, untouched])
    assert out[:, 25].max() == 0.0  # the line's centre is fully the line colour


@pytest.mark.parametrize("levels", [2, 5, 9])
def test_levels_is_the_number_of_bands_so_one_fewer_lines(levels: int) -> None:
    """A 361-px ramp crosses levels - 1 contours, each a separate run of covered columns."""
    coverage = line_coverage(ramp_picture(width=361), {"levels": levels, "thickness": 1.0})
    runs = np.count_nonzero(np.diff((coverage > 0).astype(int)) == 1)
    assert runs == levels - 1


def test_the_line_colour_is_the_colour_at_the_centre_of_a_line() -> None:
    out = registry.apply(NAME, ramp_picture(), levels=4, thickness=2.0, line="#ff8000", smoothing=0.0, fill="none")
    np.testing.assert_allclose(out[0, 50], [1.0, 128 / 255, 0.0], atol=1e-6)


def test_a_flat_picture_has_no_contours() -> None:
    flat = np.full((20, 30, 3), 0.5, np.float32)  # exactly on the middle contour of 2 levels, yet nothing varies
    np.testing.assert_array_equal(registry.apply(NAME, flat, levels=2, **BLACK_LINES), flat)


@pytest.mark.parametrize(
    ("grey", "band_grey"),
    [(0.0, 0.0), (0.04, 0.0), (0.55, 5 / 9), (0.95, 1.0), (1.0, 1.0)],
    ids=["black", "first band", "sixth band", "last band", "white lies in the last band"],
)
def test_filled_bands_colour_each_band_by_its_place_in_the_ramp(grey: float, band_grey: float) -> None:
    """10 levels cut the luminance into 10 bands; a pixel of luminance v is in band floor(10 v) (the last one for v = 1)
    and the bands are spread evenly over the colour ramp: band b is at b / 9 of the way from black to white."""
    flat = np.full((8, 8, 3), grey, np.float32)
    out = registry.apply(NAME, flat, levels=10, fill="bands", colors=["#000000", "#ffffff"], smoothing=0.0)
    np.testing.assert_allclose(out, band_grey, atol=1e-6)


@pytest.mark.parametrize(
    ("colour", "band"),
    [((1.0, 0.0, 0.0), 2), ((0.0, 1.0, 0.0), 7), ((0.0, 0.0, 1.0), 0), ((1.0, 1.0, 1.0), 9)],
    ids=["red", "green", "blue", "white"],
)
def test_brightness_is_rec_709_luminance(colour: tuple[float, float, float], band: int) -> None:
    """With 10 levels the band is floor(10 x luminance): 0.2126 -> 2, 0.7152 -> 7, 0.0722 -> 0, 1 -> the last, 9."""
    flat = np.broadcast_to(np.array(colour, np.float32), (8, 8, 3)).copy()
    out = registry.apply(NAME, flat, levels=10, fill="bands", colors=["#000000", "#ffffff"], smoothing=0.0)
    np.testing.assert_allclose(out, band / 9, atol=1e-6)


def test_bands_replace_the_picture_and_lines_are_drawn_on_top() -> None:
    out = registry.apply(
        NAME,
        ramp_picture(),
        levels=4,
        thickness=2.0,
        line="#ff0000",
        fill="bands",
        colors=["#000000", "#ffffff"],
        smoothing=0.0,
    )
    np.testing.assert_allclose(out[0, 10], 0.0, atol=1e-6)  # first band, black
    np.testing.assert_allclose(out[0, 40], 1 / 3, atol=1e-6)  # second band: 1 of 3 steps
    np.testing.assert_allclose(out[0, 50], [1.0, 0.0, 0.0], atol=1e-6)  # on the middle contour: the line colour


@pytest.mark.parametrize("edge", ["reflect", "nearest", "mirror", "wrap"])
@pytest.mark.parametrize("truncate", [2.0, 4.0])
def test_smoothing_blurs_the_luminance_first_with_a_gaussian_of_that_sigma(
    photo: np.ndarray, edge: str, truncate: float
) -> None:
    """Smoothing a photo by sigma and then taking contours equals taking contours of the photo blurred beforehand by
    scipy's Gaussian filter (the bands are drawn from the smoothed luminance, so the whole picture agrees)."""
    settings = {"levels": 8, "fill": "bands", "thickness": 1.5, "smoothing": 0.0}
    blurred = ndimage.gaussian_filter(photo, (3.0, 3.0, 0.0), mode=edge, truncate=truncate)
    smoothed = registry.apply(NAME, photo, **{**settings, "smoothing": 3.0, "truncate": truncate, "edge": edge})
    np.testing.assert_allclose(smoothed, registry.apply(NAME, blurred, **settings), atol=1e-4)


def test_the_edge_rule_matters_only_near_the_border_and_the_cutoff_changes_the_blur(photo: np.ndarray) -> None:
    """A Gaussian of sigma 3 cut at 4 sigma reaches 12 px, so the rule for what lies beyond the border cannot show
    farther in than that."""
    settings = {"levels": 8, "fill": "bands", "smoothing": 3.0}
    reflect = registry.apply(NAME, photo, **settings, edge="reflect")
    wrap = registry.apply(NAME, photo, **settings, edge="wrap")
    inside = (slice(14, -14), slice(14, -14))
    np.testing.assert_allclose(reflect[inside], wrap[inside], atol=1e-4)
    assert np.abs(reflect - wrap).max() > 0.01
    assert np.abs(registry.apply(NAME, photo, **settings, truncate=1.0) - reflect).max() > 0.01


def test_smoothing_removes_contours_that_come_from_fine_speckle() -> None:
    rng = np.random.default_rng(1)
    speckled = np.clip(0.5 + rng.normal(0, 0.15, (64, 64, 1)), 0, 1).repeat(3, axis=-1).astype(np.float32)
    rough = registry.apply(NAME, speckled, levels=8, **{**BLACK_LINES, "smoothing": 0.0})
    smooth = registry.apply(NAME, speckled, levels=8, **{**BLACK_LINES, "smoothing": 4.0})
    assert (rough != speckled).mean() > 3 * (smooth != speckled).mean()
