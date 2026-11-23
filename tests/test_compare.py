"""The `assert_looks_the_same` fixture of conftest.py, shown a pair it must accept and pairs it must refuse."""

import numpy as np
import pytest

THOUSAND = np.full((10, 100), 0.4, np.float32)


def moved(*by_levels: tuple[int, int]) -> np.ndarray:
    """THOUSAND with the samples at the given flat indices raised by the given levels of 255."""
    other = THOUSAND.copy().reshape(-1)
    for at, levels in by_levels:
        other[at] += levels / 255
    return other.reshape(THOUSAND.shape)


def test_samples_one_level_off_and_one_in_a_thousand_two_levels_off_look_the_same(assert_looks_the_same):
    assert_looks_the_same(moved((1, 1), (2, -1), (3, 1), (500, 1), (999, -1)), THOUSAND)
    assert_looks_the_same(moved((5, 2)), THOUSAND)


def test_a_sample_three_levels_off_does_not_look_the_same(assert_looks_the_same):
    with pytest.raises(AssertionError, match="3 levels apart"):
        assert_looks_the_same(moved((7, 3)), THOUSAND)


def test_more_than_a_thousandth_of_the_samples_two_levels_off_does_not_look_the_same(assert_looks_the_same):
    with pytest.raises(AssertionError, match="2 levels apart"):
        assert_looks_the_same(moved((5, 2), (6, 2)), THOUSAND)


def test_pictures_of_other_shapes_or_with_a_nan_do_not_look_the_same(assert_looks_the_same):
    with pytest.raises(AssertionError, match="shape"):
        assert_looks_the_same(THOUSAND[:, :99], THOUSAND)
    with pytest.raises(AssertionError, match="not a number"):
        assert_looks_the_same(moved((0, 0)) * np.nan, THOUSAND)
