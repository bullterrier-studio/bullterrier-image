"""The `interlace` effect: every other row slid sideways, as two fields of an interlaced frame that fell out of step."""

from typing import Literal

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.ops.shift import shift


class InterlaceParams(EffectParams):
    """The settings of `interlace`: how far one field slides, which one, and what fills the gap."""

    offset: float = param(
        6.0,
        unit="px",
        min=-500,
        max=500,
        doc="How far the slid field moves sideways, -500 to 500 px, tuned for a picture about 480 px high: positive "
        "moves its rows right, negative left, 0 leaves the picture unchanged. A distance between two pixels blends "
        "them. Raising it widens the teeth of the comb along every vertical edge.",
    )
    field: Literal["odd", "even"] = param(
        "odd",
        doc="Which field slides: 'odd' the second, fourth, sixth... row (counting from the top as the first), 'even' "
        "the first, third, fifth... row. The other field stays where it was.",
    )
    edge: Literal["wrap", "nearest", "reflect", "mirror"] = param(
        "nearest",
        doc="What fills the gap the slide opens at the side: 'nearest' holds the row's last pixel there, 'wrap' "
        "brings back what left on the other side, 'reflect' mirrors the row (the edge pixel repeated once), "
        "'mirror' mirrors it about the edge pixel.",
        advanced=True,
    )


@effect(category="glitch", params=InterlaceParams)
def interlace(rgb: np.ndarray, params: InterlaceParams) -> np.ndarray:
    """Slide every other row of the picture sideways by `offset` px, leaving the rows between untouched.

    The two fields of an interlaced frame are the odd and the even rows; when the picture moved between them, every
    vertical edge breaks into a comb. Here one field (`field`) is moved by `offset` and the other stays, which gives
    that comb on a still picture. What the slide uncovers at the side is set by `edge`.
    """
    out = rgb.copy()
    first = int(
        params.field == "odd"
    )  # the index of the first row of the slid field: odd rows are the second, fourth...
    # constant: no vertical distance, the rows only slide sideways
    out[first::2] = shift(rgb[first::2], 0, params.offset, edge=params.edge)
    return out
