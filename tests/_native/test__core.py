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


def test_the_parameters_of_an_effect_come_in_declaration_order_with_the_struct_doc():
    table = _core.params("grade")
    assert [field["name"] for field in table["fields"]] == [
        "temperature",
        "tint",
        "exposure",
        "contrast",
        "pivot",
        "saturation",
        "vibrance",
    ]
    assert table["doc"].startswith("Colour balance")


def test_a_field_is_described_with_its_unit_range_and_flags_and_its_literals_keep_their_kind():
    temperature = _core.params("grade")["fields"][0]
    assert {key: temperature[key] for key in ("kind", "unit", "min", "max", "default", "cyclic", "drivable")} == {
        "kind": "float",
        "unit": "stops",
        "min": -2,
        "max": 2,
        "default": 0.0,
        "cyclic": False,
        "drivable": True,
    }
    # `==` cannot tell -2 from -2.0, and the schema text shows the difference
    assert type(temperature["min"]) is int
    assert type(temperature["default"]) is float
    assert temperature["choices"] == []
    assert temperature["advanced"] is False
    assert temperature["doc"].startswith("Warm-cool shift in stops")


def test_the_parameters_of_an_effect_the_core_does_not_implement_are_refused_by_name():
    with pytest.raises(ValueError, match="no_such_effect"):
        _core.params("no_such_effect")
