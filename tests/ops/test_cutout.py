"""Tests of `ops/cutout.py`: laying a layer behind a subject, against values worked out by hand."""

import numpy as np
import pytest

from photo_fx.ops.cutout import put_behind

PICTURE = np.array([[[0.8, 0.4, 0.2]]], np.float32)  # one pixel
RED = np.array([1.0, 0.0, 0.0], np.float32)


@pytest.mark.parametrize(
    ("matte", "alpha", "expected"),
    [
        pytest.param(0.0, 1.0, [1.0, 0.0, 0.0], id="a full layer behind nothing replaces the picture"),
        pytest.param(0.0, 0.0, [0.8, 0.4, 0.2], id="an empty layer changes nothing"),
        pytest.param(0.0, 0.5, [0.9, 0.2, 0.1], id="half a layer is the average of picture and colour"),
        pytest.param(1.0, 1.0, [0.8, 0.4, 0.2], id="the subject is never covered"),
        pytest.param(
            0.25, 1.0, [0.95, 0.1, 0.05], id="a quarter subject at the edge leaves three quarters to the layer"
        ),
    ],
)
def test_the_layer_shows_only_where_the_subject_is_not(matte: float, alpha: float, expected: list[float]) -> None:
    out = put_behind(PICTURE, np.full((1, 1), matte, np.float32), np.full((1, 1), alpha, np.float32), RED)
    np.testing.assert_allclose(out, [[expected]], atol=1e-6)


def test_where_the_subject_is_whole_or_the_layer_empty_the_picture_is_returned_to_the_last_bit(
    photo: np.ndarray,
) -> None:
    matte = np.zeros(photo.shape[:2], np.float32)
    matte[:, :64] = 1.0
    alpha = np.full(photo.shape[:2], 0.7, np.float32)
    alpha[:, 100:] = 0.0
    out = put_behind(photo, matte, alpha, RED)
    np.testing.assert_array_equal(out[:, :64], photo[:, :64])
    np.testing.assert_array_equal(out[:, 100:], photo[:, 100:])
    assert not np.array_equal(out[:, 64:100], photo[:, 64:100])


def test_the_layer_may_be_a_picture_and_its_opacity_one_number() -> None:
    picture = np.array([[[0.8, 0.4, 0.2], [0.8, 0.4, 0.2]]], np.float32)
    layer = np.array([[[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]], np.float32)  # red behind the left pixel, blue the right
    out = put_behind(picture, np.zeros((1, 2), np.float32), 0.5, layer)
    np.testing.assert_allclose(out, [[[0.9, 0.2, 0.1], [0.4, 0.2, 0.6]]], atol=1e-6)
