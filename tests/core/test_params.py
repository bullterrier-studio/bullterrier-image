"""The parameter-model pattern every effect follows: `EffectParams`, `param`, `Color`, and the wording of errors."""

import copy
from dataclasses import FrozenInstanceError
from typing import Literal

import numpy as np
import pytest

from bullterrier_image.core import parameters
from bullterrier_image.core.params import EffectParams, param, table_of
from bullterrier_image.core.pieces import Drive
from bullterrier_image.core.values import Color, FieldName, Lut, Picture
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
    with pytest.raises(ValueError, match=field):
        Probe(**values)


def test_an_unknown_field_is_rejected():
    with pytest.raises(ValueError, match="raduis"):
        Probe(raduis=3)  # the point of the test: a misspelt field


def test_a_model_is_frozen():
    probe = Probe()
    with pytest.raises(FrozenInstanceError):
        probe.radius = 3.0  # the point of the test: models are frozen


@pytest.mark.parametrize("text", ["  neon  ", " .:", "\tx\n", " "])
def test_text_is_taken_exactly_as_given_spaces_included(text):
    # a space can be data (the lightest character of a text-art palette), so nothing is trimmed
    assert Probe(label=text).label == text


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
    with pytest.raises(ValueError, match="tint"):
        Probe(tint=written)


# --- Picture ---------------------------------------------------------------------------------------------------------


class Textured(EffectParams):
    """A model with a picture field."""

    image: Picture | None = param(None, doc="A picture laid over the photo, or none.")


def noise_picture(shape=(4, 6, 4), seed=0, label="noise.png") -> Picture:
    return Picture(np.random.default_rng(seed).random(shape, dtype=np.float32), label)


def test_a_picture_field_defaults_to_none_and_takes_a_picture_or_a_colour_as_a_flat_one():
    assert Textured().image is None
    flat = Textured(image="#808080").image
    assert flat is not None
    assert (flat.width, flat.height) == (1, 1)
    np.testing.assert_allclose(flat.pixels[0, 0], [128 / 255, 128 / 255, 128 / 255, 1.0], atol=1e-7)


def test_a_picture_is_kept_as_it_is_not_copied_when_validated_again():
    picture = noise_picture()
    assert Textured(image=picture).image is picture
    assert Textured(image=Textured(image=picture).image).image is picture


@pytest.mark.parametrize(
    "written",
    ["tex.png", "#zzzzzz", [1, 0], 7, np.zeros((5, 5, 3), np.float32), np.zeros((5, 5, 3)).tolist()],
    ids=["a path", "bad hex", "two numbers", "a number", "an array", "nested lists"],
)
def test_anything_but_a_picture_or_a_colour_is_rejected_naming_the_field_and_never_printing_pixels(written):
    with pytest.raises(RecipeError, match="`image`") as raised:
        Textured(image=written)
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


@pytest.mark.parametrize(
    "make",
    [
        pytest.param(lambda: param("#808080", unit="px", doc="d"), id="a unit"),
        pytest.param(lambda: param("#808080", min=0, max=1, doc="d"), id="a range"),
    ],
)
def test_a_picture_field_takes_no_unit_or_range(make):
    with pytest.raises(ValueError, match="Broken"):
        _declare(Picture, make())


class Lit(EffectParams):
    """A model with a field parameter and a plain text one."""

    field: FieldName = param("light", doc="The selection the probe is lit by.")
    label: str = param("lamp", doc="A name shown in labels.")


def test_a_model_names_its_field_parameters_apart_from_plain_text():
    assert parameters.field_name_fields(table_of(Lit)) == ("field",)
    assert parameters.field_name_fields(table_of(Probe)) == ()


@pytest.mark.parametrize("written", ["", 1, None], ids=["blank", "number", "none"])
def test_a_field_parameter_takes_the_name_of_a_selection_only(written):
    with pytest.raises(RecipeError, match="`field`"):
        Lit(field=written)
    assert Lit(field="depth").field == "depth"


