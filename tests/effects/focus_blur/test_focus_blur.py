"""Behaviour of the `focus_blur` effect: the picture blurred by how far a field's value lies from the focus."""

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image import recipe
from bullterrier_image.core import registry

NAME = "focus_blur"
HEIGHT, WIDTH = 100, 140


@pytest.fixture
def noise() -> np.ndarray:
    """Every pixel unlike its neighbours: any blur shows."""
    return np.random.default_rng(1).random((HEIGHT, WIDTH, 3), dtype=np.float32)


def planes(left: float, right: float) -> np.ndarray:
    """A field of two planes: `left` in the left half of the picture, `right` in the right half."""
    field = np.full((HEIGHT, WIDTH), left, np.float32)
    field[:, WIDTH // 2 :] = right
    return field


LEFT = np.zeros((HEIGHT, WIDTH), bool)
LEFT[:, : WIDTH // 2] = True
EVERYWHERE = np.ones((HEIGHT, WIDTH), bool)
RAMP = np.tile(np.linspace(1, 0, WIDTH, dtype=np.float32), (HEIGHT, 1))  # high at the left, low at the right
DOWN = np.tile(((np.arange(HEIGHT) + 0.5) / HEIGHT).astype(np.float32)[:, None], (1, WIDTH))  # 0 at the top, 1 below


@pytest.mark.parametrize(
    ("field", "settings", "sharp"),
    [
        pytest.param(np.full((HEIGHT, WIDTH), 0.8, np.float32), {"focus": 0.8}, EVERYWHERE, id="field at the focus"),
        pytest.param(RAMP, {"radius": 0.0}, EVERYWHERE, id="radius 0 blurs nothing"),
        pytest.param(planes(0.55, 0.0), {"focus": 0.5, "width": 0.2}, LEFT, id="0.05 from the focus, in a 0.2 band"),
        pytest.param(planes(0.75, 0.0), {"focus": 0.5, "width": 0.5}, LEFT, id="on the band's edge, width / 2 away"),
        pytest.param(planes(0.0, 1.0), {"focus": 0.0, "width": 0.0}, LEFT, id="a band of no width: the focus only"),
    ],
)
def test_pixels_in_the_sharp_band_are_returned_exactly(
    noise: np.ndarray, field: np.ndarray, settings: dict[str, float], sharp: np.ndarray
) -> None:
    """The sharp band is every value within width / 2 of the focus; the rest of the picture lies well beyond it, so
    it is blurred."""
    out = registry.apply(NAME, noise, field=field, **settings)
    np.testing.assert_array_equal(out[sharp], noise[sharp])
    if not sharp.all():
        assert np.abs(out[~sharp] - noise[~sharp]).max() > 0.05


@pytest.mark.parametrize(
    ("value", "radius", "levels", "edge", "truncate"),
    [
        pytest.param(0.0, 6.0, 6, "reflect", 4.0, id="farthest, as a flat photo's depth (all 0)"),
        pytest.param(0.1, 3.0, 1, "reflect", 4.0, id="1 level"),
        pytest.param(0.0, 10.0, 16, "mirror", 4.0, id="16 levels, edge mirror"),
        pytest.param(0.0, 4.0, 6, "nearest", 4.0, id="edge nearest"),
        pytest.param(0.0, 4.0, 6, "wrap", 4.0, id="edge wrap"),
        pytest.param(0.0, 5.0, 3, "reflect", 1.0, id="Gaussian cut off at 1 sigma"),
    ],
)
def test_far_from_the_focus_the_picture_is_the_gaussian_at_radius(
    noise: np.ndarray, value: float, radius: float, levels: int, edge: str, truncate: float
) -> None:
    """Focus 0.8, band 0.15, feather 0.4: a value of 0.325 or less is a full feather beyond the band. The oracle is
    scipy's Gaussian filter, whose edge rules carry the same names."""
    flat = np.full((HEIGHT, WIDTH), value, np.float32)
    out = registry.apply(NAME, noise, field=flat, radius=radius, levels=levels, edge=edge, truncate=truncate)
    expected = ndimage.gaussian_filter(noise.astype(np.float64), (radius, radius, 0), mode=edge, truncate=truncate)
    assert np.abs(out - expected).max() <= 1e-5


@pytest.mark.parametrize(
    ("levels", "mix"),
    [
        pytest.param(1, [(0.0, 0.75), (8.0, 0.25)], id="1 level: sharp and sigma 8"),
        pytest.param(2, [(0.0, 0.5), (4.0, 0.5)], id="2 levels: sharp and sigma 4"),
        pytest.param(4, [(2.0, 1.0)], id="4 levels: sigma 2"),
        pytest.param(3, [(0.0, 0.25), (8 / 3, 0.75)], id="3 levels: a quarter sharp, three of sigma 8/3"),
    ],
)
def test_levels_are_blurs_of_growing_sigma_blended_by_distance(
    noise: np.ndarray, levels: int, mix: list[tuple[float, float]]
) -> None:
    """Row 70 of the field `DOWN` is 0.705; focus 0.505 puts it 0.2 from the focus, the band is 0.2 wide, so the row is
    0.1 outside it, a quarter of the way through a feather of 0.4: a quarter of the way up a stack of `levels` blurs
    with sigma radius x k / levels (radius 8), k = 1..levels, on top of the sharp picture. Worked out by hand: at a
    quarter, 1 level mixes 3/4 sharp with 1/4 of sigma 8; 2 levels half of sharp and half of sigma 4; 4 levels land
    exactly on sigma 2; 3 levels mix a quarter of the sharp picture with three quarters of sigma 8 / 3. The oracle is
    scipy's own filter."""
    out = registry.apply(NAME, noise, field=DOWN, focus=0.505, width=0.2, feather=0.4, radius=8.0, levels=levels)
    expected = np.sum(
        [
            weight * (noise if sigma == 0 else ndimage.gaussian_filter(noise, (sigma, sigma, 0)))
            for sigma, weight in mix
        ],
        axis=0,
    )
    np.testing.assert_allclose(out[70], expected[70], atol=2e-5)


def contrast(picture: np.ndarray) -> np.ndarray:
    """The standard deviation down each column of a picture's first channel: how much of the stripes is left."""
    return picture[..., 0].std(axis=0)


@pytest.mark.parametrize("focus", [1.0, 0.5, 0.2])
def test_the_blur_grows_with_the_distance_of_the_field_from_the_focus(focus: float) -> None:
    """Horizontal stripes 2 px wide, the same in every column, on a field falling from 1 at the left to 0 at the
    right: the stripes' contrast per column never rises walking away from the focus column, either way, and is far
    lower at the picture's edge than at the focus."""
    stripes = np.tile((((np.arange(HEIGHT) // 2) % 2).astype(np.float32))[:, None, None], (1, WIDTH, 3))
    out = registry.apply(NAME, stripes, field=RAMP, focus=focus, width=0.1, feather=0.5, radius=3.0)
    left = contrast(out)
    centre = round((1 - focus) * (WIDTH - 1))
    for walk in (left[centre:], left[: centre + 1][::-1]):
        assert (np.diff(walk) <= 1e-6).all()
    assert min(left[0], left[-1]) < 0.5 * left[centre]


@pytest.mark.parametrize(
    ("focus", "sharp"),
    [pytest.param(1.0, LEFT, id="focus 1: the high plane is sharp"), pytest.param(0.0, ~LEFT, id="focus 0: the low")],
)
def test_focus_1_keeps_the_highest_values_sharp_and_0_the_lowest(
    noise: np.ndarray, focus: float, sharp: np.ndarray
) -> None:
    out = registry.apply(NAME, noise, field=planes(1.0, 0.0), focus=focus)
    np.testing.assert_array_equal(out[sharp], noise[sharp])
    assert np.abs(out[~sharp] - noise[~sharp]).max() > 0.05


def test_the_blur_never_leaves_the_range_of_the_picture(noise: np.ndarray) -> None:
    picture = noise * 0.5 + 0.3  # 0.3..0.8: away from 0 and 1, so a wider range would show
    out = registry.apply(NAME, picture, field=RAMP, radius=20.0, levels=3)
    assert out.min() >= picture.min()
    assert out.max() <= picture.max()


def test_a_linear_gradient_makes_a_sharp_band_across_the_picture(noise: np.ndarray) -> None:
    """The gradient at 270 degrees is 0 at the top and 1 at the bottom, row r at (r + 0.5) / 100; a band 0.3 wide
    around 0.5 holds the rows from 35 to 64, the closest rows beyond it are blurred."""
    across = recipe.Recipe(
        "band",
        [recipe.Step(NAME, {"field": "across", "focus": 0.5, "width": 0.3, "feather": 0.2, "radius": 6})],
        size=None,
        regions={"across": recipe.LinearGradient(angle=270)},
    )
    out = recipe.render(noise, across)
    np.testing.assert_array_equal(out[35:65], noise[35:65])
    assert np.abs(out[:30] - noise[:30]).max() > 0.05
    assert np.abs(out[70:] - noise[70:]).max() > 0.05


def test_a_band_along_a_tilted_gradient_keeps_its_own_line_sharp() -> None:
    """The gradient at 315 degrees rises towards the lower right; a picture as wide as it is tall has the middle of
    its range on the line row + column = 119, and the pixels on that line are untouched, those far from it are not."""
    noise = np.random.default_rng(6).random((120, 120, 3), dtype=np.float32)
    tilted = recipe.Recipe(
        "tilted",
        [recipe.Step(NAME, {"field": "across", "focus": 0.5, "width": 0.1, "feather": 0.2, "radius": 5})],
        size=None,
        regions={"across": recipe.LinearGradient(angle=315)},
    )
    out = recipe.render(noise, tilted)
    rows, cols = np.mgrid[0:120, 0:120]
    on_the_line = np.abs(rows + cols - 119) <= 4
    far = np.abs(rows + cols - 119) >= 60
    np.testing.assert_array_equal(out[on_the_line], noise[on_the_line])
    assert np.abs(out[far] - noise[far]).max() > 0.1


def test_a_driven_grade_adds_colour_only_where_the_picture_is_blurred() -> None:
    """The miniature look: a flat (0.8, 0.4, 0.2) has luma 0.2126 x 0.8 + 0.7152 x 0.4 + 0.0722 x 0.2 = 0.4706 (Rec.
    709); a boost of 0.5 pushes each channel 1.5 times as far from it: 0.9647, 0.3647, 0.0647. The `grade` is driven
    by the band that stays sharp (1 inside it, falling to 0 over `feather`), so rows far beyond the band have the
    boost and the rows in it have none."""
    flat = np.tile(np.array([0.8, 0.4, 0.2], np.float32), (HEIGHT, WIDTH, 1))
    miniature = recipe.Recipe(
        "miniature",
        [
            recipe.Step(NAME, {"field": "across", "focus": 0.0, "width": 0.1, "feather": 0.2, "radius": 3}),
            recipe.Step("grade", {"saturation": recipe.Drive(by="sharp", start=1.5, end=1.0)}),
        ],
        size=None,
        regions={
            "across": recipe.LinearGradient(angle=270),
            "sharp": recipe.Band(of="across", low=0.0, high=0.05, feather=0.2),
        },
    )
    out = recipe.render(flat, miniature)
    np.testing.assert_allclose(out[-1, 5], [0.9647, 0.3647, 0.0647], atol=1e-4)
    np.testing.assert_allclose(out[0], flat[0], atol=1e-6)
