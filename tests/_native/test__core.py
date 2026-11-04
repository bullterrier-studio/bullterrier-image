"""The Rust extension's boundary, `_native._core.apply`: how the parameters Pydantic dumps reach the core.

Each Python value becomes the JSON value of its kind, which the effect's Rust `Params` then reads or refuses; the
refusal names the kind it got, so each row shows which kind the value became. Grade's `contrast` is a float, so a
whole number is read as that float and every other kind is refused with ValueError (a programming error: a user's
values are validated by the parameter model first).
"""

import math
import pickle
import threading
from collections.abc import Callable, Iterator
from contextlib import contextmanager
from typing import Any

import numpy as np
import pytest

from bullterrier_image._native import _core  # noqa: PLC2701  architecture: the boundary the effects call
from bullterrier_image.core.registry import CATEGORIES, INPUTS
from bullterrier_image.errors import RecipeError, SubjectNotFoundError

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

FIELD_KEYS = {"name", "kind", "meaning", "default", "unit", "min", "max", "cyclic", "choices", "drivable", "advanced"}
LIST_KEYS = {"min_length", "max_length"}  # written for a list of colours only; `max_length` only when it has a ceiling
KINDS = {"float", "int", "bool", "choice", "color", "colors", "lut"}


@pytest.mark.parametrize("effect", _core.effects(declared=True))
def test_the_parameters_of_every_effect_are_a_doc_and_fields_of_a_known_kind_with_every_key(effect):
    table = _core.params(effect)
    assert set(table) == {"doc", "fields"}
    assert table["doc"].strip()
    assert table["fields"]
    for field in table["fields"]:
        listed = {key for key in LIST_KEYS if key in field}
        assert set(field) == FIELD_KEYS | listed
        assert (field["kind"] == "colors") == ("min_length" in field)  # a list has a floor; nothing else has lengths
        assert field["kind"] in KINDS
        assert field["meaning"].strip()
        assert (field["choices"] != []) == (field["kind"] == "choice")  # only a choice lists names


@pytest.mark.parametrize("effect", _core.effects(declared=True))
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


# --- region_presets: the shipped presets of a region kind ------------------------------------------------------------

SHARD_PRESETS = {
    "glass": (
        "Dozens of splinters crowded round an impact point in the middle, big pieces far out, straight cracks.",
        {"count": 46, "jitter": 1.0, "focus": 2.5},
    ),
    "torn": (
        "A few big pieces with wide ragged edges, like paper torn apart by hand.",
        {"count": 6, "jitter": 0.7, "edge": {"style": "torn", "amount": 5.76, "scale": 33.6}},
    ),
    "stone": (
        "About a dozen cracked slabs with slightly rough edges, like a broken pavement.",
        {"count": 14, "jitter": 0.8, "edge": {"style": "torn", "amount": 2.4, "scale": 8.64}},
    ),
    "mosaic": (
        "Many small, nearly even tiles with straight edges, like a mosaic of cut glass.",
        {"count": 70, "jitter": 0.25},
    ),
}


def test_the_shard_presets_are_the_four_with_their_sentence_and_fields_in_file_order():
    found = _core.region_presets("shards")
    assert list(found) == list(SHARD_PRESETS)
    assert found == {
        name: {"description": sentence, "params": fields} for name, (sentence, fields) in SHARD_PRESETS.items()
    }
    assert type(found["glass"]["params"]["count"]) is int  # a whole number stays an int across the boundary


def test_a_region_kind_without_presets_is_refused_naming_the_kinds_that_have_them():
    with pytest.raises(ValueError, match=r"'grid'.*shards"):
        _core.region_presets("grid")


# --- the effects' metadata and the registry --------------------------------------------------------------------------


def test_the_core_names_the_categories_and_inputs_in_the_order_the_package_lists_them():
    vocabulary = _core.vocabulary()
    assert (vocabulary["categories"], vocabulary["inputs"]) == (list(CATEGORIES), list(INPUTS))


@pytest.mark.parametrize(
    ("effect", "category"),
    [("grade", "colour"), ("pattern", "backgrounds"), ("pixel_grid", "texture")],
)
def test_a_core_effect_says_its_category_default_opacity_and_inputs(effect, category):
    assert _core.effect(effect) == {
        "name": effect,
        "category": category,
        "opacity": 1.0,
        "inputs": [],
        "fields": {},
    }


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
            "fields": {},
        }
        out = _core.apply("probe_double", PIXEL, {"amount": 3, "label": "x"})
    np.testing.assert_allclose(out, PIXEL * 2)
    assert seen == {"shape": (1, 1, 3), "dtype": np.float32, "params": {"amount": 3, "label": "x"}}