@pytest.mark.parametrize(
    "make",
    [
        pytest.param(lambda: param("depth", unit="px", doc="d"), id="a unit"),
        pytest.param(lambda: param("depth", min=0, max=1, doc="d"), id="a range"),
        pytest.param(lambda: param("depth", drivable=True, doc="d"), id="drivable"),
    ],
)
def test_a_field_parameter_takes_no_unit_range_or_drive_when_declared(make):
    with pytest.raises(ValueError, match="Broken"):
        _declare(FieldName, make())


# --- the rules of a declaration, checked when the class is made ------------------------------------------------------


def _declare(annotation, field):
    """Make a model with one field `x: annotation = field`, as an effect module would at import."""
    return type("Broken", (EffectParams,), {"__annotations__": {"x": annotation}, "x": field, "__doc__": "Broken."})


@pytest.mark.parametrize(
    ("annotation", "make", "complaint"),
    [
        (float, lambda: param(1.0, unit="px", min=0, max=2, doc=""), "doc"),
        (float, lambda: param(1.0, unit="px", min=0, max=2, doc="   "), "doc"),
        (float, lambda: param(1.0, min=0, max=2, doc="d"), None),
        (float, lambda: param(1.0, unit="pixels", min=0, max=2, doc="d"), None),
        (float, lambda: param(1.0, unit="px", max=2, doc="d"), None),
        (int, lambda: param(1, unit="count", min=0, doc="d"), None),
        (float, lambda: param(1.0, unit="px", min=3, max=2, doc="d"), None),
        (Color, lambda: param("not a colour", doc="d"), None),
        (float, lambda: param(5.0, unit="px", min=0, max=2, doc="d"), None),
        (str, lambda: param("a", min=0, max=2, doc="d"), None),
        (str, lambda: param("a", unit="px", doc="d"), None),
        (Literal["soft", "hard"], lambda: param("soft", doc="Edge: 'soft' blends."), "hard"),
        (float, lambda: 1.0, "param"),  # a plain default: no description, no range
        (tuple[str, ...], lambda: param(("a",), min=0, max=1, doc="d"), None),
        (bool, lambda: param(False, unit="count", doc="d"), None),
        (float, lambda: param(1.0, unit="px", min=0, max=360, cyclic=True, doc="d"), None),
        (float, lambda: param(1.0, unit="degrees", min=0, max=180, cyclic=True, doc="d"), None),
        (float, lambda: param(1.0, unit="degrees", min=-360, max=360, cyclic=True, doc="d"), None),
        (str, lambda: param("a", cyclic=True, doc="d"), None),
    ],
)
def test_a_badly_declared_field_fails_when_the_class_is_made_naming_model_and_field(annotation, make, complaint):
    with pytest.raises(ValueError, match="Broken") as raised:
        _declare(annotation, make())
    message = str(raised.value)
    assert '"x"' in message or "`x`" in message
    assert complaint is None or complaint in message  # the core words most of the rules; the rest are this class's


def test_a_field_without_any_default_fails_when_the_class_is_made():
    with pytest.raises(ValueError, match=r"Broken.*`x`"):
        type("Broken", (EffectParams,), {"__annotations__": {"x": float}, "__doc__": "Broken."})


def test_a_seed_is_a_count_from_zero_to_the_largest_32_bit_number():
    class Seeded(EffectParams):
        """Seeded."""

        seed: int = param(0, unit="count", min=0, max=2**32 - 1, doc="Picks the random pattern.")

    assert Seeded(seed=2**32 - 1).seed == 2**32 - 1
    with pytest.raises(ValueError, match="seed"):
        Seeded(seed=2**32)


# --- documentation generated from the model --------------------------------------------------------------------------


def _entry(text: str, name: str) -> str:
    """The documentation entry of field `name`: its heading line and the indented lines under it."""
    lines = text.splitlines()
    start = next(i for i, line in enumerate(lines) if line.startswith(f"{name} : "))
    end = next((i for i in range(start + 1, len(lines)) if not lines[i].startswith(" ")), len(lines))
    return " ".join(" ".join(lines[start:end]).split())


