"""Behaviour of the `sparkle` effect: glints with a cross of rays on the picture's highlights."""

import numpy as np
import pytest
from scipy import ndimage

from photo_fx.core import registry

NAME = "sparkle"
SIZE = 160
CENTRE = (80, 80)


def dark_with(*bright: tuple[int, int], value: float = 0.6) -> np.ndarray:
    """A black picture whose pixels at (row, column) `bright` are grey `value`."""
    rgb = np.zeros((SIZE, SIZE, 3), np.float32)
    for row, column in bright:
        rgb[row, column] = value
    return rgb


def light_of(out: np.ndarray, rgb: np.ndarray) -> np.ndarray:
    """The light screened over `rgb` to make `out`, (h, w): screen is out = rgb + light (1 - rgb), so red gives it."""
    return (out[..., 0] - rgb[..., 0]) / (1 - rgb[..., 0])


def glint(**settings: float) -> np.ndarray:
    """The light of the single sparkle on the one bright pixel at the middle of an otherwise black picture."""
    rgb = dark_with(CENTRE)
    return light_of(registry.apply(NAME, rgb, count=1, threshold=0.5, **settings), rgb)


@pytest.mark.parametrize(
    ("threshold", "value", "sparkles"),
    [
        pytest.param(0.5, 0.6, True, id="a pixel above the threshold sparkles"),
        pytest.param(0.5, 0.4, False, id="a pixel below the threshold does not"),
        pytest.param(0.6, 0.6, False, id="a pixel at the threshold is not brighter than it"),
        pytest.param(1.0, 1.0, False, id="nothing is brighter than white"),
        pytest.param(0.0, 0.01, True, id="at threshold 0 any lit pixel sparkles"),
    ],
)
def test_only_pixels_brighter_than_the_threshold_sparkle(threshold: float, value: float, sparkles: bool) -> None:
    rgb = dark_with(CENTRE, value=value)
    out = registry.apply(NAME, rgb, count=5, threshold=threshold)
    assert (out != rgb).any() == sparkles


def test_sparkles_sit_on_the_highlights_and_stay_within_reach_of_them() -> None:
    """Dots of at most 6 px across (radius 3, fading out by 4.5 px) on a bright block: nothing outside the block grown
    by 5 px changes, and a picture whose block is below the threshold does not change at all."""
    rgb = np.full((SIZE, SIZE, 3), 0.2, np.float32)
    rgb[60:80, 60:100] = 0.95
    near_block = ndimage.binary_dilation(rgb[..., 0] > 0.9, iterations=5)
    out = registry.apply(NAME, rgb, count=8, threshold=0.9, size=6.0, rays=0.0, seed=2)
    touched = (out != rgb).any(axis=-1)
    assert touched.any()
    assert not (touched & ~near_block).any()
    np.testing.assert_array_equal(registry.apply(NAME, rgb, count=8, threshold=0.96, size=6.0, rays=0.0), rgb)


@pytest.mark.parametrize("count", [1, 4, 7])
def test_count_sparkles_are_made_each_on_a_different_highlight(count: int) -> None:
    """Nine isolated bright pixels 40 px apart and a dot of 4 px: each sparkle is a separate patch of light."""
    spots = [(40 + 40 * i, 40 + 40 * j) for i in range(3) for j in range(3)]
    rgb = np.zeros((200, 200, 3), np.float32)
    for row, column in spots:
        rgb[row, column] = 0.6
    out = registry.apply(NAME, rgb, count=count, threshold=0.5, size=4.0, rays=0.0, seed=1)
    patches, found = ndimage.label((out != rgb).any(axis=-1), structure=np.ones((3, 3)))
    assert found == count
    for patch in range(1, found + 1):  # each patch is centred on one of the bright pixels
        rows, columns = np.nonzero(patches == patch)
        assert min(abs(rows.mean() - row) + abs(columns.mean() - column) for row, column in spots) < 0.5


def test_more_sparkles_than_highlights_make_one_on_each_highlight() -> None:
    spots = [(40, 40), (40, 120), (120, 80)]
    rgb = dark_with(*spots)
    out = registry.apply(NAME, rgb, count=50, threshold=0.5, size=4.0, rays=0.0)
    assert ndimage.label((out != rgb).any(axis=-1), structure=np.ones((3, 3)))[1] == len(spots)


