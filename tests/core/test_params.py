"""The parameter-model pattern every effect follows: `EffectParams`, `param`, `Color`, and the wording of errors."""

import inspect
import re
from typing import Literal

import numpy as np
import pytest
from pydantic import ValidationError

from bullterrier_image.core.params import Color, Drive, EffectParams, Picture, describe_error, param
from bullterrier_image.errors import RecipeError


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


# --- Picture ---------------------------------------------------------------------------------------------------------


class Textured(EffectParams):
    """A model with a picture field."""

    image: Picture = param("#808080", doc="A picture laid over the photo.")


def noise_picture(shape=(4, 6, 4), seed=0, label="noise.png") -> Picture:
    return Picture(np.random.default_rng(seed).random(shape, dtype=np.float32), label)


def test_a_picture_field_has_a_colour_default_that_is_a_flat_opaque_picture():
    image = Textured().image
    assert (image.width, image.height) == (1, 1)
    np.testing.assert_allclose(image.pixels[0, 0], [128 / 255, 128 / 255, 128 / 255, 1.0], atol=1e-7)


@pytest.mark.parametrize(
    ("written", "rgb"),
    [("#ff8000", (1.0, 128 / 255, 0.0)), ("#f80", (1.0, 136 / 255, 0.0)), ((0.25, 0.5, 1.0), (0.25, 0.5, 1.0))],
)
def test_a_colour_is_a_flat_picture(written, rgb):
    np.testing.assert_allclose(Textured(image=written).image.pixels[0, 0], [*rgb, 1.0], atol=1e-7)


def test_a_picture_is_kept_as_it_is_not_copied_when_validated_again():
    picture = noise_picture()
    assert Textured(image=picture).image is picture
    assert Textured(image=Textured(image=picture).image).image is picture


@pytest.mark.parametrize(
    "written",
    ["tex.png", "#zzzzzz", [1, 0], 7, None, np.zeros((5, 5, 3), np.float32), np.zeros((5, 5, 3)).tolist()],
    ids=["a path", "bad hex", "two numbers", "a number", "none", "an array", "nested lists"],
)
def test_anything_but_a_picture_or_a_colour_is_rejected_naming_the_field_and_never_printing_pixels(written):
    with pytest.raises(RecipeError, match="`image`") as raised:
        Textured.parse({"image": written})
    assert len(str(raised.value)) < 300


@pytest.mark.parametrize(
    "array",
    [
        pytest.param(np.zeros((4, 6, 3), np.float32), id="rgb"),
        pytest.param(np.zeros((4, 6, 4), np.float64), id="rgba, double precision"),
        pytest.param(np.zeros((1, 1, 3), np.float32), id="one pixel"),
    ],
)
def test_a_picture_is_made_from_a_float_array_with_three_or_four_channels(array):
    picture = Picture(array)
    assert picture.pixels.shape == (*array.shape[:2], 4)
    assert picture.pixels.dtype == np.float32


@pytest.mark.parametrize(
    "array",
    [
        pytest.param(np.zeros((4, 6, 3), np.uint8), id="integers"),
        pytest.param(np.zeros((4, 6), np.float32), id="no channels"),
        pytest.param(np.zeros((4, 6, 2), np.float32), id="two channels"),
        pytest.param(np.zeros((0, 6, 3), np.float32), id="no rows"),
        pytest.param(np.full((4, 6, 3), 1.5, np.float32), id="above one"),
        pytest.param(np.full((4, 6, 3), np.nan, np.float32), id="nan"),
    ],
)
def test_an_array_that_is_not_a_picture_is_refused(array):
    with pytest.raises(RecipeError, match="picture"):
        Picture(array)


def test_a_picture_is_straight_alpha_and_opaque_when_it_has_none():
    rgba = np.array([[[0.2, 0.4, 0.6, 0.25]]], np.float32)
    np.testing.assert_array_equal(Picture(rgba).pixels, rgba)
    np.testing.assert_array_equal(Picture(rgba[..., :3]).pixels, np.array([[[0.2, 0.4, 0.6, 1.0]]], np.float32))


