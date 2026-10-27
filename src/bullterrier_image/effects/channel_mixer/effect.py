"""The `channel_mixer` effect: each output channel as a weighted sum of the input red, green and blue."""

import numpy as np

from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect


class ChannelMixerParams(EffectParams):
    """A 3x3 matrix, one weight per pair of channels, that builds each output channel from the input channels.

    Named `<output>_from_<input>`, so a row of the matrix is one output channel. The defaults are the identity matrix.
    A row summing to 1 keeps greys at their brightness in that channel.
    """

    red_from_red: float = param(
        1.0,
        unit="ratio",
        min=-3,
        max=3,
        doc="Weight of input red in output red, -3..3: 1 keeps it as it is (default), "
        "0 removes it, above 1 amplifies it, a negative weight inverts it.",
    )
    red_from_green: float = param(
        0.0,
        unit="ratio",
        min=-3,
        max=3,
        doc="Weight of input green in output red, -3..3: 1 passes it through, 0 leaves it out, "
        "a negative weight subtracts it.",
    )
    red_from_blue: float = param(
        0.0,
        unit="ratio",
        min=-3,
        max=3,
        doc="Weight of input blue in output red, -3..3: 1 passes it through, 0 leaves it out, "
        "a negative weight subtracts it.",
    )
    green_from_red: float = param(
        0.0,
        unit="ratio",
        min=-3,
        max=3,
        doc="Weight of input red in output green, -3..3: 1 passes it through, 0 leaves it out, "
        "a negative weight subtracts it.",
    )
    green_from_green: float = param(
        1.0,
        unit="ratio",
        min=-3,
        max=3,
        doc="Weight of input green in output green, -3..3: 1 keeps it as it is (default), "
        "0 removes it, above 1 amplifies it, a negative weight inverts it.",
    )
    green_from_blue: float = param(
        0.0,
        unit="ratio",
        min=-3,
        max=3,
        doc="Weight of input blue in output green, -3..3: 1 passes it through, 0 leaves it out, "
        "a negative weight subtracts it.",
    )
    blue_from_red: float = param(
        0.0,
        unit="ratio",
        min=-3,
        max=3,
        doc="Weight of input red in output blue, -3..3: 1 passes it through, 0 leaves it out, "
        "a negative weight subtracts it.",
    )
    blue_from_green: float = param(
        0.0,
        unit="ratio",
        min=-3,
        max=3,
        doc="Weight of input green in output blue, -3..3: 1 passes it through, 0 leaves it out, "
        "a negative weight subtracts it.",
    )
    blue_from_blue: float = param(
        1.0,
        unit="ratio",
        min=-3,
        max=3,
        doc="Weight of input blue in output blue, -3..3: 1 keeps it as it is (default), "
        "0 removes it, above 1 amplifies it, a negative weight inverts it.",
    )


@effect(category="colour", params=ChannelMixerParams)
def channel_mixer(rgb: np.ndarray, params: ChannelMixerParams) -> np.ndarray:
    """Build each output channel as a weighted sum of the input red, green and blue, set by a 3x3 matrix of weights:
    black and white, infrared, colour swaps.

    The sum is made on the gamma-encoded values, as in Photoshop's Channel Mixer, not in linear light. Row i of the
    matrix (the `<output>_from_<input>` weights) is output channel i, so
    `out[..., i] = sum_j matrix[i][j] * in[..., j]`. The result is clipped to 0..1.
    """
    rows = np.array(
        [
            [params.red_from_red, params.red_from_green, params.red_from_blue],
            [params.green_from_red, params.green_from_green, params.green_from_blue],
            [params.blue_from_red, params.blue_from_green, params.blue_from_blue],
        ],
        dtype=np.float32,
    )
    return np.clip(rgb @ rows.T, 0.0, 1.0).astype(np.float32)
