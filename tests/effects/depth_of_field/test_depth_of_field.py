"""Behaviour of the `depth_of_field` effect: the picture blurred by how far each pixel's depth lies from the focus."""

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image.core import registry

NAME = "depth_of_field"
HEIGHT, WIDTH = 60, 80


@pytest.fixture
def noise() -> np.ndarray:
    """Every pixel unlike its neighbours: any blur shows."""
    return np.random.default_rng(1).random((HEIGHT, WIDTH, 3), dtype=np.float32)


def planes(left: float, right: float) -> np.ndarray:
    """A depth of two planes: `left` in the left half of the picture, `right` in the right half."""
    depth = np.full((HEIGHT, WIDTH), left, np.float32)
    depth[:, WIDTH // 2 :] = right
    return depth


LEFT = np.zeros((HEIGHT, WIDTH), bool)
LEFT[:, : WIDTH // 2] = True
EVERYWHERE = np.ones((HEIGHT, WIDTH), bool)
RAMP = np.tile(np.linspace(1, 0, WIDTH, dtype=np.float32), (HEIGHT, 1))  # nearest at the left, farthest at the right


@pytest.mark.parametrize(
    ("depth", "settings", "sharp"),
    [
        pytest.param(np.full((HEIGHT, WIDTH), 0.8, np.float32), {"focus": 0.8}, EVERYWHERE, id="depth at the focus"),
        pytest.param(RAMP, {"radius": 0.0}, EVERYWHERE, id="radius 0 blurs nothing"),
        pytest.param(planes(0.55, 0.0), {"focus": 0.5, "width": 0.2}, LEFT, id="0.05 from the focus, in a 0.2 band"),
        pytest.param(planes(0.75, 0.0), {"focus": 0.5, "width": 0.5}, LEFT, id="on the band's edge, width / 2 away"),
        pytest.param(planes(0.0, 1.0), {"focus": 0.0, "width": 0.0}, LEFT, id="a band of no width: the focus only"),
    ],
)
def test_pixels_in_the_sharp_band_are_returned_exactly(
    noise: np.ndarray, depth: np.ndarray, settings: dict[str, float], sharp: np.ndarray
) -> None:
    """The sharp band is every depth within width / 2 of the focus; the rest of the picture lies well beyond it, so
    it is blurred."""
    out = registry.apply(NAME, noise, depth=depth, **settings)
    np.testing.assert_array_equal(out[sharp], noise[sharp])
    if not sharp.all():
        assert np.abs(out[~sharp] - noise[~sharp]).max() > 0.05


@pytest.mark.parametrize(
    ("depth", "radius", "levels", "edge"),
    [
        pytest.param(0.0, 6.0, 6, "reflect", id="farthest, as a flat photo's depth (all 0)"),
        pytest.param(0.1, 3.0, 1, "reflect", id="1 level"),
        pytest.param(0.0, 10.0, 16, "mirror", id="16 levels, edge mirror"),
        pytest.param(0.0, 4.0, 6, "nearest", id="edge nearest"),
        pytest.param(0.0, 4.0, 6, "wrap", id="edge wrap"),
    ],
)
def test_far_from_the_focus_the_picture_is_the_gaussian_at_radius(
    noise: np.ndarray, depth: float, radius: float, levels: int, edge: str
) -> None:
    """Focus 0.8, band 0.15, feather 0.4: a depth of 0.275 or less is a full feather beyond the band. The oracle is
    scipy's Gaussian filter, whose edge rules carry the same names."""
    flat = np.full((HEIGHT, WIDTH), depth, np.float32)
    out = registry.apply(NAME, noise, depth=flat, radius=radius, levels=levels, edge=edge)
    expected = ndimage.gaussian_filter(noise.astype(np.float64), (radius, radius, 0), mode=edge)
    assert np.abs(out - expected).max() <= 1e-5


def contrast(picture: np.ndarray) -> np.ndarray:
    """The standard deviation down each column of a picture's first channel: how much of the stripes is left."""
    return picture[..., 0].std(axis=0)


@pytest.mark.parametrize("focus", [1.0, 0.5, 0.2])
def test_the_blur_grows_with_the_distance_of_the_depth_from_the_focus(focus: float) -> None:
    """Horizontal stripes 2 px wide, the same in every column, on a depth falling from 1 at the left to 0 at the
    right: the stripes' contrast per column never rises walking away from the focus column, either way, and is far
    lower at the picture's edge than at the focus."""
    stripes = np.tile((((np.arange(HEIGHT) // 2) % 2).astype(np.float32))[:, None, None], (1, WIDTH, 3))
    out = registry.apply(NAME, stripes, depth=RAMP, focus=focus, width=0.1, feather=0.5, radius=3.0)
    left = contrast(out)
    centre = round((1 - focus) * (WIDTH - 1))
    for walk in (left[centre:], left[: centre + 1][::-1]):
        assert (np.diff(walk) <= 1e-6).all()
    assert min(left[0], left[-1]) < 0.5 * left[centre]


@pytest.mark.parametrize(
    ("focus", "sharp"),
    [pytest.param(1.0, LEFT, id="focus 1: the near plane is sharp"), pytest.param(0.0, ~LEFT, id="focus 0: the far")],
)
def test_focus_1_keeps_the_nearest_sharp_and_0_the_farthest(noise: np.ndarray, focus: float, sharp: np.ndarray) -> None:
    out = registry.apply(NAME, noise, depth=planes(1.0, 0.0), focus=focus)
    np.testing.assert_array_equal(out[sharp], noise[sharp])
    assert np.abs(out[~sharp] - noise[~sharp]).max() > 0.05


@pytest.mark.parametrize("value", [0.0, 0.5, 1.0])
def test_a_flat_depth_gives_a_finite_picture_the_same_each_time(noise: np.ndarray, value: float) -> None:
    flat = np.full((HEIGHT, WIDTH), value, np.float32)
    out = registry.apply(NAME, noise, depth=flat)
    assert np.isfinite(out).all()
    np.testing.assert_array_equal(out, registry.apply(NAME, noise, depth=flat))


def test_the_blur_never_leaves_the_range_of_the_picture(noise: np.ndarray) -> None:
    picture = noise * 0.5 + 0.3  # 0.3..0.8: away from 0 and 1, so a wider range would show
    out = registry.apply(NAME, picture, depth=RAMP, radius=20.0, levels=3)
    assert out.min() >= picture.min()
    assert out.max() <= picture.max()


def test_without_a_depth_the_picture_is_returned_unchanged_as_a_copy(noise: np.ndarray) -> None:
    out = registry.apply(NAME, noise)
    np.testing.assert_array_equal(out, noise)
    assert out is not noise
