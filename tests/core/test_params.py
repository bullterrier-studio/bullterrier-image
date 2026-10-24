"""The parameter-model pattern every effect follows: `EffectParams`, `param`, `Color`, and the wording of errors."""

import inspect
import re
from typing import Literal

import numpy as np
import pytest
from pydantic import ValidationError

from photo_fx.core.params import Color, EffectParams, describe_error, param


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


@pytest.mark.parametrize(
    ("values", "field", "expected"),
    [({"radius": 0}, "radius", 0.0), ({"radius": 500.0}, "radius", 500.0), ({"count": 9}, "count", 9)],
    ids=["min", "max", "int max"],
)
def test_values_inside_the_range_are_accepted_including_the_bounds(values, field, expected):
    assert getattr(Probe(**values), field) == expected


def test_a_whole_number_is_accepted_for_a_decimal_field():
    probe = Probe(radius=7)
    assert probe.radius == 7.0
    assert isinstance(probe.radius, float)


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


@pytest.mark.parametrize("text", ["  neon  ", " .:", "\tx\n", " "])
def test_text_is_taken_exactly_as_given_spaces_included(text):
    # a space can be data (the lightest character of a text-art palette), so nothing is trimmed
    assert Probe(label=text).label == text
    assert Probe.parse({"label": text}).label == text


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
        (tuple[str, ...], lambda: param(("a",), min=0, max=1, doc="d"), "min"),
        (bool, lambda: param(False, unit="count", doc="d"), "unit"),
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


def _entry(text: str, name: str) -> str:
    """The documentation entry of field `name`: its heading line and the indented lines under it."""
    lines = text.splitlines()
    start = next(i for i, line in enumerate(lines) if line.startswith(f"{name} : "))
    end = next((i for i in range(start + 1, len(lines)) if not lines[i].startswith(" ")), len(lines))
    return " ".join(" ".join(lines[start:end]).split())


def test_the_documentation_has_each_fields_type_unit_range_default_and_description():
    text = Probe.documentation()
    assert text.startswith("Parameters")
    radius = _entry(text, "radius")
    for part in ("number", "px", "0 to 500", "24.0", "Size of the probe's halo. Larger = wider."):
        assert part in radius, part
    count = _entry(text, "count")
    for part in ("whole number", "count", "1 to 9", "3", "How many probes."):
        assert part in count, part
    assert "'soft', 'hard'" in _entry(text, "mode")
    assert "colour" in _entry(text, "tint")
    assert "#ff8000" in _entry(text, "tint")


def test_the_documentation_says_an_animated_whole_number_moves_in_whole_steps():
    assert "whole steps" in _entry(Probe.documentation(), "count")
    assert "whole steps" not in _entry(Probe.documentation(), "radius")


# --- strictness for every kind of field (known-good and known-bad per kind) -----------------------------------------


class Kinds(EffectParams):
    """One field of each kind an effect may declare."""

    invert: bool = param(False, doc="Swap dark and light.")
    count: int = param(3, unit="count", min=1, max=9, doc="How many.")
    radius: float = param(2.0, unit="px", min=0, max=10, doc="How far.")
    mode: Literal["soft", "hard"] = param("soft", doc="Edge: 'soft' or 'hard'.")
    tint: Color = param("#000000", doc="A colour.")
    names: tuple[str, ...] = param(("a",), doc="Labels.")


@pytest.mark.parametrize(
    ("values", "field", "expected"),
    [
        ({"invert": True}, "invert", True),
        ({"count": 9}, "count", 9),
        ({"radius": 3}, "radius", 3.0),
        ({"radius": np.float32(2.5)}, "radius", 2.5),
        ({"mode": "hard"}, "mode", "hard"),
        ({"tint": "#fff"}, "tint", (1.0, 1.0, 1.0)),
        ({"tint": np.array([0.25, 0.5, 1.0])}, "tint", (0.25, 0.5, 1.0)),
        ({"tint": (np.float32(0.5), 0, 1)}, "tint", (0.5, 0.0, 1.0)),
        ({"names": ["x", "y"]}, "names", ("x", "y")),
    ],
)
def test_every_kind_of_field_accepts_its_own_kind_of_value(values, field, expected):
    assert getattr(Kinds(**values), field) == expected  # every expected value is exact in float32 and float64


