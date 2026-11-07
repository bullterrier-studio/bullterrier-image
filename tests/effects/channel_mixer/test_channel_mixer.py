"""The shipped presets of the `channel_mixer` effect; its behaviour is tested in Rust."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "channel_mixer"
TOLERANCE = 2 * float(np.finfo(np.float32).eps)  # 2 x float32 epsilon: float32 rounding of a sum of three products
MONOCHROME = registry.presets(NAME)["black_and_white"].params


@pytest.mark.parametrize(
    ("pixel", "expected"),
    [
        # Rec. 709 luma weights (ITU-R BT.709), published as 0.2126 / 0.7152 / 0.0722.
        pytest.param((1, 0, 0), 0.2126, id="pure red has the luma of red"),
        pytest.param((0, 1, 0), 0.7152, id="pure green has the luma of green"),
        pytest.param((0, 0, 1), 0.0722, id="pure blue has the luma of blue"),
        pytest.param((0.2, 0.4, 0.6), 0.2126 * 0.2 + 0.7152 * 0.4 + 0.0722 * 0.6, id="a mixed colour"),
    ],
)
def test_the_black_and_white_preset_gives_every_channel_the_rec_709_luma(
    pixel: tuple[float, float, float], expected: float
) -> None:
    out = registry.apply(NAME, np.array([[pixel]], np.float32), **MONOCHROME)
    np.testing.assert_allclose(out[0, 0], [expected] * 3, atol=TOLERANCE)
