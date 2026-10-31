"""The facade over the Rust extension: an error the core reports comes back as the exception of its kind.

The extension raises `_core.CoreError` (a `ValueError`) carrying a `kind`; `native` turns that into the
`BullterrierImageError` subclass of the kind and leaves every other exception alone, so a caller catches the package's
exceptions whatever ran the work.
"""

import pytest

from bullterrier_image._native import _core  # noqa: PLC2701  the kinds the core can report, and a CoreError to translate
from bullterrier_image.core import native
from bullterrier_image.errors import (
    DepthNotFoundError,
    InvalidImageError,
    MapNotFoundError,
    NonFiniteImageError,
    RecipeError,
    SubjectNotFoundError,
    UnknownEffectError,
)

# What each kind the core reports is raised as: the table of `Error` in crates/core/src/error.rs, written out
RAISED_AS = {
    "recipe": RecipeError,
    "unknown_effect": UnknownEffectError,
    "subject_not_found": SubjectNotFoundError,
    "depth_not_found": DepthNotFoundError,
    "map_not_found": MapNotFoundError,
    "invalid_image": InvalidImageError,
    "non_finite_image": NonFiniteImageError,
}


def core_error(kind: str, message: str = "it went wrong") -> _core.CoreError:
    """A `CoreError` of `kind`, as the extension raises one."""
    error = _core.CoreError(message)
    error.kind = kind
    return error


@pytest.mark.parametrize(("kind", "raised"), RAISED_AS.items(), ids=RAISED_AS)
def test_an_error_of_a_kind_is_raised_as_the_exception_of_that_kind(kind, raised):
    @native.entry
    def failing():
        raise core_error(kind, "the message")

    with pytest.raises(raised, match="the message") as caught:
        failing()
    assert type(caught.value) is raised
    assert isinstance(caught.value.__cause__, _core.CoreError)


def test_the_kinds_the_core_reports_are_exactly_the_ones_the_facade_knows_and_other():
    assert set(_core.vocabulary()["error_kinds"]) == {*RAISED_AS, "other"}


def test_an_error_of_the_other_kind_is_left_as_the_value_error_it_is():
    @native.entry
    def failing():
        raise core_error("other")

    with pytest.raises(_core.CoreError) as caught:
        failing()
    assert isinstance(caught.value, ValueError)
    assert not isinstance(caught.value, RecipeError)


@pytest.mark.parametrize(
    "raised",
    [
        pytest.param(KeyError("amount"), id="a builtin"),
        pytest.param(ValueError("a plain value error"), id="a value error that is no CoreError"),
        pytest.param(RecipeError("what a Python effect raised"), id="an exception of the package"),
    ],
)
def test_an_exception_that_is_no_error_of_the_core_passes_through_as_it_is(raised):
    @native.entry
    def failing():
        raise raised

    with pytest.raises(type(raised)) as caught:
        failing()
    assert caught.value is raised


def test_an_error_of_a_kind_the_facade_does_not_know_is_not_hidden_behind_another():
    @native.entry
    def failing():
        raise core_error("a_kind_from_the_future")

    with pytest.raises(KeyError, match="a_kind_from_the_future"):
        failing()


def test_a_call_that_succeeds_gives_its_result_and_takes_its_arguments_as_they_are():
    @native.entry
    def add(left, right=0):
        return left + right

    assert add(1, right=2) == 3


def test_an_entry_point_of_the_core_raises_the_exception_of_the_kind_the_core_reports():
    with pytest.raises(UnknownEffectError, match="no_such_effect"):
        native.params("no_such_effect")
    with pytest.raises(UnknownEffectError, match="no_such_effect"):
        native.effect("no_such_effect")
    assert native.effect("grade")["name"] == "grade"
