"""The rule for the values of a map of a photo."""

import numpy as np
import pytest

from bullterrier_image.ops.maps import map_problem


@pytest.mark.parametrize(
    "layer",
    [np.zeros((2, 3)), np.ones((2, 3)), np.full((2, 3), 0.5), np.array([[0.0, 1.0]])],
    ids=["all 0", "all 1", "inside", "both bounds"],
)
def test_values_from_0_to_1_are_no_problem(layer):
    assert map_problem(layer) is None


@pytest.mark.parametrize(
    ("layer", "words"),
    [
        (np.array([[0.5, -0.25]]), ["below 0", "-0.25"]),
        (np.array([[0.5, 1.5]]), ["above 1", "1.5"]),
        (np.array([[0.5, np.nan]]), ["not finite"]),
        (np.array([[0.5, np.inf]]), ["not finite"]),
    ],
    ids=["below", "above", "nan", "infinity"],
)
def test_values_outside_0_to_1_or_not_finite_are_a_problem_saying_which(layer, words):
    problem = map_problem(layer)
    assert problem is not None
    for word in words:
        assert word in problem
