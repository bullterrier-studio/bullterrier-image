"""The parameter-model pattern every effect follows: `EffectParams`, `param`, `Color`, and the wording of errors."""

import inspect
from typing import Literal

import numpy as np
import pytest
from pydantic import ValidationError

from photo_fx.core.params import UNITS, Color, EffectParams, describe_error, param


class Probe(EffectParams):
    """A known-good model: one field of every kind."""

    radius: float = param(24.0, unit="px", min=0, max=500, doc="Size of the probe's halo. Larger = wider.")
    count: int = param(3, unit="count", min=1, max=9, doc="How many probes.")
    mode: Literal["soft", "hard"] = param("soft", doc="Edge: 'soft' blends, 'hard' cuts.")
    tint: Color = param("#ff8000", doc="Colour of the probe.")
    label: str = param("probe", doc="A name shown in labels.")


# --- known-good ----------------------------------------------------------------------------------------------------


def test_a_well_declared_model_builds_with_its_defaults():
    probe = Probe()
    assert (probe.radius, probe.count, probe.mode, probe.label) == (24.0, 3, "soft", "probe")


def test_values_inside_the_range_are_accepted_including_the_bounds():
    assert Probe(radius=0).radius == 0.0
    assert Probe(radius=500.0).radius == 500.0
    assert Probe(count=9).count == 9


def test_a_whole_number_is_accepted_for_a_decimal_field():
    probe = Probe(radius=7)
    assert probe.radius == 7.0
    assert isinstance(probe.radius, float)


def test_units_are_the_agreed_list():
    assert UNITS == ("px", "fraction", "degrees", "stops", "seconds", "count", "ratio", "cycles")


# --- values are validated --------------------------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("values", "field"),
    [
        ({"radius": True}, "radius"),  # a bool is not a number here, although Python says it is an int
        ({"radius": "3"}, "radius"),  # a numeric-looking string is not a number either
        ({"count": False}, "count"),
        ({"count": "3"}, "count"),
        ({"count": 2.5}, "count"),
        ({"radius": float("nan")}, "radius"),
        ({"radius": float("inf")}, "radius"),
        ({"radius": -0.5}, "radius"),
        ({"radius": 500.5}, "radius"),
        ({"count": 0}, "count"),
        ({"mode": "fuzzy"}, "mode"),
    ],
)
def test_wrong_kinds_and_out_of_range_values_are_rejected(values, field):
    with pytest.raises(ValidationError, match=field):
        Probe(**values)


def test_an_unknown_field_is_rejected():
    with pytest.raises(ValidationError, match="raduis"):
        Probe(raduis=3)  # type: ignore[call-arg]  # the point of the test: a misspelt field


def test_a_model_is_frozen():
    probe = Probe()
    with pytest.raises(ValidationError, match="frozen"):
        probe.radius = 3.0  # type: ignore[misc]  # the point of the test: models are frozen


def test_text_is_stripped_of_surrounding_whitespace():
    assert Probe(label="  neon  ").label == "neon"


# --- Color -----------------------------------------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("written", "expected"),
    [
        ("#ff8000", (1.0, 128 / 255, 0.0)),
        ("#f80", (1.0, 136 / 255, 0.0)),  # #rgb doubles each digit
        ("  #FF8000 ", (1.0, 128 / 255, 0.0)),
        ([0.25, 0.5, 1], (0.25, 0.5, 1.0)),
        ((0, 0, 0), (0.0, 0.0, 0.0)),
    ],
)
def test_a_colour_may_be_written_as_hex_or_three_numbers(written, expected):
    tint = Probe(tint=written).tint
    np.testing.assert_allclose(tint, expected, rtol=1e-7)  # float32 precision, as effects compute in float32
    assert len(tint) == 3


def test_a_colour_holds_exactly_the_float32_values_effects_compute_with():
    tint = Probe(tint="#3a1bff").tint
    np.testing.assert_array_equal(np.asarray(tint, np.float32), np.array([58, 27, 255], np.float32) / np.float32(255))


@pytest.mark.parametrize(
    "written",
    ["#zzzzzz", "ff8000", "#ff80", "", [1, 0], [2, 0, 0], [-0.1, 0, 0], [True, 0, 0], ["1", 0, 0], 7, None],
)
def test_a_malformed_colour_is_rejected_naming_the_field(written):
    with pytest.raises(ValidationError, match="tint"):
        Probe(tint=written)


# --- the rules of a declaration, checked when the class is made ------------------------------------------------------


def _declare(annotation, field):
    """Make a model with one field `x: annotation = field`, as an effect module would at import."""
    return type("Broken", (EffectParams,), {"__annotations__": {"x": annotation}, "x": field, "__doc__": "Broken."})


