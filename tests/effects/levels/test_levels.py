"""The shipped presets of the `levels` effect; its behaviour is tested in Rust."""

import numpy as np

from bullterrier_image import fx
from bullterrier_image.recipe import Recipe, render


def test_the_invert_preset_makes_every_channel_value_v_one_minus_v(photo: np.ndarray) -> None:
    negative = render(photo, Recipe([fx.levels(preset="invert")], name="negative", size=None))
    np.testing.assert_allclose(negative, 1.0 - photo, atol=1e-6)
