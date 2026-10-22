"""Properties of the seeded noise fields: determinism, range, crop consistency, feature size, no lattice artefacts."""

import numpy as np
import pytest

from photo_fx.ops.noise import fractal_noise

KINDS = ["gradient", "value"]


def _correlation(a: np.ndarray, b: np.ndarray) -> float:
    return float(np.corrcoef(a.ravel(), b.ravel())[0, 1])


@pytest.mark.parametrize("kind", KINDS)
class TestFractalNoiseContract:
    def test_same_arguments_give_identical_arrays(self, kind):
        a = fractal_noise((64, 80), 16, 5, octaves=3, kind=kind)
        b = fractal_noise((64, 80), 16, 5, octaves=3, kind=kind)
        assert np.array_equal(a, b)

    def test_different_seed_gives_an_unrelated_field(self, kind):
        a = fractal_noise((128, 128), 16, 1, kind=kind)
        b = fractal_noise((128, 128), 16, 2, kind=kind)
        assert abs(_correlation(a, b)) < 0.1

    def test_float32_requested_shape_and_range_for_non_square(self, kind):
        field = fractal_noise((40, 90), 12, 3, octaves=3, kind=kind)
        assert field.dtype == np.float32
        assert field.shape == (40, 90)
        assert field.min() >= 0.0
        assert field.max() <= 1.0

    def test_crop_of_a_larger_field_equals_the_smaller_field(self, kind):
        # 37.5 divides neither 50 nor 100, so a lattice that depended on the canvas size would show up.
        big = fractal_noise((100, 100), 37.5, 7, octaves=3, kind=kind)
        small = fractal_noise((50, 50), 37.5, 7, octaves=3, kind=kind)
        assert np.array_equal(big[:50, :50], small)

    def test_crop_consistency_holds_for_non_square_canvases(self, kind):
        wide = fractal_noise((30, 120), 20, 4, octaves=2, kind=kind)
        narrow = fractal_noise((30, 45), 20, 4, octaves=2, kind=kind)
        assert np.array_equal(wide[:, :45], narrow)


def test_gradient_noise_uses_most_of_the_range():
    field = fractal_noise((256, 256), 64, 1)
    low, high = np.percentile(field, [1, 99])
    assert low < 0.25
    assert high > 0.75
