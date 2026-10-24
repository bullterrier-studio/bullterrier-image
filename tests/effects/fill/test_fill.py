"""Behaviour of the `fill` effect."""

import numpy as np
import pytest

from bullterrier_image.core import registry


def test_fill_ignores_the_picture(photo: np.ndarray) -> None:
    out = registry.apply("fill", photo, color="#102030")
    assert np.allclose(out, [0x10 / 255, 0x20 / 255, 0x30 / 255])


def test_fill_refuses_a_colour_it_cannot_read(photo: np.ndarray) -> None:
    with pytest.raises(ValueError, match=r"fill.*`color`.*'#12345'"):
        registry.apply("fill", photo, color="#12345")
