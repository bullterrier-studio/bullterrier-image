"""Fixtures the tests of several effects share."""

from collections.abc import Callable

import numpy as np
import pytest


@pytest.fixture
def lone_particle() -> Callable[..., np.ndarray]:
    """`lone_particle(light, seed, **settings)`: the layer of light of a single particle that is not cut off by the
    picture's edge. `light` takes one dict of settings and returns the (h, w) layer; the seed is the first from `seed`
    on whose layer is dark along all four edges."""

    def find(light: Callable[[dict], np.ndarray], seed: int, **settings: object) -> np.ndarray:
        for candidate in range(seed, seed + 50):
            layer = light({**settings, "seed": candidate})
            if layer[0].max() == 0 == layer[-1].max() == layer[:, 0].max() == layer[:, -1].max():
                return layer
        raise AssertionError("every one of 50 seeds put the particle on the picture's edge")

    return find