def test_the_effects_the_core_declares_are_the_ones_it_runs_itself_not_the_ones_registered_with_it():
    with registered("probe_double", fill(0.5)):
        assert "probe_double" in _core.effects()
        assert "probe_double" not in _core.effects(declared=True)  # it describes its own parameters
        assert {"grade", "pattern", "pixel_grid"} <= set(_core.effects(declared=True)) <= set(_core.effects())
    assert _core.effects(declared=True) == sorted(_core.effects(declared=True))
    for effect in _core.effects(declared=True):
        _core.params(effect)  # the parameters of each are declared, so none refuses


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


@pytest.mark.parametrize(
    ("function", "said"),
    [
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


# --- inputs: what a step offers an effect, and the part the effect declares ------------------------------------------

PICTURE = np.full((2, 3, 3), 0.5, np.float32)
PIECES = np.array([[0, 0, 1], [1, 2, 2]], np.int32)
DEPTH = np.array([[0.0, 0.25, 0.5], [0.75, 1.0, 0.125]], np.float32)
LIGHT = np.full((2, 3), 0.875, np.float32)


def recording() -> tuple[list[dict[str, Any]], Callable[..., np.ndarray]]:
    """A foreign effect that returns the picture as it is and logs the keyword inputs it received."""
    received: list[dict[str, Any]] = []

    def function(rgb, params, **inputs):
        received.append(
            {name: value.copy() if isinstance(value, np.ndarray) else value for name, value in inputs.items()}
        )
        return rgb

    return received, function


@contextmanager
def registered_with(
    name: str, function: Callable, inputs: list[str] | None = None, fields: dict[str, str] | None = None
) -> Iterator[None]:
    """`function` registered as `name` with the `inputs` and `fields` it declares, for the block."""
    _core.register(name, "utilities", 1.0, function, inputs=inputs or [], fields=fields)
    try:
        yield
    finally:
        _core.unregister(name)


def test_a_declared_partition_arrives_as_the_int32_array_and_an_undeclared_one_is_absent():
    taken, takes = recording()
    ignored, ignores = recording()
    with registered_with("probe_takes", takes, inputs=["partition"]), registered_with("probe_ignores", ignores):
        _core.apply("probe_takes", PICTURE, {}, partition=PIECES)
        _core.apply("probe_ignores", PICTURE, {}, partition=PIECES)
    assert list(taken[0]) == ["partition"]
    assert taken[0]["partition"].dtype == np.int32
    np.testing.assert_array_equal(taken[0]["partition"], PIECES)
    assert ignored == [{}]


def test_a_declared_field_arrives_as_a_float32_array_under_its_parameter_name_and_an_undeclared_one_is_absent():
    taken, takes = recording()
    ignored, ignores = recording()
    # two parameters that hold fields, so a map arriving under the wrong parameter shows
    with (
        registered_with("probe_takes", takes, fields={"near": "depth", "lit": "light"}),
        registered_with("probe_ignores", ignores),
    ):
        # offered under the parameter's name, so the field the step chose is the caller's business
        offered = {"near": DEPTH, "lit": LIGHT, "extra": DEPTH}
        _core.apply("probe_takes", PICTURE, {}, fields=offered)
        _core.apply("probe_ignores", PICTURE, {}, fields=offered)
    assert sorted(taken[0]) == ["lit", "near"]
    assert {name: array.dtype for name, array in taken[0].items()} == {"near": np.float32, "lit": np.float32}
    np.testing.assert_array_equal(taken[0]["near"], DEPTH)
    np.testing.assert_array_equal(taken[0]["lit"], LIGHT)
    assert ignored == [{}]


def test_the_size_ratio_arrives_only_if_declared():
    taken, takes = recording()
    ignored, ignores = recording()
    with registered_with("probe_takes", takes, inputs=["size_ratio"]), registered_with("probe_ignores", ignores):
        _core.apply("probe_takes", PICTURE, {}, size_ratio=2.5)
        _core.apply("probe_ignores", PICTURE, {}, size_ratio=2.5)
    assert taken == [{"size_ratio": 2.5}]
    assert ignored == [{}]


def test_an_effect_says_the_fields_it_declares_with_their_defaults():
    with registered_with("probe_fields", fill(0.5), inputs=["partition"], fields={"field": "depth", "mask": "subject"}):
        assert _core.effect("probe_fields")["fields"] == {"field": "depth", "mask": "subject"}
        assert _core.effect("probe_fields")["inputs"] == ["partition"]


@pytest.mark.parametrize(
    ("declared", "offered", "said"),
    [
        pytest.param({"inputs": ["partition"]}, {}, "partition", id="a partition declared and not offered"),
        pytest.param({"fields": {"field": "depth"}}, {}, "field", id="a field declared and not offered"),
        pytest.param(
            {"inputs": ["partition"]},
            {"partition": np.zeros((3, 2), np.int32)},
            "3 x 2",
            id="a partition of another size than the picture",
        ),
        pytest.param(
            {"fields": {"field": "depth"}},
            {"fields": {"field": np.zeros((1, 3), np.float32)}},
            "1 x 3",
            id="a field of another size than the picture",
        ),
    ],
)
def test_a_declared_input_that_is_missing_or_of_another_size_refuses_the_run_and_the_function_never_runs(
    declared, offered, said
):
    calls, function = recording()
    with registered_with("probe_picky", function, **declared), pytest.raises(ValueError, match=said):
        _core.apply("probe_picky", PICTURE, {}, **offered)
    assert calls == []


@pytest.mark.parametrize(
    ("offered", "said"),
    [
        pytest.param({"size_ratio": 0.0}, "size_ratio", id="a size ratio of 0"),
        pytest.param({"size_ratio": math.nan}, "size_ratio", id="a size ratio that is NaN"),
        pytest.param({"partition": PIECES.astype(np.int64)}, "int32", id="a partition that is not int32"),
        pytest.param({"partition": np.zeros(6, np.int32)}, "shape", id="a partition that is not 2-dimensional"),
        pytest.param({"partition": np.array([[0, -1, 1], [1, 1, 1]], np.int32)}, "-1", id="a negative piece number"),
        pytest.param(
            {"partition": np.array([[0, 1, 2], [3, 4, 6]], np.int32)}, "6", id="a piece number past one for each pixel"
        ),
        pytest.param(
            {"partition": np.array([[0, 1, 2], [3, 4, 2**31 - 1]], np.int32)},
            "2147483647",
            id="the largest piece number",
        ),
        pytest.param({"fields": {"field": DEPTH.astype(np.float64)}}, "float32", id="a field that is not float32"),
        pytest.param({"fields": {"field": DEPTH + 1}}, "from 0 to 1", id="a field above 1"),
        pytest.param({"fields": {"field": np.where(DEPTH > 0.5, np.nan, DEPTH).astype(np.float32)}}, "NaN", id="NaN"),
    ],
)
def test_an_offered_input_that_is_no_valid_input_is_refused_naming_it(offered, said):
    with registered_with("probe_any", fill(0.5)), pytest.raises(ValueError, match=said):
        _core.apply("probe_any", PICTURE, {}, **offered)


def test_a_partition_may_number_every_pixel_its_own_piece():
    taken, takes = recording()
    every_pixel = np.arange(6, dtype=np.int32).reshape(2, 3)
    with registered_with("probe_takes", takes, inputs=["partition"]):
        _core.apply("probe_takes", PICTURE, {}, partition=every_pixel)
    np.testing.assert_array_equal(taken[0]["partition"], every_pixel)


@pytest.mark.parametrize(
    ("declared", "said"),
    [
        pytest.param({"inputs": ["seams"]}, "seams", id="an input that does not exist"),
        pytest.param({"fields": {"partition": "depth"}}, "partition", id="a field parameter named like an input"),
        pytest.param({"fields": {"field": 3}}, "str", id="a default field that is no name"),
    ],
)
def test_a_declaration_that_is_no_inputs_or_fields_is_refused_and_registers_nothing(declared, said):
    with pytest.raises((ValueError, TypeError), match=said):
        _core.register("probe_declared", "utilities", 1.0, fill(0.5), **declared)
    assert "probe_declared" not in _core.effects()


# --- errors: the kind the core reports, and the exception a function raised ------------------------------------------


@pytest.mark.parametrize(
    "raised",
    [
        pytest.param(KeyError("amount"), id="a builtin"),
        pytest.param(RecipeError("the amount is out of range"), id="a bullterrier-image exception"),
        pytest.param(SubjectNotFoundError("no subject in this piece"), id="a subclass with its own base"),
    ],
)
def test_the_exception_a_foreign_effect_raised_comes_back_unchanged(raised):
    def raises(rgb, params):
        raise raised

    with registered("probe_raises", raises), pytest.raises(type(raised)) as caught:
        _core.apply("probe_raises", PIXEL, {})
    assert caught.value is raised


def test_the_exception_of_a_foreign_effect_that_a_foreign_effect_ran_comes_back_unchanged_through_both():
    raised = RecipeError("deep inside")

    def raises(rgb, params):
        raise raised

    def through(rgb, params):
        return _core.apply("probe_inner", rgb, {})

    with registered("probe_inner", raises), registered("probe_outer", through), pytest.raises(RecipeError) as caught:
        _core.apply("probe_outer", PIXEL, {})
    assert caught.value is raised


@pytest.mark.parametrize(
    ("call", "kind"),
    [
        pytest.param(lambda: _core.check("grade", {"exposure": 11}), "recipe", id="a value out of range"),
        pytest.param(lambda: _core.read_recipe('{"steps": [{"opacity": 1}]}', "json"), "recipe", id="a broken recipe"),
        pytest.param(lambda: _core.params("no_such_effect"), "unknown_effect", id="params of no effect"),
        pytest.param(lambda: _core.effect("no_such_effect"), "unknown_effect", id="metadata of no effect"),
        pytest.param(lambda: _core.apply("no_such_effect", PIXEL, {}), "unknown_effect", id="running no effect"),
        pytest.param(lambda: _core.check("no_such_effect", {}), "unknown_effect", id="checking no effect"),
        pytest.param(
            lambda: _core.apply("grade", np.zeros((2, 2), np.float32), {}), "invalid_image", id="a picture of no shape"
        ),
        pytest.param(
            lambda: _core.apply("grade", np.full((1, 1, 3), np.nan, np.float32), GRADE),
            "non_finite_image",
            id="a NaN picture",
        ),
        pytest.param(
            lambda: _core.apply("grade", np.full((1, 1, 3), np.inf, np.float32), GRADE),
            "non_finite_image",
            id="an infinite picture",
        ),
        pytest.param(lambda: _core.apply("grade", PIXEL, {}), "other", id="parameters that are not the effect's"),
        pytest.param(lambda: _core.register("grade", "utilities", 1.0, fill(0.5)), "other", id="a name that is taken"),
    ],
)
def test_an_error_of_the_core_is_a_value_error_that_names_its_kind(call, kind):
    with pytest.raises(_core.CoreError) as caught:
        call()
    assert caught.value.kind == kind
    assert isinstance(caught.value, ValueError)
    assert str(caught.value)


def test_an_error_of_the_core_survives_pickling_with_its_kind():
    """Renders may run in other processes, which send their exceptions back pickled."""
    with pytest.raises(_core.CoreError) as caught:
        _core.effect("no_such_effect")
    again = pickle.loads(pickle.dumps(caught.value))  # noqa: S301  a value made just above, not outside data
    assert (type(again), again.kind, str(again)) == (_core.CoreError, "unknown_effect", str(caught.value))


def test_a_foreign_effect_that_returns_no_picture_is_an_error_of_the_other_kind_naming_it():
    with registered("probe_bad", lambda rgb, params: None), pytest.raises(_core.CoreError, match="probe_bad") as caught:
        _core.apply("probe_bad", PIXEL, {})
    assert caught.value.kind == "other"


def test_the_core_names_the_kinds_of_its_errors_in_the_order_of_the_table():
    assert _core.vocabulary()["error_kinds"] == [
        "recipe",
        "unknown_effect",
        "subject_not_found",
        "depth_not_found",
        "map_not_found",
        "invalid_image",
        "non_finite_image",
        "image_read",
        "image_write",
        "backend_unavailable",
        "lut",
        "other",
    ]


# --- checking parameters and reading recipes ---------------------------------------------------------------------


def test_checking_gives_the_complete_parameters_with_a_whole_number_kept_whole_and_a_decimal_kept_decimal():
    checked = _core.check("pixel_grid", {"cell": 5, "gap": 1})
    assert checked == {"cell": 5, "gap": 1.0, "amount": 0.4, "keep_brightness": False}
    assert [type(checked[key]) for key in ("cell", "gap")] == [int, float]


def test_a_value_the_declaration_refuses_is_a_value_error_naming_the_field_and_the_limit():
    with pytest.raises(ValueError, match=r"exposure.*<= 10"):
        _core.check("grade", {"exposure": 11})


def test_checking_the_parameters_of_an_effect_the_core_does_not_describe_is_refused_by_name():
    with pytest.raises(ValueError, match="no_such_effect"):
        _core.check("no_such_effect", {})
    with registered("probe_double", fill(0.5)), pytest.raises(ValueError, match=r"probe_double.*its own parameters"):
        _core.check("probe_double", {})


def test_a_recipe_is_read_from_json_or_toml_and_any_other_format_is_refused():
    assert _core.read_recipe('{"steps": [{"effect": "grade", "opacity": 0.5}]}', "json") == {
        "steps": [{"effect": "grade", "opacity": 0.5}]
    }
    assert _core.read_recipe("[[step]]\neffect = 'grade'\nexposure = 1\n", "toml") == {
        "steps": [{"effect": "grade", "params": {"exposure": 1}}]
    }
    with pytest.raises(ValueError, match=r"toml.*json"):
        _core.read_recipe("steps: []", "yaml")


def test_a_recipe_that_is_not_one_is_a_value_error_naming_the_key():
    with pytest.raises(ValueError, match="opacity"):
        _core.read_recipe('{"steps": [{"effect": "grade", "opacity": "half"}]}', "json")


@pytest.mark.parametrize(
    ("text", "named"),
    [
        pytest.param('{"steps": [{"effect": "grade", "effect": "pattern"}]}', "effect", id="a key written twice"),
        pytest.param(
            '{"steps": [{"effect": "grade", "params": {"a": 1, "a": 2}}]}', "a", id="a parameter written twice"
        ),
        pytest.param('{"steps": [], "regions": {"depth": null}}', "depth", id="a region that is null"),
    ],
)
def test_a_recipe_in_json_with_a_repeated_key_or_a_region_that_is_no_table_is_a_value_error(text, named):
    with pytest.raises(ValueError, match=named):
        _core.read_recipe(text, "json")


SMALLEST_STACK = 256  # KB; the least a thread needs to read and check the deepest recipe the core accepts


def on_the_smallest_stack(work: Callable[[], Any]) -> Any:
    """What `work` returns (or raises) in a thread with `SMALLEST_STACK`. A stack overflow in the core kills the
    process, which is how a test here fails."""
    outcome: dict[str, Any] = {}

    def run() -> None:
        try:
            outcome["value"] = work()
        except BaseException as error:  # noqa: BLE001  handed to the caller below
            outcome["error"] = error

    previous = threading.stack_size(SMALLEST_STACK * 1024)
    try:
        thread = threading.Thread(target=run)
        thread.start()
        thread.join()
    finally:
        threading.stack_size(previous)
    if "error" in outcome:
        raise outcome["error"]
    return outcome["value"]


NESTED_JSON = 123  # the deepest a step's parameter can nest in a JSON recipe: serde_json allows 128 levels in all
NESTED_TOML = 80  # the deepest the toml crate reads, in arrays or in inline tables


def nested_in_lists(levels: int) -> Any:
    value: Any = 1
    for _ in range(levels):
        value = [value]
    return value


@pytest.mark.parametrize(
    ("text", "syntax"),
    [
        pytest.param(
            '{"steps": [{"effect": "grade", "params": {"exposure": '
            + "[" * NESTED_JSON
            + "1"
            + "]" * NESTED_JSON
            + "}}]}",
            "json",
            id="arrays as deep as JSON allows",
        ),
        pytest.param(
            "format = 3\n[[step]]\neffect = 'grade'\nexposure = "
            + "{a=" * NESTED_TOML
            + "1"
            + "}" * NESTED_TOML
            + "\n",
            "toml",
            id="inline tables as deep as TOML allows",
        ),
    ],
)
def test_a_recipe_nested_as_deep_as_its_syntax_allows_is_read_and_checked_on_a_small_stack(text, syntax):
    def work():
        recipe = _core.read_recipe(text, syntax)
        with pytest.raises(ValueError, match="exposure"):
            _core.check_recipe(recipe)
        return recipe

    assert "exposure" in on_the_smallest_stack(work)["steps"][0]["params"]


@pytest.mark.parametrize(
    ("levels", "named"),
    [
        pytest.param(NESTED_JSON, "exposure", id="as deep as JSON allows: the core refuses the value, not the depth"),
        pytest.param(100_000, "nested", id="deeper than JSON allows: refused before anything walks it"),
    ],
)
def test_a_recipe_dict_is_walked_to_the_depth_json_allows_and_no_further(levels, named):
    recipe = {"steps": [{"effect": "grade", "params": {"exposure": nested_in_lists(levels)}}]}
    with pytest.raises(ValueError, match=named):
        on_the_smallest_stack(lambda: _core.check_recipe(recipe))
