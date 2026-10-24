"""Behaviour of the `solarize` effect."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "solarize"


@pytest.mark.parametrize(
    ("threshold", "value", "result"),
    [
        pytest.param(0.5, 0.3, 0.3, id="below the threshold a value stays"),
        pytest.param(0.5, 0.8, 0.2, id="above the threshold a value is inverted"),
        pytest.param(0.4, 0.4, 0.4, id="exactly at the threshold a value stays"),
        pytest.param(0.4, 0.45, 0.55, id="just above the threshold a value is inverted"),
        pytest.param(0.0, 0.75, 0.25, id="threshold 0 inverts everything above black"),
        pytest.param(0.0, 0.0, 0.0, id="threshold 0 leaves black alone"),
        pytest.param(1.0, 0.95, 0.95, id="threshold 1 changes nothing"),
        pytest.param(0.8, 0.9, 0.1, id="a high threshold inverts only the brightest"),
        pytest.param(0.8, 0.7, 0.7, id="a high threshold leaves the rest"),
    ],
)
def test_a_value_is_inverted_only_when_it_is_above_the_threshold(threshold: float, value: float, result: float) -> None:
    picture = np.full((1, 1, 3), value, np.float32)
    np.testing.assert_allclose(registry.apply(NAME, picture, threshold=threshold), result, atol=1e-6)


def test_each_channel_is_judged_on_its_own() -> None:
    picture = np.array([[[0.2, 0.8, 0.6]]], np.float32)
    np.testing.assert_allclose(registry.apply(NAME, picture, threshold=0.5)[0, 0], [0.2, 0.2, 0.4], atol=1e-6)


@pytest.mark.parametrize("threshold", [0.0, 0.25, 0.5, 0.9], ids=lambda t: f"threshold {t}")
def test_a_photo_is_its_own_negative_above_the_threshold_and_untouched_elsewhere(
    photo: np.ndarray, threshold: float
) -> None:
    expected = np.where(photo > threshold, 1 - photo, photo)  # the definition, applied to every value of the photo
    np.testing.assert_allclose(registry.apply(NAME, photo, threshold=threshold), expected, atol=1e-6)