class Tiered(EffectParams):
    """A model whose advanced fields sit between its basic ones."""

    amount: float = param(0.5, unit="fraction", min=0, max=1, doc="How much of the look is applied.")
    edge: Literal["reflect", "wrap"] = param("reflect", advanced=True, doc="Past the border: 'reflect' or 'wrap'.")
    angle: float = param(30.0, unit="degrees", min=0, max=180, doc="Direction of the look.")
    softness: float = param(2.0, unit="px", min=0, max=10, advanced=True, doc="Blur of the technique's mask.")


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
        with pytest.raises(ValueError, match=refused):
            Tiered(**values)


# --- strictness for every kind of field (known-good and known-bad per kind) -----------------------------------------


class Kinds(EffectParams):
    """One field of each kind an effect may declare."""

    invert: bool = param(False, doc="Swap dark and light.")
    count: int = param(3, unit="count", min=1, max=9, doc="How many.")
    radius: float = param(2.0, unit="px", min=0, max=10, doc="How far.")
    mode: Literal["soft", "hard"] = param("soft", doc="Edge: 'soft' or 'hard'.")
    tint: Color = param("#000000", doc="A colour.")
    stops: tuple[Color, ...] = param(("#000", "#fff"), doc="Colours in order.", min_length=2)


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
        ({"stops": ["#000000", [1, 1, 1]]}, "stops", ((0.0, 0.0, 0.0), (1.0, 1.0, 1.0))),
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
        ({"stops": [1, 2]}, "stops"),
        ({"stops": ["#000"]}, "stops"),
    ],
)
def test_every_kind_of_field_refuses_values_of_another_kind(values, field):
    with pytest.raises(ValueError, match=field):
        Kinds(**values)


# --- parse: the one way callers validate ------------------------------------------------------------


@pytest.mark.parametrize("value", [float("nan"), float("inf")], ids=["nan", "infinity"])
def test_a_nan_or_infinity_is_reported_as_not_a_finite_number_not_as_out_of_range(value):
    with pytest.raises(ValueError, match=r"`radius`.*finite") as raised:
        Probe(radius=value)
    assert "<=" not in str(raised.value)


# --- error wording ---------------------------------------------------------------------------------------------------


# --- drivable fields: a parameter a field may vary pixel by pixel ----------------------------------------------------


class Drivable(EffectParams):
    """A model with one field of each kind that matters to driving."""

    radius: float = param(24.0, unit="px", min=0, max=500, drivable=True, doc="Size of the halo. Larger = wider.")
    amount: float = param(0.5, unit="fraction", min=0, max=1, doc="How much of it. More = stronger.")
    count: int = param(3, unit="count", min=1, max=9, doc="How many halos.")


@pytest.mark.parametrize(
    ("annotation", "make", "complaint"),
    [
        (int, lambda: param(3, unit="count", min=1, max=9, drivable=True, doc="d"), "decimal"),
        (str, lambda: param("a", drivable=True, doc="d"), "decimal"),
        (bool, lambda: param(False, drivable=True, doc="d"), "decimal"),
        (Color, lambda: param("#ff0000", drivable=True, doc="d"), "decimal"),
        (Literal["a", "b"], lambda: param("a", drivable=True, doc="a, b"), "decimal"),
    ],
)
def test_only_a_decimal_number_can_be_declared_drivable_naming_model_and_field(annotation, make, complaint):
    with pytest.raises(ValueError, match="Broken") as raised:
        _declare(annotation, make())
    assert '"x"' in str(raised.value)
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
    "an end that is not finite": ({"by": "near", "start": 0, "end": float("inf")}, "finite"),
    "a nan": ({"by": "near", "start": float("nan"), "end": 1}, "finite"),
    "a misspelt key": ({"by": "near", "start": 0, "end": 1, "step": 3}, "step"),
    "no end": ({"by": "near", "start": 0}, "end"),
    "no field": ({"start": 0, "end": 1}, "by"),
}


@pytest.mark.parametrize(("values", "complaint"), DRIVES.values(), ids=DRIVES.keys())
def test_a_drive_names_its_field_its_two_ends_and_how_many_levels_or_is_refused_naming_the_key(values, complaint):
    if complaint is None:
        assert Drive(**values).by == values["by"]
        return
    with pytest.raises((ValueError, TypeError), match=complaint):
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