@pytest.mark.parametrize(
    ("annotation", "make", "complaint"),
    [
        (float, lambda: param(1.0, unit="px", min=0, max=2, doc=""), "doc"),
        (float, lambda: param(1.0, unit="px", min=0, max=2, doc="   "), "doc"),
        (float, lambda: param(1.0, min=0, max=2, doc="d"), "unit"),
        (float, lambda: param(1.0, unit="pixels", min=0, max=2, doc="d"), "pixels"),
        (float, lambda: param(1.0, unit="px", max=2, doc="d"), "min"),
        (int, lambda: param(1, unit="count", min=0, doc="d"), "max"),
        (float, lambda: param(1.0, unit="px", min=3, max=2, doc="d"), "above max"),
        (Color, lambda: param("not a colour", doc="d"), "default"),
        (float, lambda: param(5.0, unit="px", min=0, max=2, doc="d"), "default"),
        (str, lambda: param("a", min=0, max=2, doc="d"), "min"),
        (str, lambda: param("a", unit="px", doc="d"), "unit"),
        (Literal["soft", "hard"], lambda: param("soft", doc="Edge: 'soft' blends."), "hard"),
        (float, lambda: 1.0, "param"),  # a plain default: no description, no range
    ],
)
def test_a_badly_declared_field_fails_when_the_class_is_made_naming_model_and_field(annotation, make, complaint):
    with pytest.raises(ValueError, match="Broken") as raised:
        _declare(annotation, make())
    message = str(raised.value)
    assert "`x`" in message
    assert complaint in message


def test_a_field_without_any_default_fails_when_the_class_is_made():
    with pytest.raises(ValueError, match=r"Broken.*`x`"):
        type("Broken", (EffectParams,), {"__annotations__": {"x": float}, "__doc__": "Broken."})


def test_a_seed_is_a_count_from_zero_to_the_largest_32_bit_number():
    class Seeded(EffectParams):
        """Seeded."""

        seed: int = param(0, unit="count", min=0, max=2**32 - 1, doc="Picks the random pattern.")

    assert Seeded(seed=2**32 - 1).seed == 2**32 - 1
    with pytest.raises(ValidationError, match="seed"):
        Seeded(seed=2**32)


# --- documentation generated from the model --------------------------------------------------------------------------


def test_the_json_schema_carries_every_description_and_unit():
    properties = Probe.model_json_schema()["properties"]
    assert properties["radius"]["description"] == "Size of the probe's halo. Larger = wider."
    assert properties["radius"]["unit"] == "px"
    assert (properties["radius"]["minimum"], properties["radius"]["maximum"]) == (0, 500)
    assert properties["mode"]["unit"] is None
    assert set(properties) == {"radius", "count", "mode", "tint", "label"}
    assert all(p["description"] for p in properties.values())


def test_the_signature_lists_every_field_in_order_with_its_default_and_a_readable_type():
    signature = Probe.signature()
    assert list(signature.parameters) == ["radius", "count", "mode", "tint", "label"]
    assert [p.default for p in signature.parameters.values()] == [24.0, 3, "soft", "#ff8000", "probe"]
    assert all(p.kind is inspect.Parameter.POSITIONAL_OR_KEYWORD for p in signature.parameters.values())
    text = str(signature)
    assert "radius: float = 24.0" in text
    assert "mode: Literal['soft', 'hard']" in text
    assert "tint: str | list[float] | tuple[float, float, float] = '#ff8000'" in text  # what a caller writes
    assert "pydantic" not in text
    assert "Annotated" not in text


def test_the_documentation_has_each_fields_type_unit_range_default_and_description():
    text = Probe.documentation()
    assert text.startswith("Parameters\n----------\n")
    assert "radius : number, px, 0 to 500, default 24.0\n    Size of the probe's halo. Larger = wider." in text
    assert "count : whole number, count, 1 to 9, default 3\n" in text
    assert "mode : one of 'soft', 'hard', default 'soft'\n    Edge: 'soft' blends, 'hard' cuts." in text
    assert "tint : colour, default '#ff8000'\n" in text


def test_field_docs_give_the_parts_separately():
    (radius, *_) = Probe.fields_doc()
    assert (radius.name, radius.unit, radius.minimum, radius.maximum, radius.default) == ("radius", "px", 0, 500, 24.0)
    assert radius.doc == "Size of the probe's halo. Larger = wider."


# --- error wording ---------------------------------------------------------------------------------------------------


def _error(**values) -> ValidationError:
    with pytest.raises(ValidationError) as raised:
        Probe(**values)
    return raised.value


def test_an_error_names_the_owner_the_field_the_constraint_and_the_value():
    message = describe_error(_error(radius=-3), "effect 'probe'", Probe.model_fields)
    assert "effect 'probe'" in message
    assert "`radius`" in message
    assert ">= 0" in message
    assert "-3" in message


def test_an_unknown_field_error_lists_the_valid_ones():
    message = describe_error(_error(raduis=3), "effect 'probe'", Probe.model_fields)
    assert "`raduis`" in message
    assert "count, label, mode, radius, tint" in message


def test_a_wrong_kind_error_says_what_kind_is_wanted_and_what_was_given():
    message = describe_error(_error(count="3"), "effect 'probe'", Probe.model_fields)
    assert "`count`" in message
    assert "whole number" in message
    assert "'3'" in message


def test_every_problem_is_reported_not_just_the_first():
    message = describe_error(_error(radius=-3, count=99), "effect 'probe'", Probe.model_fields)
    assert "`radius`" in message
    assert "`count`" in message
