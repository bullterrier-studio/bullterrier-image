"""Behaviour of the `topographic` effect: contour lines where a field crosses a level."""

from collections.abc import Callable
from typing import Any

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image.core import registry, runner
from bullterrier_image.recipe import Recipe, Step

NAME = "topographic"
BLACK_LINES: dict[str, Any] = {"line": "#000000", "smoothing": 0.0, "fill": "none"}
LUMA = np.array([0.2126, 0.7152, 0.0722])  # Rec. 709, written here again so that the test does not use the code's


def light(picture: np.ndarray) -> np.ndarray:
    """The brightness of a picture, by hand: the field the effect used to contour on its own."""
    return (picture @ LUMA).astype(np.float32)


def contours(picture: np.ndarray, field: np.ndarray | None = None, **settings: Any) -> np.ndarray:
    """The effect on `picture` contouring `field`, else the picture's own brightness."""
    return registry.apply(NAME, picture, field=light(picture) if field is None else field, **settings)


def ramp_picture(low: float = 0.0, high: float = 1.0, width: int = 101, height: int = 6) -> np.ndarray:
    """A grey picture whose luminance runs evenly from `low` at its first column to `high` at its last one."""
    return np.broadcast_to(np.linspace(low, high, width, dtype=np.float32)[None, :, None], (height, width, 3)).copy()


def line_coverage(picture: np.ndarray, settings: dict) -> np.ndarray:
    """How much of each column of a grey picture the black lines cover, 0..1 (out = picture x (1 - coverage))."""
    out = contours(picture, **{**BLACK_LINES, **settings})
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
    across = contours(ramp_picture(), levels=4, thickness=3.0, **BLACK_LINES)
    down = contours(ramp_picture().transpose(1, 0, 2), levels=4, thickness=3.0, **BLACK_LINES)
    np.testing.assert_allclose(down.transpose(1, 0, 2)[:, 1:-1], across[:, 1:-1], atol=1e-6)


def test_lines_are_drawn_only_near_a_contour_and_the_picture_is_untouched_elsewhere() -> None:
    picture = ramp_picture()
    out = contours(picture, levels=4, thickness=3.0, **BLACK_LINES)
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
    out = contours(ramp_picture(), levels=4, thickness=2.0, line="#ff8000", smoothing=0.0, fill="none")
    np.testing.assert_allclose(out[0, 50], [1.0, 128 / 255, 0.0], atol=1e-6)


def test_a_flat_picture_has_no_contours() -> None:
    flat = np.full((20, 30, 3), 0.5, np.float32)  # exactly on the middle contour of 2 levels, yet nothing varies
    np.testing.assert_array_equal(contours(flat, levels=2, **BLACK_LINES), flat)


@pytest.mark.parametrize(
    ("grey", "band_grey"),
    [(0.0, 0.0), (0.04, 0.0), (0.55, 5 / 9), (0.95, 1.0), (1.0, 1.0)],
    ids=["black", "first band", "sixth band", "last band", "white lies in the last band"],
)
def test_filled_bands_colour_each_band_by_its_place_in_the_ramp(grey: float, band_grey: float) -> None:
    """10 levels cut the luminance into 10 bands; a pixel of luminance v is in band floor(10 v) (the last one for v = 1)
    and the bands are spread evenly over the colour ramp: band b is at b / 9 of the way from black to white."""
    flat = np.full((8, 8, 3), grey, np.float32)
    out = contours(flat, levels=10, fill="bands", colors=["#000000", "#ffffff"], smoothing=0.0)
    np.testing.assert_allclose(out, band_grey, atol=1e-6)


@pytest.mark.parametrize(
    ("colour", "band"),
    [((1.0, 0.0, 0.0), 2), ((0.0, 1.0, 0.0), 7), ((0.0, 0.0, 1.0), 0), ((1.0, 1.0, 1.0), 9)],
    ids=["red", "green", "blue", "white"],
)
def test_a_recipe_step_without_a_field_contours_the_picture_s_brightness(
    colour: tuple[float, float, float], band: int
) -> None:
    """With 10 levels the band is floor(10 x luminance): 0.2126 -> 2, 0.7152 -> 7, 0.0722 -> 0, 1 -> the last, 9."""
    flat = np.broadcast_to(np.array(colour, np.float32), (8, 8, 3)).copy()
    step = Step(NAME, params={"levels": 10, "fill": "bands", "colors": ["#000000", "#ffffff"], "smoothing": 0.0})
    out = runner.render(flat, Recipe([step], name="bands", size=8))
    np.testing.assert_allclose(out, band / 9, atol=1e-6)