def test_a_picture_never_changes_not_through_its_pixels_nor_through_the_array_it_was_made_from():
    source = np.full((2, 2, 3), 0.5, np.float32)
    picture = Picture(source)
    source[:] = 0.0
    assert picture.pixels.min() == 0.5
    with pytest.raises(ValueError, match="read-only"):
        picture.pixels[0, 0, 0] = 1.0


def test_pictures_are_equal_when_their_pixels_are_whatever_their_labels_and_hash_alike():
    a, same, other = noise_picture(label="a.png"), noise_picture(label="b.png"), noise_picture(seed=1)
    assert a == same
    assert hash(a) == hash(same)
    assert a != other
    assert a != Picture(a.pixels[:2])  # the same first pixels, another size
    assert a != "a.png"
    flat = np.linspace(0, 1, 24, dtype=np.float32)
    assert Picture(flat.reshape(2, 3, 4)) != Picture(flat.reshape(3, 2, 4))  # the same values in another shape


def test_a_pictures_repr_names_the_label_and_the_size_never_the_pixels():
    assert repr(noise_picture((640, 480, 3), label="tex.png")) == "Picture('tex.png', 480x640)"


def test_the_json_schema_of_a_picture_is_a_path_or_a_colour():
    schema = Textured.model_json_schema()["properties"]["image"]
    assert {option["type"] for option in schema["anyOf"]} == {"string", "array"}
    assert schema["default"] == "#808080"


def test_a_picture_is_documented_as_a_picture_and_called_with_the_forms_a_caller_writes():
    (doc,) = Textured.fields_doc()
    assert (doc.type, doc.unit, doc.range, doc.default) == ("picture", None, "", "#808080")
    text = str(Textured.signature())
    assert "Picture" in text
    assert "PathLike" in text
    assert "pydantic" not in text


@pytest.mark.parametrize(
    "make",
    [
        pytest.param(lambda: param("#808080", unit="px", doc="d"), id="a unit"),
        pytest.param(lambda: param("#808080", min=0, max=1, doc="d"), id="a range"),
        pytest.param(lambda: param("not a colour", doc="d"), id="a default that is no colour"),
    ],
)
def test_a_picture_field_takes_no_unit_or_range_and_needs_a_default_colour(make):
    with pytest.raises(ValueError, match="Broken"):
        _declare(Picture, make())


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


class Tiered(EffectParams):
    """A model whose advanced fields sit between its basic ones."""

    amount: float = param(0.5, unit="fraction", min=0, max=1, doc="How much of the look is applied.")
    edge: Literal["reflect", "wrap"] = param("reflect", advanced=True, doc="Past the border: 'reflect' or 'wrap'.")
    angle: float = param(30.0, unit="degrees", min=0, max=180, doc="Direction of the look.")
    softness: float = param(2.0, unit="px", min=0, max=10, advanced=True, doc="Blur of the technique's mask.")


def test_the_documentation_lists_basic_fields_then_an_advanced_section_in_declaration_order():
    text = Tiered.documentation()
    basic, advanced = text.split("Advanced parameters\n-------------------\n")
    assert basic.startswith("Parameters\n----------\n")
    assert [name for name in ("amount", "angle") if f"{name} : " in basic] == ["amount", "angle"]
    assert basic.index("amount : ") < basic.index("angle : ")
    assert advanced.index("edge : ") < advanced.index("softness : ")
    assert "edge : " not in basic
    assert "softness : " not in basic
    assert "amount : " not in advanced
    assert "advanced)" not in text  # the full text has no pointer to itself


def test_the_short_documentation_hides_the_advanced_fields_and_counts_them():
    text = Tiered.documentation(advanced=False)
    assert "amount : " in text
    assert "angle : " in text
    assert "edge" not in text
    assert "softness" not in text
    assert text.rstrip().endswith("(+2 advanced)")


