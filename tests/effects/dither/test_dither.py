"""The presets of `dither`, through the facade: the data keeps its parameter names. What the effect does is tested in
the core (`crates/core/tests/it/dither.rs`)."""

import numpy as np
import pytest

from bullterrier_image import Recipe, fx, render

PHOTO = np.random.default_rng(7).random((24, 24, 3), dtype=np.float32)


def _levels(out: np.ndarray, channel: int) -> int:
    return len(np.unique(out[..., channel]))


@pytest.mark.parametrize(
    ("preset", "rule"),
    [
        pytest.param("posterize", lambda out: all(_levels(out, c) <= 4 for c in range(3)), id="4 levels per channel"),
        pytest.param("threshold", lambda out: set(np.unique(out)) <= {0.0, 1.0}, id="black and white only"),
        pytest.param(
            "palette_quantize", lambda out: len(np.unique(out.reshape(-1, 3), axis=0)) <= 8, id="at most 8 colours"
        ),
    ],
)
def test_each_shipped_preset_applies_and_gives_its_look(preset, rule) -> None:
    out = render(PHOTO, Recipe(name="t", size=None, steps=[fx.dither(preset=preset)]))
    assert rule(out)
