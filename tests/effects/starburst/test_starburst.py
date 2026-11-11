"""The facade of `starburst`: the `size_ratio` a recipe hands it. The spikes and the sparkle look are tested in Rust."""

import numpy as np
import pytest

from bullterrier_image import Recipe, fx, render
from bullterrier_image.core import registry

NAME = "starburst"
SIZE = 201


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
