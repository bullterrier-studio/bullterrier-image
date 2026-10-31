"""The Rust extension's boundary, `_native._core.apply`: how the parameters Pydantic dumps reach the core.

Each Python value becomes the JSON value of its kind, which the effect's Rust `Params` then reads or refuses; the
refusal names the kind it got, so each row shows which kind the value became. Grade's `contrast` is a float, so a
whole number is read as that float and every other kind is refused with ValueError (a programming error: a user's
values are validated by the parameter model first).
"""

import math
from collections.abc import Callable, Iterator
from contextlib import contextmanager

import numpy as np
import pytest

from bullterrier_image._native import _core  # noqa: PLC2701  architecture: the boundary the effects call
from bullterrier_image.core.registry import CATEGORIES, INPUTS

PIXEL = np.array([[[0.2, 0.5, 0.7]]], np.float32)  # contrast moves each channel differently
GRADE = {"temperature": 0.0, "tint": 0.0, "exposure": 0.0, "pivot": 0.5, "saturation": 1.0, "vibrance": 0.0}


def grade_with_contrast(contrast: object) -> np.ndarray:
    return _core.apply("grade", PIXEL, {**GRADE, "contrast": contrast})


def test_a_whole_number_is_read_as_the_float_it_equals():
    np.testing.assert_array_equal(grade_with_contrast(2), grade_with_contrast(2.0))
    np.testing.assert_allclose(grade_with_contrast(2.0)[0, 0], [0.0, 0.5, 0.9], atol=1e-6)  # 0.5 + (v - 0.5) * 2


@pytest.mark.parametrize(
    ("value", "kind"),
    [
        pytest.param(True, "boolean", id="a bool is a JSON boolean, not a number"),
        pytest.param("2", "string", id="a str is a JSON string"),
        pytest.param([2.0], "sequence", id="a list is a JSON array"),
        pytest.param((2.0,), "sequence", id="a tuple is a JSON array too"),
        pytest.param(None, "null", id="None is JSON null"),
        pytest.param({"value": 2.0}, "map", id="a dict is a JSON object"),
        pytest.param(math.nan, "not finite", id="NaN has no JSON form"),
        pytest.param(math.inf, "not finite", id="infinity has no JSON form"),
        pytest.param(2**70, "out of range", id="an integer beyond 64 bits has no JSON form"),
    ],
)
def test_a_value_the_effect_cannot_read_is_refused_naming_its_kind(value, kind):
    with pytest.raises(ValueError, match=kind):
        grade_with_contrast(value)


# --- params: what an effect's Rust struct declares -----------------------------------------------------------------

FIELD_KEYS = {"name", "kind", "doc", "default", "unit", "min", "max", "cyclic", "choices", "drivable", "advanced"}
KINDS = {"float", "int", "bool", "choice", "color"}


@pytest.mark.parametrize("effect", _core.effects())
def test_the_parameters_of_every_effect_are_a_doc_and_fields_of_a_known_kind_with_every_key(effect):
    table = _core.params(effect)
    assert set(table) == {"doc", "fields"}
    assert table["doc"].strip()
    assert table["fields"]
    for field in table["fields"]:
        assert set(field) == FIELD_KEYS
        assert field["kind"] in KINDS
        assert field["doc"].strip()
        assert (field["choices"] != []) == (field["kind"] == "choice")  # only a choice lists names


@pytest.mark.parametrize("effect", _core.effects())
def test_a_literal_keeps_its_kind_across_the_boundary_a_whole_number_is_an_int_and_a_decimal_a_float(effect):
    """The declaration's `0` and `0.0` are different values to the schema, the docs and the stub, so the table must
    not widen or round one into the other; `==` cannot tell them apart, so the types are compared."""
    for field in _core.params(effect)["fields"]:
        if field["kind"] == "int":
            assert [type(field[key]) for key in ("default", "min", "max")] == [int, int, int], field["name"]
        if field["kind"] == "float":
            assert type(field["default"]) is float, field["name"]
            assert {type(field[key]) for key in ("min", "max")} <= {int, float}, field["name"]


def test_the_parameters_of_an_effect_the_core_does_not_implement_are_refused_by_name():
    with pytest.raises(ValueError, match="no_such_effect"):
        _core.params("no_such_effect")


# --- the effects' metadata and the registry --------------------------------------------------------------------------


def test_the_core_names_the_categories_and_inputs_in_the_order_the_package_lists_them():
    assert _core.vocabulary() == {"categories": list(CATEGORIES), "inputs": list(INPUTS)}


@pytest.mark.parametrize(
    ("effect", "category"),
    [("grade", "colour"), ("pattern", "backgrounds"), ("pixel_grid", "texture")],
)
def test_a_core_effect_says_its_category_default_opacity_and_inputs(effect, category):
    assert _core.effect(effect) == {"name": effect, "category": category, "opacity": 1.0, "inputs": []}


def test_the_metadata_of_an_unknown_effect_is_refused_by_name():
    with pytest.raises(ValueError, match="no_such_effect"):
        _core.effect("no_such_effect")


def fill(level: float) -> Callable[[np.ndarray, dict], np.ndarray]:
    """A foreign effect: the picture filled with `level`."""
    return lambda rgb, params: np.full(rgb.shape, level, np.float32)