def test_a_model_without_advanced_fields_has_no_advanced_section_and_no_count():
    for text in (Probe.documentation(), Probe.documentation(advanced=False)):
        assert "dvanced" not in text


def test_the_json_schema_says_which_fields_are_advanced():
    properties = Tiered.model_json_schema()["properties"]
    assert {name: p["advanced"] for name, p in properties.items()} == {
        "amount": False,
        "edge": True,
        "angle": False,
        "softness": True,
    }


def test_the_flag_does_not_reorder_the_signature_or_the_fields():
    assert list(Tiered.signature().parameters) == ["amount", "edge", "angle", "softness"]
    assert list(Tiered.model_fields) == ["amount", "edge", "angle", "softness"]


@pytest.mark.parametrize(
    ("values", "refused"),
    [
        pytest.param({"edge": "wrap", "softness": 10}, None, id="advanced-fields-accept-valid-values"),
        pytest.param({"softness": 10.5}, "softness", id="advanced-number-out-of-range-is-refused"),
        pytest.param({"edge": "clamp"}, "edge", id="advanced-choice-not-offered-is-refused"),
    ],
)
def test_an_advanced_field_is_validated_like_any_other(values, refused):
    if refused is None:
        assert Tiered(**values).edge == "wrap"
    else:
        with pytest.raises(ValidationError, match=refused):
            Tiered(**values)


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
    pattern = Probe.model_json_schema()["properties"]["tint"]["anyOf"][0]["pattern"]
    assert re.search(pattern, "#fff")
    assert re.search(pattern, "#a1B2c3")
    assert not re.search(pattern, "xx#fff00")  # JSON Schema patterns search, so they must be anchored
    assert not re.search(pattern, "#fff0")


# --- parse: the one way callers validate ------------------------------------------------------------


def test_parse_builds_the_model_from_a_mapping():
    assert Probe.parse({"radius": 3, "mode": "hard"}, "effect 'probe'") == Probe(radius=3.0, mode="hard")


def test_parse_reports_every_problem_as_one_value_error_naming_the_owner():
    with pytest.raises(ValueError, match=r"probe.*`radius`.*\b0\b.*-1.*`raduis`") as raised:
        Probe.parse({"radius": -1, "raduis": 2}, "effect 'probe'")
    assert not isinstance(raised.value, ValidationError)  # callers see a plain ValueError, never pydantic's type


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


# --- drivable fields: a parameter a field may vary pixel by pixel ----------------------------------------------------


class Drivable(EffectParams):
    """A model with one field of each kind that matters to driving."""

    radius: float = param(24.0, unit="px", min=0, max=500, drivable=True, doc="Size of the halo. Larger = wider.")
    amount: float = param(0.5, unit="fraction", min=0, max=1, doc="How much of it. More = stronger.")
    count: int = param(3, unit="count", min=1, max=9, doc="How many halos.")


def test_a_model_names_its_drivable_fields():
    assert Drivable.drivable_fields() == frozenset({"radius"})
    assert Probe.drivable_fields() == frozenset()


def test_a_drivable_field_is_written_float_or_drive_in_the_signature_and_the_others_as_they_were():
    text = str(Drivable.signature())
    assert "radius: float | bullterrier_image.core.params.Drive = 24.0" in text
    assert "amount: float = 0.5" in text
    assert "count: int = 3" in text


def test_a_drivable_field_says_so_in_its_documentation_and_the_others_do_not():
    text = Drivable.documentation()
    assert re.search(r"radius : number \(drivable\), px, 0 to 500, default 24\.0", text)
    assert "amount : number, fraction" in text
    assert "drivable" not in text.split("amount :")[1]
    assert [doc.type for doc in Drivable.fields_doc()] == ["number (drivable)", "number", "whole number"]


def test_the_json_schema_marks_a_drivable_field():
    properties = Drivable.model_json_schema()["properties"]
    assert properties["radius"]["drivable"] is True
    assert properties["amount"]["drivable"] is False


