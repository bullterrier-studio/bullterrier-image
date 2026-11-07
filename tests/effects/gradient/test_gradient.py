"""Behaviour of the `gradient` effect that Python owns: its presets and its errors. The layouts are tested in Rust."""

import numpy as np
import pytest

from bullterrier_image import Recipe, RecipeError, Step
from bullterrier_image import render as render_recipe
from bullterrier_image.core import registry

NAME = "gradient"
GREEN = (0.0, 1.0, 0.0)


def test_a_gradient_needs_at_least_one_colour() -> None:
    with pytest.raises(RecipeError, match="colors"):
        registry.apply(NAME, np.zeros((4, 4, 3), np.float32), colors=[])


def test_the_fill_preset_is_a_flat_black_picture_that_ignores_the_input() -> None:
    """Preset `fill` keeps the old `fill` effect findable: black by default, any one colour with `colors`."""
    photo = np.random.default_rng(0).random((20, 30, 3), dtype=np.float32)
    black = render_recipe(photo, Recipe([Step("gradient", preset="fill")], name="t", size=None))
    np.testing.assert_array_equal(black, np.zeros((20, 30, 3), np.float32))
    green = render_recipe(
        photo, Recipe([Step("gradient", params={"colors": ["#00ff00"]}, preset="fill")], name="t", size=None)
    )
    np.testing.assert_array_equal(green, np.broadcast_to(np.array(GREEN, np.float32), (20, 30, 3)))


def test_the_fill_preset_masked_to_the_background_blacks_out_only_the_background() -> None:
    photo = np.random.default_rng(1).random((20, 30, 3), dtype=np.float32) * 0.5 + 0.25
    subject = np.zeros((20, 30), np.float32)
    subject[:, 15:] = 1
    recipe = Recipe([Step("gradient", preset="fill", mask="background")], name="t", size=None)
    out = render_recipe(photo, recipe, maps={"subject": subject})
    np.testing.assert_array_equal(out[:, 15:], photo[:, 15:])
    np.testing.assert_array_equal(out[:, :15], np.zeros((20, 15, 3), np.float32))


def test_the_fill_preset_and_the_summary_say_how_to_replace_the_background() -> None:
    assert 'mask = "background"' in registry.presets(NAME)["fill"].description
    assert 'mask = "background"' in registry.summary(NAME)