@contextmanager
def registered(name: str, function: Callable, category: str = "utilities", opacity: float = 1.0) -> Iterator[None]:
    """`function` registered as `name` for the block; the registry is the process's, so it is always removed."""
    _core.register(name, category, opacity, function)
    try:
        yield
    finally:
        _core.unregister(name)


def test_a_python_function_registered_as_an_effect_is_listed_described_and_run_by_the_core():
    seen: dict[str, object] = {}

    def double(rgb, params):
        seen.update(shape=rgb.shape, dtype=rgb.dtype, params=params)
        return rgb * 2

    with registered("probe_double", double, category="light", opacity=0.5):
        assert "probe_double" in _core.effects()
        assert _core.effects() == sorted(_core.effects())
        assert _core.effect("probe_double") == {
            "name": "probe_double",
            "category": "light",
            "opacity": 0.5,
            "inputs": [],
        }
        out = _core.apply("probe_double", PIXEL, {"amount": 3, "label": "x"})
    np.testing.assert_allclose(out, PIXEL * 2)
    assert seen == {"shape": (1, 1, 3), "dtype": np.float32, "params": {"amount": 3, "label": "x"}}


def test_an_unregistered_effect_is_gone_and_a_core_effect_cannot_be_unregistered():
    with registered("probe_gone", fill(0.5)):
        pass
    assert "probe_gone" not in _core.effects()
    with pytest.raises(ValueError, match="probe_gone"):
        _core.apply("probe_gone", PIXEL, {})
    with pytest.raises(ValueError, match="probe_gone"):
        _core.unregister("probe_gone")
    with pytest.raises(ValueError, match="grade"):
        _core.unregister("grade")
    assert "grade" in _core.effects()


@pytest.mark.parametrize("taken", ["grade", "probe_taken"])
def test_a_name_that_is_taken_is_refused_and_the_effect_that_has_it_stays(taken):
    with registered("probe_taken", fill(0.25)):
        with pytest.raises(ValueError, match=taken):
            _core.register(taken, "utilities", 1.0, fill(0.75))
        if taken == "probe_taken":
            np.testing.assert_allclose(_core.apply(taken, PIXEL, {}), 0.25)


def test_an_unknown_category_is_refused_and_registers_nothing():
    with pytest.raises(ValueError, match="sepia"):
        _core.register("probe_sepia", "sepia", 1.0, fill(0.5))
    assert "probe_sepia" not in _core.effects()


@pytest.mark.parametrize(
    ("opacity", "function", "said"),
    [
        pytest.param(math.nan, fill(0.5), "opacity", id="a NaN opacity"),
        pytest.param(math.inf, fill(0.5), "opacity", id="an infinite opacity"),
        pytest.param(-0.1, fill(0.5), "opacity", id="an opacity below 0"),
        pytest.param(1.5, fill(0.5), "opacity", id="an opacity above 1"),
        pytest.param(1.0, "not callable", "callable", id="a function that is not callable"),
        pytest.param(1.0, None, "callable", id="no function"),
    ],
)
def test_an_opacity_outside_0_to_1_or_a_function_that_cannot_be_called_is_refused_and_registers_nothing(
    opacity, function, said
):
    with pytest.raises(ValueError, match=said):
        _core.register("probe_refused", "utilities", opacity, function)
    assert "probe_refused" not in _core.effects()


def test_a_foreign_effect_describes_its_own_parameters():
    with registered("probe_own", fill(0.5)), pytest.raises(ValueError, match="probe_own"):
        _core.params("probe_own")


def fails(rgb, params):
    raise KeyError("amount")


@pytest.mark.parametrize(
    ("function", "said"),
    [
        pytest.param(fails, ["KeyError", "amount"], id="an exception is reported with its class and message"),
        pytest.param(lambda rgb, params: [[[0.5]]], ["ndarray"], id="a list is not an array"),
        pytest.param(lambda rgb, params: rgb.astype(np.float64), ["float32"], id="float64 is not float32"),
        pytest.param(lambda rgb, params: rgb[:, :, :2].copy(), ["[1, 1, 3]", "[1, 1, 2]"], id="a wrong shape"),
        pytest.param(
            lambda rgb, params: np.zeros((1, 1, 3, 1), np.float32), ["[1, 1, 3]", "[1, 1, 3, 1]"], id="a wrong rank"
        ),
    ],
)
def test_a_foreign_effect_that_fails_or_returns_no_picture_is_reported_naming_the_effect(function, said):
    with registered("probe_bad", function), pytest.raises(ValueError, match="probe_bad") as caught:
        _core.apply("probe_bad", PIXEL, {})
    assert all(word in str(caught.value) for word in said), str(caught.value)


def test_a_foreign_effect_may_run_a_core_effect_itself():
    """`apply` runs the core without the interpreter lock: the foreign effect takes it back, and may call in again."""

    def through_grade(rgb, params):
        return _core.apply("grade", rgb, {**GRADE, "contrast": 2.0})

    with registered("probe_nested", through_grade):
        np.testing.assert_allclose(_core.apply("probe_nested", PIXEL, {}), grade_with_contrast(2.0))
