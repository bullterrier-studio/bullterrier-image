"""Contract tests every effect must pass, whatever it does."""

import numpy as np
import pytest

from photo_fx import effects

ALL = sorted(effects.describe())


@pytest.mark.parametrize("name", ALL)
def test_every_effect_keeps_shape_dtype_and_range(name, photo):
    out = effects.apply(name, photo)
    assert out.shape == photo.shape
    assert out.dtype == np.float32
    assert np.isfinite(out).all()
    assert out.min() >= -1e-6 and out.max() <= 1 + 1e-6


@pytest.mark.parametrize("name", ALL)
def test_every_effect_is_deterministic_and_leaves_its_input_alone(name, photo):
    before = photo.copy()
    first, second = effects.apply(name, photo), effects.apply(name, photo)
    np.testing.assert_array_equal(first, second)
    np.testing.assert_array_equal(photo, before)


@pytest.mark.parametrize("name", ALL)
def test_every_effect_documents_itself(name):
    assert effects.describe()[name]


def test_unknown_effect_lists_choices(photo):
    with pytest.raises(KeyError, match="ascii"):
        effects.apply("nope", photo)
