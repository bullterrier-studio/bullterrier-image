"""The facade of `starburst`: its preset and the `size_ratio` a recipe hands it. The spikes are tested in Rust."""

import numpy as np
import pytest

from bullterrier_image import Recipe, fx, render
from bullterrier_image.core import registry

NAME = "starburst"
SIZE = 201


def test_the_preset_sparkle_scatters_glints_over_a_bright_area_where_plain_spikes_flood_it() -> None:
    """A bright block: the preset's spikes without its spacing wrap the whole block in light; with it a few small
    separate glints are lit."""
    picture = np.zeros((SIZE, SIZE, 3), np.float32)
    picture[60:140, 60:140] = 0.95
    sparkle = registry.presets(NAME)["sparkle"].params
    glitter = registry.apply(NAME, picture, **sparkle, seed=1)
    flood = registry.apply(NAME, picture, **{**sparkle, "spacing": 0.0})
    lit_glitter = (glitter - picture).max(axis=-1) > 0.02
    lit_flood = (flood - picture).max(axis=-1) > 0.02
    assert lit_glitter.any()
    assert lit_glitter.sum() < lit_flood.sum() / 4


@pytest.mark.parametrize(("spacing", "design_spacing"), [(0.0, 0.0), (25.8, 12.9)], ids=["every point", "a random few"])
def test_a_call_with_size_ratio_equals_the_recipe_rendered_at_that_ratio(spacing: float, design_spacing: float) -> None:
    """A recipe rendered at 960 hands the effect `size_ratio` 2 and doubles its px values; calling the effect directly
    with those values and `size_ratio=2` gives the same picture."""
    picture = np.zeros((960, 960, 3), np.float32)
    picture[300:660, 300:660] = 0.9
    step = fx.starburst(length=15.0, width=1.0, threshold=0.5, spacing=design_spacing, seed=3, variation=0.3)
    by_recipe = render(picture, Recipe(name="two", steps=[step]), size=960)
    by_call = registry.apply(
        NAME, picture, length=30.0, width=2.0, threshold=0.5, spacing=spacing, seed=3, variation=0.3, size_ratio=2.0
    )
    np.testing.assert_array_equal(by_call, by_recipe)
    assert not np.array_equal(
        by_call, registry.apply(NAME, picture, length=30.0, width=2.0, threshold=0.5)
    )  # it matters


def test_a_call_without_a_recipe_has_a_size_ratio_of_1() -> None:
    picture = np.zeros((SIZE, SIZE, 3), np.float32)
    picture[60:140, 60:140] = 0.9
    plain = registry.apply(NAME, picture, threshold=0.5, length=8.0)
    np.testing.assert_array_equal(plain, registry.apply(NAME, picture, threshold=0.5, length=8.0, size_ratio=1.0))
    assert not np.array_equal(plain, registry.apply(NAME, picture, threshold=0.5, length=8.0, size_ratio=2.0))
