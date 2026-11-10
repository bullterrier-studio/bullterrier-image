"""The presets of `drop_shadow`, through the facade: the data keeps its parameter names and its look. What the effect
does is tested in the core (`crates/core/tests/it/drop_shadow.rs`)."""

import numpy as np

from bullterrier_image.core import registry
from bullterrier_image.core.steps import Step

NAME = "drop_shadow"
GREY = 0.8


def test_the_long_shadow_preset_is_a_solid_shadow_that_fades_out_over_300_px() -> None:
    """A 40 px wide band that fills the picture's height, the preset's shadow turned to the right: dark beside the
    subject, lighter at every pixel farther on, gone 300 px past the subject's edge."""
    subject = np.zeros((6, 380), np.float32)
    subject[:, 10:50] = 1.0
    picture = np.full((6, 380, 3), GREY, np.float32)
    out = registry.apply(NAME, picture, field=subject, **{**Step(NAME, preset="long_shadow").params, "angle": 0})
    removed = GREY - out[3, :, 0]
    assert removed[50] > 0.9 * GREY  # beside the subject
    assert (np.diff(removed[50:]) <= 1e-6).all()  # never darker farther out
    assert removed[290] > 0.2 * GREY  # still there 240 px out
    assert removed[330] < 0.1 * GREY  # nearly gone 280 px out
    np.testing.assert_allclose(removed[352:], 0.0, atol=1e-6)  # nothing past the last copy
