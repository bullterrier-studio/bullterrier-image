"""Behaviour of the `gradient_map` effect that Python owns: its errors. The mapping is tested in Rust."""

import numpy as np
import pytest

from bullterrier_image.core import registry


def test_gradient_map_needs_two_stops(photo: np.ndarray) -> None:
    with pytest.raises(ValueError, match="stops"):
        registry.apply("gradient_map", photo, stops=["#000000"])