@pytest.mark.parametrize(
    ("annotation", "make", "complaint"),
    [
        (int, lambda: param(3, unit="count", min=1, max=9, drivable=True, doc="d"), "whole number"),
        (str, lambda: param("a", drivable=True, doc="d"), "number"),
        (bool, lambda: param(False, drivable=True, doc="d"), "number"),
        (Color, lambda: param("#ff0000", drivable=True, doc="d"), "number"),
        (Literal["a", "b"], lambda: param("a", drivable=True, doc="a, b"), "number"),
    ],
)
def test_only_a_decimal_number_can_be_declared_drivable_naming_model_and_field(annotation, make, complaint):
    with pytest.raises(ValueError, match="Broken") as raised:
        _declare(annotation, make())
    assert "`x`" in str(raised.value)
    assert "drivable" in str(raised.value)
    assert complaint in str(raised.value)


DRIVES = {
    "all four keys": ({"by": "near", "start": 0.0, "end": 30.0, "levels": 8}, None),
    "levels left out": ({"by": "near", "start": 1, "end": 2}, None),
    "a field running backwards": ({"by": "light", "start": 30, "end": 0}, None),
    "levels 2": ({"by": "near", "start": 0, "end": 1, "levels": 2}, None),
    "levels 16": ({"by": "near", "start": 0, "end": 1, "levels": 16}, None),
    "levels 1": ({"by": "near", "start": 0, "end": 1, "levels": 1}, "levels"),
    "levels 17": ({"by": "near", "start": 0, "end": 1, "levels": 17}, "levels"),
    "levels a share": ({"by": "near", "start": 0, "end": 1, "levels": 2.5}, "levels"),
    "no field named": ({"by": "", "start": 0, "end": 1}, "by"),
    "a field that is no text": ({"by": 3, "start": 0, "end": 1}, "by"),
    "a start that is not a number": ({"by": "near", "start": "0", "end": 1}, "start"),
    "an end that is not finite": ({"by": "near", "start": 0, "end": float("inf")}, "end"),
    "a nan": ({"by": "near", "start": float("nan"), "end": 1}, "start"),
    "a misspelt key": ({"by": "near", "start": 0, "end": 1, "step": 3}, "step"),
    "no end": ({"by": "near", "start": 0}, "end"),
    "no field": ({"start": 0, "end": 1}, "by"),
}


@pytest.mark.parametrize(("values", "complaint"), DRIVES.values(), ids=DRIVES.keys())
def test_a_drive_names_its_field_its_two_ends_and_how_many_levels_or_is_refused_naming_the_key(values, complaint):
    if complaint is None:
        assert Drive(**values).by == values["by"]
        return
    with pytest.raises(RecipeError, match=rf"^drive: .*{complaint}"):
        Drive(**values)


@pytest.mark.parametrize(
    ("start", "end", "levels", "expected"),
    [
        pytest.param(0, 30, 2, [0, 30], id="two levels are the two ends"),
        pytest.param(0, 30, 4, [0, 10, 20, 30], id="evenly spaced between"),
        pytest.param(30, 0, 4, [30, 20, 10, 0], id="running backwards"),
        pytest.param(-1, 1, 5, [-1, -0.5, 0, 0.5, 1], id="across zero"),
        pytest.param(2, 2, 3, [2, 2, 2], id="a start equal to the end"),
    ],
)
def test_a_drive_has_its_levels_evenly_spaced_from_start_to_end(start, end, levels, expected):
    drive = Drive(by="f", start=start, end=end, levels=levels)
    assert drive.values() == pytest.approx(expected)
    assert [drive.value(level) for level in range(levels)] == pytest.approx(expected)


def test_a_drive_is_frozen_and_equal_to_another_of_the_same_values_and_says_what_it_is():
    drive = Drive(by="near", start=0, end=30)
    assert drive == Drive(by="near", start=0.0, end=30.0, levels=5)
    assert drive != Drive(by="near", start=0, end=31)
    with pytest.raises(ValueError, match="frozen"):
        drive.start = 5  # type: ignore[misc]  # the point of the test
    assert drive.describe() == "by near 0..30"