@pytest.mark.parametrize("levels", [2, 3, 16])
@pytest.mark.parametrize(
    ("start", "end"),
    [(0.05, 0.0), (0.08, 1.0), (0.0, 1.0), (1.0, 0.0), (0.1, 0.1), (0.3, 0.7), (-1.0, 1.0), (0.2, 4.0), (4.0, 0.4)],
)
def test_a_drive_runs_exactly_from_its_start_to_its_end_and_never_outside_them(start, end, levels):
    values = Drive(by="f", start=start, end=end, levels=levels).values()
    assert values[0] == start
    assert values[-1] == end  # not a float rounding of it: a parameter at its bound must stay valid
    assert all(min(start, end) <= value <= max(start, end) for value in values)


def test_a_drive_is_frozen_and_equal_to_another_of_the_same_values_and_says_what_it_is():
    drive = Drive(by="near", start=0, end=30)
    assert drive == Drive(by="near", start=0.0, end=30.0, levels=5)
    assert drive != Drive(by="near", start=0, end=31)
    with pytest.raises(FrozenInstanceError):
        drive.start = 5  # type: ignore[misc]  # the point of the test
    assert drive.describe() == "by near 0..30"


# --- cyclic fields: an angle that wraps ------------------------------------------------------------------------------


class Turning(EffectParams):
    """A model with a direction (cyclic, one turn from -180), a twist (not cyclic) and a whole-degree angle."""

    angle: float = param(45.0, unit="degrees", min=-180, max=180, cyclic=True, doc="Direction. -180 to 180.")
    twist: float = param(0.0, unit="degrees", min=-720, max=720, doc="How far it winds. -720 to 720.")


@pytest.mark.parametrize(
    ("written", "stored"),
    [
        pytest.param(180, 180, id="the top of the turn stays the top"),
        pytest.param(-180, -180, id="the bottom of the turn stays the bottom"),
        pytest.param(90, 90, id="inside the turn"),
        pytest.param(190, -170, id="just past the top wraps to the bottom"),
        pytest.param(-190, 170, id="just past the bottom wraps to the top"),
        pytest.param(725, 5, id="two turns and a bit"),
        pytest.param(-1_000_005, 75, id="far below"),  # -1_000_005 + 2778 * 360,
    ],
)
def test_a_cyclic_field_takes_any_real_and_stores_the_same_direction_inside_its_turn(written, stored):
    assert Turning(angle=written).angle == pytest.approx(stored)


@pytest.mark.parametrize(
    ("values", "field"),
    [
        pytest.param({"angle": float("nan")}, "angle", id="nan"),
        pytest.param({"angle": float("inf")}, "angle", id="infinity"),
        pytest.param({"angle": "90"}, "angle", id="a string"),
        pytest.param({"angle": True}, "angle", id="a bool"),
        pytest.param({"twist": 721}, "twist", id="a field that is not cyclic keeps its range"),
    ],
)
def test_a_cyclic_field_still_refuses_what_is_no_real_number_and_others_keep_their_range(values, field):
    with pytest.raises(RecipeError, match=field):
        Turning(**values)


@pytest.mark.parametrize(
    ("written", "stored"),
    [
        pytest.param(np.float32(400), 40, id="a float32"),
        pytest.param(np.float64(-190), 170, id="a float64"),
        pytest.param(np.int64(725), 5, id="an int64"),
        pytest.param(np.float32(90), 90, id="a float32 inside the turn"),
    ],
)
def test_a_cyclic_field_takes_a_numpy_number_as_it_takes_a_python_one(written, stored):
    assert Turning(angle=written).angle == pytest.approx(stored)


def test_only_a_decimal_can_be_cyclic():
    with pytest.raises(ValueError, match=r"Whole.*cyclic"):

        class Whole(EffectParams):
            """A whole-degree direction."""

            turns: int = param(0, unit="degrees", min=0, max=360, cyclic=True, doc="Whole-degree direction. 0 to 360.")


# --- the table a class declares, and what reads it -----------------------------------------------------------------