def test_bands_replace_the_picture_and_lines_are_drawn_on_top() -> None:
    out = contours(
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
    settings: dict[str, Any] = {"levels": 8, "fill": "bands", "thickness": 1.5, "smoothing": 0.0}
    blurred = ndimage.gaussian_filter(photo, (3.0, 3.0, 0.0), mode=edge, truncate=truncate)
    smoothed = contours(photo, **{**settings, "smoothing": 3.0, "truncate": truncate, "edge": edge})
    np.testing.assert_allclose(smoothed, contours(blurred, **settings), atol=1e-4)


def test_the_edge_rule_matters_only_near_the_border_and_the_cutoff_changes_the_blur(photo: np.ndarray) -> None:
    """A Gaussian of sigma 3 cut at 4 sigma reaches 12 px, so the rule for what lies beyond the border cannot show
    farther in than that."""
    settings: dict[str, Any] = {"levels": 8, "fill": "bands", "smoothing": 3.0}
    reflect = contours(photo, **settings, edge="reflect")
    wrap = contours(photo, **settings, edge="wrap")
    inside = (slice(14, -14), slice(14, -14))
    np.testing.assert_allclose(reflect[inside], wrap[inside], atol=1e-4)
    assert np.abs(reflect - wrap).max() > 0.01
    assert np.abs(contours(photo, **settings, truncate=1.0) - reflect).max() > 0.01


def test_smoothing_removes_contours_that_come_from_fine_speckle() -> None:
    rng = np.random.default_rng(1)
    speckled = np.clip(0.5 + rng.normal(0, 0.15, (64, 64, 1)), 0, 1).repeat(3, axis=-1).astype(np.float32)
    rough = contours(speckled, levels=8, **{**BLACK_LINES, "smoothing": 0.0})
    smooth = contours(speckled, levels=8, **{**BLACK_LINES, "smoothing": 4.0})
    assert (rough != speckled).mean() > 3 * (smooth != speckled).mean()


def depth_ramp(photo: np.ndarray) -> np.ndarray:
    """A depth map unlike the photo's brightness: 0 at the left edge to 1 at the right edge."""
    return np.broadcast_to(np.linspace(0, 1, photo.shape[1], dtype=np.float32), photo.shape[:2]).copy()


def test_the_lines_follow_the_field_given_whatever_the_picture_looks_like(photo: np.ndarray) -> None:
    """Contours of 4 levels of a ramp across the picture are vertical lines at 1/4, 1/2 and 3/4 of the way from the
    first to the last column. A 3 px line covers the columns within 2 px of its centre: the photo is untouched beyond
    them, and the 1/2 line (between columns 63 and 64 of 128) is black."""
    width = photo.shape[1]
    out = contours(photo, depth_ramp(photo), levels=4, thickness=3.0, **BLACK_LINES)
    columns = np.arange(width)
    near = np.zeros(width, bool)
    for quarter in (1, 2, 3):
        near |= np.abs(columns - (width - 1) * quarter / 4) < 2
    np.testing.assert_array_equal(out[:, ~near], photo[:, ~near])
    assert photo[:, 63:65].min() > 0.01  # so that black is not simply the photo
    np.testing.assert_allclose(out[:, 63:65], 0.0, atol=1e-6)


@pytest.mark.parametrize(
    ("step_params", "field"),
    [
        pytest.param({}, "light", id="no field written: the picture's brightness, as before fields existed"),
        pytest.param({"field": "light"}, "light", id="light"),
        pytest.param({"field": "depth"}, "depth", id="depth"),
    ],
)
def test_a_field_named_in_a_recipe_gives_the_same_picture_as_the_equal_array_passed_directly(
    step_params: dict, field: str, photo: np.ndarray, apply_as_rendered: Callable[..., np.ndarray]
) -> None:
    depth = depth_ramp(photo)
    settings: dict[str, Any] = {"levels": 6, "fill": "bands", "smoothing": 2.0}
    recipe = Recipe([Step(NAME, params={**settings, **step_params})], name="named", size=max(photo.shape[:2]))
    named = runner.render(photo, recipe, maps={"depth": depth})
    expected = apply_as_rendered(NAME, photo, field=depth if field == "depth" else light(photo), **settings)
    # the hand-made brightness differs from the library's in the last float32 digit, which a line's edge magnifies
    np.testing.assert_allclose(named, expected, atol=1e-3, rtol=0)
    assert np.abs(contours(photo, depth, **settings) - contours(photo, **settings)).max() > 0.05  # fields differ


def test_a_field_that_varies_down_the_picture_puts_its_lines_where_it_crosses_the_levels_not_mirrored(
    photo: np.ndarray,
) -> None:
    """The field (row / last row)^2 crosses 2/4 and 3/4 at rows 0.707 and 0.866 of the way down (67.2 and 82.3 of 96),
    which are not where the field turned upside down would cross them (28.8 and 13.7)."""
    height, width = photo.shape[:2]
    field = np.broadcast_to(((np.arange(height) / (height - 1)) ** 2)[:, None], (height, width)).astype(np.float32)
    out = contours(photo, field, levels=4, thickness=3.0, **BLACK_LINES)
    np.testing.assert_allclose(out[[67, 82]], 0.0, atol=1e-6)
    np.testing.assert_array_equal(out[[29, 14]], photo[[29, 14]])