@pytest.mark.parametrize(
    ("values", "field"),
    [
        ({"invert": "yes"}, "invert"),
        ({"invert": 1}, "invert"),
        ({"count": True}, "count"),
        ({"radius": "3"}, "radius"),
        ({"mode": 1}, "mode"),
        ({"tint": np.array([0.1, 0.2, 0.3, 0.4])}, "tint"),
        ({"tint": [0.5, 0.5, True]}, "tint"),
        ({"names": [1, 2]}, "names"),
    ],
)
def test_every_kind_of_field_refuses_values_of_another_kind(values, field):
    with pytest.raises(ValidationError, match=field):
        Kinds(**values)


def test_whole_number_fields_are_the_int_fields():
    assert Kinds.whole_number_fields() == frozenset({"count"})
    assert Probe.whole_number_fields() == frozenset({"count"})


def test_the_published_colour_pattern_matches_a_whole_value_only():
    pattern = Probe.json_schema()["properties"]["tint"]["anyOf"][0]["pattern"]
    assert re.search(pattern, "#fff")
    assert re.search(pattern, "#a1B2c3")
    assert not re.search(pattern, "xx#fff00")  # JSON Schema patterns search, so they must be anchored
    assert not re.search(pattern, "#fff0")


# --- parse and field_names: the one way callers validate ------------------------------------------------------------


def test_parse_builds_the_model_from_a_mapping():
    assert Probe.parse({"radius": 3, "mode": "hard"}, "effect 'probe'") == Probe(radius=3.0, mode="hard")


def test_parse_reports_every_problem_as_one_value_error_naming_the_owner():
    with pytest.raises(ValueError, match=r"probe.*`radius`.*\b0\b.*-1.*`raduis`") as raised:
        Probe.parse({"radius": -1, "raduis": 2}, "effect 'probe'")
    assert not isinstance(raised.value, ValidationError)  # callers see a plain ValueError, never pydantic's type


def test_field_names_are_the_fields_in_declaration_order():
    assert Probe.field_names() == ("radius", "count", "mode", "tint", "label")


@pytest.mark.parametrize("value", [float("nan"), float("inf")], ids=["nan", "infinity"])
def test_a_nan_or_infinity_is_reported_as_not_a_finite_number_not_as_out_of_range(value):
    with pytest.raises(ValueError, match=r"`radius`.*finite") as raised:
        Probe.parse({"radius": value}, "effect 'probe'")
    assert "<=" not in str(raised.value)


def test_field_docs_give_the_parts_separately():
    (radius, *_) = Probe.fields_doc()
    assert (radius.name, radius.unit, radius.minimum, radius.maximum, radius.default) == ("radius", "px", 0, 500, 24.0)
    assert radius.doc == "Size of the probe's halo. Larger = wider."


# --- error wording ---------------------------------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("values", "parts"),
    [
        ({"radius": -3}, ["effect 'probe'", "`radius`", ">= 0", "-3"]),  # the owner, field, constraint and value
        ({"raduis": 3}, ["`raduis`", "count, label, mode, radius, tint"]),  # an unknown field lists the valid ones
        ({"count": "3"}, ["`count`", "whole number", "'3'"]),  # the kind wanted and the value given
        ({"radius": -3, "count": 99}, ["`radius`", "`count`"]),  # every problem, not just the first
    ],
    ids=["out of range", "unknown field", "wrong kind", "two problems"],
)
def test_an_error_message_names_what_is_wrong(values, parts):
    with pytest.raises(ValidationError) as raised:
        Probe(**values)
    message = describe_error(raised.value, "effect 'probe'", Probe.model_fields)
    for part in parts:
        assert part in message
