"""Behaviour of the `fill` effect."""

import numpy as np
import pytest

from photo_fx.core import registry
from photo_fx.ops.color import parse_color


def test_fill_ignores_the_picture(photo: np.ndarray) -> None:
    out = registry.apply("fill", photo, color="#102030")
    assert np.allclose(out, parse_color("#102030"))


def test_fill_refuses_a_colour_it_cannot_read(photo: np.ndarray) -> None:
    with pytest.raises(ValueError, match=r"fill.*`color`.*'#12345'"):
        registry.apply("fill", photo, color="#12345")
