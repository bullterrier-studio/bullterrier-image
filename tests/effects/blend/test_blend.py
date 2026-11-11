"""The `blend` effect through the Python facade: a colour handed to a step. Its presets, and how a picture is fitted and
blended, are tested in Rust (`crates/core/tests/it/preset_looks.rs`, `blend_effect.rs`)."""

import numpy as np

from bullterrier_image import Recipe, fx, render


def test_a_colour_given_to_fx_is_a_flat_picture_and_none_leaves_the_photo_as_it_was(photo):
    red = render(photo, Recipe(name="red", steps=[fx.blend(image="#ff0000", mode="normal")]))
    np.testing.assert_array_equal(red, np.broadcast_to([1, 0, 0], photo.shape))
    np.testing.assert_array_equal(render(photo, Recipe(name="none", steps=[fx.blend()])), photo)