def test_a_sparkle_is_a_soft_dot_with_the_brightness_of_its_size() -> None:
    """The dot's diameter is `size` x s and its brightness `opacity` x s with s from 0.5 to 1 (random per sparkle): the
    dot is `opacity` x s at its middle, half of that at its radius `size` x s / 2, and fades out to one and a half
    radii."""
    for seed in range(8):
        dot = glint(size=30.0, rays=0.0, opacity=1.0, seed=seed)
        brightness = dot[CENTRE]
        assert 0.5 <= brightness <= 1.0
        radius = 15.0 * brightness
        assert dot[CENTRE[0], CENTRE[1] + round(radius)] == pytest.approx(brightness / 2, abs=0.08)
        assert dot[CENTRE[0] + round(1.5 * radius) + 1, CENTRE[1]] == 0.0
        assert dot[CENTRE[0], CENTRE[1] + round(0.5 * radius) - 1] == pytest.approx(brightness, abs=0.001)


def test_the_size_variation_spreads_the_sparkles_from_all_alike_to_the_smallest_nearly_gone() -> None:
    alike = [glint(size=10.0, rays=0.0, opacity=1.0, size_variation=0.0, seed=seed)[CENTRE] for seed in range(8)]
    np.testing.assert_allclose(alike, 1.0, atol=1e-6)
    spread = [glint(size=10.0, rays=0.0, opacity=1.0, size_variation=1.0, seed=seed)[CENTRE] for seed in range(30)]
    assert min(spread) < 0.2  # brightness is opacity x the scale, which runs from 1 - size_variation to 1
    assert max(spread) > 0.9


def test_the_thickness_is_the_width_of_the_rays() -> None:
    """A ray of width w covers a pixel fully up to w / 2 - 1 / 2 from its line and not at all from w / 2 + 1 / 2."""
    wide = glint(size=10.0, rays=3.0, thickness=5.0, opacity=1.0, seed=1)
    hairline = glint(size=10.0, rays=3.0, thickness=1.0, opacity=1.0, seed=1)
    brightness, row, column = wide[CENTRE], CENTRE[0], CENTRE[1] + 12  # on the right arm, beyond the dot
    assert wide[row + 2, column] == pytest.approx(brightness)
    assert wide[row + 3, column] == 0.0
    assert hairline[row, column] == pytest.approx(brightness)
    assert hairline[row + 1, column] == 0.0


def test_rays_are_four_arms_along_the_axes_reaching_rays_times_the_dot_size() -> None:
    """The arms run left, right, up and down from the middle, `rays` x `size` x s long (s the sparkle's scale, which is
    also its brightness as a share of `opacity`); no light falls on the diagonals beyond the dot."""
    for seed in range(8):
        star = glint(size=10.0, rays=3.0, opacity=1.0, seed=seed)
        scale = star[CENTRE]
        reach = 3.0 * 10.0 * scale
        for arm in (
            star[CENTRE[0], CENTRE[1] :],
            star[CENTRE[0], CENTRE[1] :: -1],
            star[CENTRE[0] :, CENTRE[1]],
            star[CENTRE[0] :: -1, CENTRE[1]],
        ):
            assert np.flatnonzero(arm > 0).max() == pytest.approx(reach, abs=1.01)
        assert star[CENTRE[0] + 12, CENTRE[1] + 12] == 0.0


def test_no_rays_leave_only_the_dot() -> None:
    dot = glint(size=10.0, rays=0.0, opacity=1.0, seed=1)
    rows, columns = np.nonzero(dot)
    assert rows.max() - rows.min() <= 10
    assert columns.max() - columns.min() <= 10


def test_the_opacity_scales_the_light() -> None:
    full, half = glint(opacity=1.0, seed=1), glint(opacity=0.5, seed=1)
    np.testing.assert_allclose(half, full / 2, atol=1e-6)


def test_sparkles_only_add_light_and_never_darken(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, count=200, threshold=0.3, seed=2)
    assert (out >= photo).all()
    assert (out > photo).any()