def test_a_class_declares_its_parameter_table_with_each_field_in_the_formats_terms():
    table = table_of(Probe)
    assert table["doc"] == "A known-good model: one field of every kind."
    kinds = {entry["name"]: (entry["kind"], entry["default"]) for entry in table["fields"]}
    assert kinds == {
        "radius": ("float", 24.0),
        "count": ("int", 3),
        "mode": ("choice", "soft"),
        "tint": ("color", "#ff8000"),
        "label": ("text", "probe"),
    }
    radius = table["fields"][0]
    assert (radius["unit"], radius["min"], radius["max"]) == ("px", 0, 500)


def test_the_signature_of_a_table_lists_every_field_in_order_with_its_default_and_the_type_a_caller_writes():
    text = str(parameters.signature(table_of(Probe)))
    assert text.startswith("(*, radius: float = 24.0, count: int = 3, mode: Literal['soft', 'hard'] = 'soft', tint: ")
    assert "tint: str | list[float] | tuple[float, float, float] = '#ff8000'" in text


def test_the_field_docs_of_a_table_give_each_fields_type_unit_range_default_and_description():
    radius, _, mode, tint, _ = parameters.fields_doc(table_of(Probe))
    assert (radius.type, radius.unit, radius.range, radius.default) == ("number", "px", "0 to 500", 24.0)
    assert radius.doc.startswith("Size of the probe")
    assert mode.type == "one of 'soft', 'hard'"
    assert (tint.type, tint.range) == ("colour", "")


class Pixels(EffectParams):
    """A model whose lookup table, field name and optional colour have their defaults."""

    file: Lut = param(Lut.identity(), doc="The table.")
    field: FieldName = param("depth", doc="The selection.")
    glow: Color | None = param(None, doc="The glow's colour, or none.")


def test_a_lookup_table_defaults_to_the_identity_and_takes_a_lut_only():
    assert Pixels().file == Lut.identity()
    other = Lut(np.zeros((2, 2, 2, 3), np.float32) + 0.5)
    assert Pixels(file=other).file is other
    for given in ["film.cube", np.zeros((2, 2, 2, 3))]:
        with pytest.raises(ValueError, match="Lut"):
            Pixels(file=given)


@pytest.mark.parametrize("copy_it", [copy.copy, copy.deepcopy])
def test_a_model_with_the_identity_table_is_equal_to_its_copy(copy_it):
    model = Pixels()
    assert copy_it(model) == model


def test_a_field_defaults_to_its_declared_name_and_takes_a_non_empty_name_only():
    assert Pixels().field == "depth"
    assert Pixels(field="near").field == "near"
    for given in ["", 3, True, ["depth"], None]:
        with pytest.raises(RecipeError, match=r"`field`"):
            Pixels(field=given)


def test_an_optional_colour_defaults_to_none_and_takes_a_colour_or_none():
    assert Pixels().glow is None
    assert Pixels(glow=None).glow is None
    for given in ["#00f", " #0000FF ", [0, 0, 1], (0.0, 0.0, 1.0)]:
        assert Pixels(glow=given).glow == (0.0, 0.0, 1.0), given
    for bad in ["red", [2, 0, 0], 1, False, ""]:
        with pytest.raises(RecipeError, match=r"`glow`"):
            Pixels(glow=bad)


class Banded(EffectParams):
    """Two numbers that stay ordered."""

    low: int = param(10, unit="px", min=1, max=500, at_most="high", doc="The lowest, at most `high`.")
    high: int = param(100, unit="px", min=1, max=500, doc="The highest.")


@pytest.mark.parametrize(
    ("given", "valid"),
    [({}, True), ({"low": 100}, True), ({"low": 101}, False), ({"high": 9}, False), ({"low": 50, "high": 50}, True)],
)
def test_a_number_may_not_exceed_the_one_it_is_limited_by_whichever_is_given(given, valid):
    if valid:
        assert Banded(**given)
    else:
        with pytest.raises(RecipeError, match=r"`low`.*`high`|`high`.*`low`"):
            Banded(**given)


def test_a_limit_must_name_another_field_of_the_class():
    with pytest.raises(ValueError, match="Lonely"):

        class Lonely(EffectParams):
            """A limit that names nothing."""

            low: int = param(10, unit="px", min=1, max=500, at_most="high", doc="The lowest.")
