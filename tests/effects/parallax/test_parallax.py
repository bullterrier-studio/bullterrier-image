"""`parallax` reading its field from the recipe's maps. The kernel is tested in `crates/core/tests/it/parallax.rs`."""

import numpy as np

from bullterrier_image import Recipe, fx, render


def test_the_subject_map_moves_as_one_layer_over_a_still_background() -> None:
    """A bright 16 px square, the subject, on a picture 480 px wide (so design px are px), with the focus on the
    background: it moves 8 px to the right, whole, and every other pixel stays dark."""
    picture = np.full((96, 480, 3), 0.1, np.float32)
    square = np.zeros((96, 480), bool)
    square[40:56, 200:216] = True
    picture[square] = 0.9
    recipe = Recipe([fx.parallax(field="subject", amount=8, focus=0, angle=0, interpolation="nearest")], size=None)
    out = render(picture, recipe, maps={"subject": square.astype(np.float32)})
    np.testing.assert_array_equal(out[..., 0] > 0.5, np.roll(square, 8, axis=1))
