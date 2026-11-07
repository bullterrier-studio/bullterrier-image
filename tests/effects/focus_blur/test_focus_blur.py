"""The `focus_blur` effect through the recipe and `fx` builders: regions, drives and maps given by name. The blur
itself is tested in `crates/core/tests/it/focus_blur.rs`."""

import numpy as np
import pytest

from bullterrier_image import fx, recipe

NAME = "focus_blur"
HEIGHT, WIDTH = 100, 140


@pytest.fixture
def noise() -> np.ndarray:
    """Every pixel unlike its neighbours: any blur shows."""
    return np.random.default_rng(1).random((HEIGHT, WIDTH, 3), dtype=np.float32)


RAMP = np.tile(np.linspace(1, 0, WIDTH, dtype=np.float32), (HEIGHT, 1))  # high at the left, low at the right


def test_a_linear_gradient_makes_a_sharp_band_across_the_picture(noise: np.ndarray) -> None:
    """The gradient at 90 degrees is 0 at the top and 1 at the bottom, row r at (r + 0.5) / 100; a band 0.3 wide
    around 0.5 holds the rows from 35 to 64, the closest rows beyond it are blurred."""
    across = recipe.Recipe(
        [recipe.Step(NAME, params={"field": "across", "focus": 0.5, "width": 0.3, "feather": 0.2, "radius": 6})],
        name="band",
        size=None,
        regions={"across": recipe.LinearGradient(angle=90)},
    )
    out = recipe.render(noise, across)
    np.testing.assert_array_equal(out[35:65], noise[35:65])
    assert np.abs(out[:30] - noise[:30]).max() > 0.05
    assert np.abs(out[70:] - noise[70:]).max() > 0.05


def test_a_driven_grade_adds_colour_only_where_the_picture_is_blurred() -> None:
    """The miniature look: a flat (0.8, 0.4, 0.2) has luma 0.2126 x 0.8 + 0.7152 x 0.4 + 0.0722 x 0.2 = 0.4706 (Rec.
    709); a boost of 0.5 pushes each channel 1.5 times as far from it: 0.9647, 0.3647, 0.0647. The `grade` is driven
    by the band that stays sharp (1 inside it, falling to 0 over `feather`), so rows far beyond the band have the
    boost and the rows in it have none."""
    flat = np.tile(np.array([0.8, 0.4, 0.2], np.float32), (HEIGHT, WIDTH, 1))
    miniature = recipe.Recipe(
        [
            recipe.Step(NAME, params={"field": "across", "focus": 0.0, "width": 0.1, "feather": 0.2, "radius": 3}),
            recipe.Step("grade", params={"saturation": recipe.Drive(by="sharp", start=1.5, end=1.0)}),
        ],
        name="miniature",
        size=None,
        regions={
            "across": recipe.LinearGradient(angle=90),
            "sharp": recipe.Band(of="across", low=0.0, high=0.05, feather=0.2),
        },
    )
    out = recipe.render(flat, miniature)
    np.testing.assert_allclose(out[-1, 5], [0.9647, 0.3647, 0.0647], atol=1e-4)
    np.testing.assert_allclose(out[0], flat[0], atol=1e-6)


@pytest.mark.parametrize("named", ["subject", "depth"])
def test_a_step_follows_the_map_the_photo_is_given_under_the_name_it_names(noise: np.ndarray, named: str) -> None:
    """The map falls from 1 at the left to 0 at the right: focus 0.5 and a band of 0.2 keep the columns where it lies
    within 0.1 of 0.5 as they are, and the farthest columns are blurred."""
    out = fx.focus_blur(field=named, focus=0.5, width=0.2, radius=40.0).apply(noise, maps={named: RAMP})
    in_focus = np.abs(RAMP[0] - 0.5) <= 0.1
    np.testing.assert_array_equal(out[:, in_focus], noise[:, in_focus])
    assert np.abs(out[:, 0] - noise[:, 0]).max() > 0.05
    assert np.abs(out[:, -1] - noise[:, -1]).max() > 0.05
