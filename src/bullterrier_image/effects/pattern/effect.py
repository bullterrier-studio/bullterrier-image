"""The `pattern` effect: stripes, checks, dots or a grid in two colours in place of the picture.

Its parameters are declared by the Rust struct `Params` in `crates/core/src/effects/pattern.rs`.
"""

import numpy as np

from bullterrier_image._native import _core
from bullterrier_image.core.params import EffectParams
from bullterrier_image.core.registry import effect


class PatternParams(EffectParams, core="pattern"):  # noqa: D101  (documented by its Rust declaration)
    pass


@effect(params=PatternParams)
def pattern(rgb: np.ndarray, params: PatternParams) -> np.ndarray:
    """Replace the picture with a repeating pattern of stripes, checks, dots or a grid in two colours.

    A generated background: the input is used only for its height and width, its content is ignored. The pattern is
    anchored at the top-left corner, so it does not shift when the picture is resized. Every shape is drawn from
    its signed distance in px to its border, so edges have the same softness at every angle.
    """
    return _core.apply("pattern", rgb, params.model_dump())
