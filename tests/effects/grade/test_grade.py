"""The shipped presets of the `grade` effect; its behaviour is tested in Rust (`crates/core/tests/it/grade.rs`)."""

import numpy as np

from bullterrier_image.core import registry
from bullterrier_image.core.steps import Step


def test_the_white_balance_preset_takes_the_orange_out_of_a_warm_grey() -> None:
    assert "white_balance" in registry.presets("grade")
    warm = np.array([[[0.7, 0.6, 0.5]]], np.float32)
    out = registry.apply("grade", warm, **Step("grade", preset="white_balance").params)[0, 0]
    assert out[0] - out[2] < warm[0, 0, 0] - warm[0, 0, 2]  # red over blue shrinks: a warm cast is corrected
