"""The Rust extension's boundary, `_native._core.apply`: how the parameters Pydantic dumps reach the core.

Each Python value becomes the JSON value of its kind, which the effect's Rust `Params` then reads or refuses; the
refusal names the kind it got, so each row shows which kind the value became. Grade's `contrast` is a float, so a
whole number is read as that float and every other kind is refused with ValueError (a programming error: a user's
values are validated by the parameter model first).
"""

import math

import numpy as np
import pytest

from bullterrier_image._native import _core  # noqa: PLC2701  architecture: the boundary the effects call

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
