"""What the Python facade keeps true for every effect, whatever runs it: the picture a call returns, shipped presets,
documentation, parameter names and conventions, categories, drives, and the registration of a Python effect. The kernel
rules (limits, choices, seeds, sizes, threads) are `crates/core/tests/effects_contract.rs`."""

import re
import typing
from typing import Literal

import numpy as np
import pytest

from bullterrier_image import fx
from bullterrier_image import recipe as facade
from bullterrier_image._native import _core  # noqa: PLC2701  architecture: the effects the Rust core implements
from bullterrier_image.core import registry
from bullterrier_image.core.params import (
    UNITS,
    Drive,
    EffectParams,
    drivable_fields,
    field_name_fields,
    param,
    picture_fields,
)
from bullterrier_image.core.params import signature as signature_of
from bullterrier_image.core.regions import Kind
from bullterrier_image.core.registry import effect
from bullterrier_image.core.steps import Recipe, Step, drives
from bullterrier_image.core.values import FieldName, Picture
from bullterrier_image.errors import RecipeError
from bullterrier_image.loader import load_recipe
from bullterrier_image.recipe import Band, Curve, Edge, Framing, Given, Line

ALL = sorted(registry.describe())
# What the Rust core runs is held to the same rules by `crates/core/tests/effects_contract.rs`; the tests below that
# run an effect's kernel over its parameters, sizes and choices cover the effects registered from Python only.
PYTHON = [name for name in ALL if name not in set(_core.effects(declared=True))]


def assert_a_picture_like(out: np.ndarray, rgb: np.ndarray) -> None:
    """`out` is a picture of `rgb`'s shape: float32, finite, every value in 0..1."""
    assert out.shape == rgb.shape
    assert out.dtype == np.float32
    assert np.isfinite(out).all()
    assert out.min() >= -1e-6
    assert out.max() <= 1 + 1e-6


@pytest.mark.parametrize("name", ALL)
def test_every_effect_keeps_shape_dtype_and_range_is_deterministic_and_leaves_its_input_alone(name, photo):
    before = photo.copy()
    inputs = _inputs(name, photo)
    first, second = registry.apply(name, photo, **inputs), registry.apply(name, photo, **inputs)
    assert_a_picture_like(first, photo)
    np.testing.assert_array_equal(first, second)
    np.testing.assert_array_equal(photo, before)


SHIPPED_PRESETS = [(name, preset) for name in ALL for preset in registry.presets(name)]


@pytest.mark.parametrize(("name", "preset"), SHIPPED_PRESETS, ids=[f"{n}-{p}" for n, p in SHIPPED_PRESETS])
def test_every_shipped_preset_is_valid_for_its_effect_and_makes_a_picture_that_differs_from_the_default(
    name, preset, photo
):
    step = Step(name, preset=preset)  # validated by the effect's own model, like a recipe's step
    inputs = {**_pictures(name), **_inputs(name, photo)}
    out = registry.apply(name, photo, **{**step.params, **inputs})  # a preset names a field; the array replaces it
    assert_a_picture_like(out, photo)
    assert not np.array_equal(out, registry.apply(name, photo, **inputs))


def test_every_effect_of_the_rust_core_is_a_registered_effect():
    assert set(_core.effects(declared=True)) <= set(ALL)


@pytest.mark.parametrize("name", ALL)
def test_every_effect_takes_a_float64_picture_and_makes_the_float32_one_of_the_same_values(name, photo):
    wide = photo.astype(np.float64)
    out = registry.apply(name, wide, **_inputs(name, photo))
    assert_a_picture_like(out, photo)
    np.testing.assert_array_equal(out, registry.apply(name, photo, **_inputs(name, photo)))


@pytest.mark.parametrize("name", ALL)
def test_every_effect_documents_itself(name):
    assert registry.describe()[name]


def test_unknown_effect_lists_choices(photo, throwaway_effects, register_probe):
    register_probe("listed_probe")
    with pytest.raises(KeyError) as raised:
        registry.apply("nope", photo)
    message = str(raised.value)
    # the given name and the choices
    assert "'nope'" in message
    assert "listed_probe" in message
    assert "ascii" in message


def test_an_emptied_registry_holds_only_what_the_test_registers_and_gets_the_real_effects_back(
    registry_emptied, register_probe
):
    """The helper behind `fresh_registry`: real effects come back, in their order, even when the block fails."""
    real = registry.all_effects()
    real_categories = {name: registry.category(name) for name in real}

    def use_the_emptied_registry_then_fail():
        with registry_emptied():
            assert registry.all_effects() == {}
            register_probe("only_probe", "light")
            assert list(registry.all_effects()) == ["only_probe"]
            raise RuntimeError("test failed")

    with pytest.raises(RuntimeError, match="test failed"):
        use_the_emptied_registry_then_fail()
    assert list(registry.all_effects().items()) == list(real.items())
    assert {name: registry.category(name) for name in real} == real_categories


# --- parameter vocabulary ----------------------------------------------------------------------------------------


def _naming_problems(name: str) -> list[str]:
    """Every parameter of the effect `name` that uses a discouraged name, with the name to use instead."""
    from bullterrier_image.core.vocabulary import DISCOURAGED

    return [
        f"effect {name!r}: parameter `{parameter}` should be called `{DISCOURAGED[parameter]}`"
        for parameter in registry.parameters(name).model_fields
        if parameter in DISCOURAGED
    ]


@pytest.mark.parametrize("name", ALL)
def test_no_effect_uses_a_discouraged_parameter_name(name):
    assert _naming_problems(name) == []


def test_the_vocabulary_check_flags_discouraged_field_names_and_passes_preferred_ones(throwaway_effects):
    class StrengthParams(EffectParams):
        """Uses two discouraged names."""

        strength: float = param(0.5, unit="fraction", min=0, max=1, doc="Should be called amount.")
        radius: float = param(3.0, unit="px", min=0, max=10, doc="A preferred name.")
        rng: int = param(0, unit="count", min=0, max=9, doc="Should be called seed.")

    class AmountParams(EffectParams):
        """Uses the preferred names."""

        amount: float = param(0.5, unit="fraction", min=0, max=1, doc="A preferred name.")
        seed: int = param(0, unit="count", min=0, max=9, doc="A preferred name.")

    @effect(category="light", params=StrengthParams)
    def bad_probe(rgb, params):
        """Uses discouraged field names."""
        return rgb

    @effect(category="light", params=AmountParams)
    def good_probe(rgb, params):
        """Uses preferred field names."""
        return rgb

    assert _naming_problems("bad_probe") == [
        "effect 'bad_probe': parameter `strength` should be called `amount`",
        "effect 'bad_probe': parameter `rng` should be called `seed`",
    ]
    assert _naming_problems("good_probe") == []


def _edge_naming_problems(name: str) -> list[str]:
    """Every choice field of the effect `name` that offers edge rules ('reflect' or 'mirror' among its choices, so it
    decides what lies beyond the picture's edge) under a name other than `edge`."""
    return [
        f"effect {name!r}: field `{field}` chooses an edge rule and should be called `edge`"
        for field, spec in registry.parameters(name).model_json_schema()["properties"].items()
        if field != "edge" and {"reflect", "mirror"} & set(spec.get("enum", ()))
    ]


@pytest.mark.parametrize("name", ALL)
def test_what_lies_beyond_the_picture_edge_is_always_a_field_called_edge(name):
    assert _edge_naming_problems(name) == []


class ModeAsEdgeRule(EffectParams):
    """The old name: `mode` choosing edge rules."""

    mode: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect", doc="Edge rule: 'reflect', 'nearest', 'mirror' or 'wrap'."
    )


class BorderWithMirror(EffectParams):
    """Another name, recognised by 'mirror' alone."""

    border: Literal["nearest", "mirror"] = param("nearest", doc="Edge rule: 'nearest' or 'mirror'.")


class BeyondWithReflect(EffectParams):
    """Another name, recognised by 'reflect' alone, beside 'constant'."""

    beyond: Literal["constant", "reflect"] = param("reflect", doc="Edge rule: 'constant' or 'reflect'.")


class EdgeNamedEdge(EffectParams):
    """The right name."""

    edge: Literal["reflect", "nearest", "mirror", "wrap"] = param(
        "reflect", doc="Edge rule: 'reflect', 'nearest', 'mirror' or 'wrap'."
    )


class ModeOfSomethingElse(EffectParams):
    """`mode` is free for choices that are no edge rules."""

    mode: Literal["spin", "zoom"] = param("spin", doc="'spin' or 'zoom'.")


class InterpolationChoice(EffectParams):
    """The sampling of a warp, whose choices share 'nearest' with the edge rules."""

    interpolation: Literal["nearest", "bilinear", "bicubic"] = param(
        "bilinear", doc="'nearest', 'bilinear' or 'bicubic'."
    )


EDGE_PROBES: dict[str, tuple[type[EffectParams], str | None]] = {
    "the old name `mode` for the edge rules": (ModeAsEdgeRule, "mode"),
    "any other name, found by 'mirror' alone": (BorderWithMirror, "border"),
    "any other name, found by 'reflect' alone": (BeyondWithReflect, "beyond"),
    "the name `edge`": (EdgeNamedEdge, None),
    "`mode` for choices that are no edge rule": (ModeOfSomethingElse, None),
    "`interpolation` for the sampling of a warp": (InterpolationChoice, None),
}


@pytest.mark.parametrize(("params", "misnamed"), EDGE_PROBES.values(), ids=EDGE_PROBES.keys())
def test_the_edge_naming_check_flags_edge_rules_under_another_name_and_passes_the_rest(
    params, misnamed, throwaway_effects
):
    @effect(category="light", params=params)
    def edge_probe(rgb, params):
        """Has one choice field."""
        return rgb

    problems = _edge_naming_problems("edge_probe")
    assert len(problems) == (misnamed is not None)
    assert all(f"`{misnamed}`" in problem for problem in problems)


# --- categories --------------------------------------------------------------------------------------------------


def test_categories_maps_every_category_in_catalogue_order_to_its_sorted_effects(fresh_registry, register_probe):
    """A category with no effects is listed too, empty. The catalogue order is `registry.CATEGORIES`, the published
    list effect authors choose from."""
    for name, category in [("zeta", "utilities"), ("beta", "colour"), ("alpha", "colour"), ("gamma", "light")]:
        register_probe(name, category)  # registered out of order, on purpose
    expected: dict[str, list[str]] = {category: [] for category in registry.CATEGORIES}
    expected |= {"colour": ["alpha", "beta"], "light": ["gamma"], "utilities": ["zeta"]}
    assert registry.categories() == expected
    assert list(registry.categories()) == list(registry.CATEGORIES)  # dict order is the catalogue order
    assert registry.categories()["distortion"] == []


def test_the_real_effects_are_each_in_exactly_one_category():
    grouped = registry.categories()
    assert list(grouped) == list(registry.CATEGORIES)
    assert sorted(name for names in grouped.values() for name in names) == ALL
    assert all(names == sorted(names) for names in grouped.values())


# --- the parameter model of every effect --------------------------------------------------------------------


def _small(photo: np.ndarray) -> np.ndarray:
    return np.ascontiguousarray(photo[::4, ::4])  # 24 x 32: quick, even with every parameter at its maximum


def _inputs(name: str, photo: np.ndarray) -> dict[str, np.ndarray]:
    """The injected inputs the effect declares, made for `photo`: a soft disc as the subject map, a cone as its
    depth (1, nearest, at the centre, falling to 0 at the corners), the four quarters of the picture as its
    partition, and that cone again as the array of each of its field parameters."""
    height, width = photo.shape[:2]
    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    off_centre = np.hypot(x - width / 2, y - height / 2)
    disc = np.clip(min(height, width) / 3 - off_centre, 0, 1)
    cone = 1 - off_centre / off_centre.max()
    quarters = (y * 2 // height) * 2 + x * 2 // width
    made = {
        "subject": disc.astype(np.float32),
        "depth": cone.astype(np.float32),
        "partition": quarters.astype(np.int32),
    }
    fields = dict.fromkeys(field_name_fields(registry.parameters(name)), made["depth"])
    return {**fields, **{key: value for key, value in made.items() if key in registry.inputs(name)}}


def _pictures(name: str) -> dict[str, Picture]:
    """A small picture with some transparency for each picture parameter of the effect (none are set by default)."""
    asset = Picture(np.random.default_rng(0).random((6, 8, 4), dtype=np.float32))
    return dict.fromkeys(picture_fields(registry.parameters(name)), asset)


@pytest.mark.parametrize("name", ALL)
def test_every_parameter_is_described_and_has_a_unit_entry(name):
    schema = registry.parameters(name).model_json_schema()
    for field, prop in schema["properties"].items():
        assert prop.get("description", "").strip(), f"{name}.{field} has no description"
        assert "unit" in prop, f"{name}.{field} has no unit entry"
        if prop.get("type") in ("number", "integer"):
            assert prop["unit"] in UNITS, f"{name}.{field}: unit {prop['unit']!r}"


@pytest.mark.parametrize("name", ALL)
def test_every_effect_takes_its_model_its_keywords_or_nothing(name, photo):
    small, model = _small(photo), registry.parameters(name)
    inputs = _inputs(name, small)
    defaults = {field: info.default for field, info in model.model_fields.items() if field not in inputs}
    by_model = registry.get(name)(small, model(), **inputs)
    np.testing.assert_array_equal(registry.get(name)(small, **inputs), by_model)
    np.testing.assert_array_equal(registry.get(name)(small, **defaults, **inputs), by_model)


def _picture_changes_the_background(name: str) -> bool:
    """Whether the effect's output depends on more than the picture's size."""
    busy = np.random.default_rng(0).random((31, 41, 3), dtype=np.float32)
    return not np.array_equal(registry.apply(name, busy), registry.apply(name, np.zeros_like(busy)))


@pytest.mark.parametrize("name", registry.categories()["backgrounds"])
def test_a_background_takes_only_its_size_from_the_picture(name):
    assert not _picture_changes_the_background(name)


def test_a_background_that_reads_the_picture_is_caught(throwaway_effects, register_probe):
    register_probe("reading_probe", "backgrounds")  # returns the picture, so the picture shows through
    assert _picture_changes_the_background("reading_probe")


# --- the effects that scatter particles of light -----------------------------------------------------------------


def test_no_drops_leave_the_picture_unchanged_in_a_new_array(photo):
    out = registry.apply("lens_drops", photo, count=0)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)


# --- every drivable parameter: a field may vary it pixel by pixel, the effect itself unchanged ------------------------

# every effect: none registered from Python is drivable now, and the facade must still build and render a core Drive
DRIVABLE = sorted((name, field) for name in ALL for field in drivable_fields(registry.parameters(name)))


def _range(name: str, field: str) -> tuple[float, float]:
    """The documented (least, greatest) of the number `field` of the effect `name`."""
    doc = next(doc for doc in registry.parameters(name).fields_doc() if doc.name == field)
    assert doc.minimum is not None
    assert doc.maximum is not None
    return doc.minimum, doc.maximum


def _driven_and_plain(name: str, field: str, levels: int, photo: np.ndarray):
    """A recipe driving `field` of `name` by the given map `f`, from its least to its greatest value in `levels`, and
    a function that makes the plain recipe at one value; the maps the effect takes are made as for the other tests."""
    least, greatest = _range(name, field)
    driven = Recipe(
        [Step(name, params={field: Drive(by="f", start=least, end=greatest, levels=levels)})],
        name="driven",
        size=None,
        regions={"f": Given()},
    )

    def plain(value: float) -> Recipe:
        return Recipe([Step(name, params={field: value})], name="plain", size=None)

    return driven, plain, _inputs(name, photo)


def test_some_parameters_are_drivable():
    assert DRIVABLE  # else every test below is parametrized over nothing and passes without testing anything


@pytest.mark.parametrize(("name", "field"), DRIVABLE)
def test_a_drivable_field_says_so_in_the_documentation_and_the_signature(name, field):
    assert f"{field} : number (drivable)" in registry.documentation(name)
    assert Drive in typing.get_args(signature_of(registry.parameters(name)).parameters[field].annotation)


@pytest.mark.parametrize("level", [0, 2, 4])
@pytest.mark.parametrize(("name", "field"), DRIVABLE)
def test_a_constant_field_gives_the_plain_effect_at_that_levels_value(name, field, level, photo):
    least, greatest = _range(name, field)
    driven, plain, maps = _driven_and_plain(name, field, 5, photo)
    expected = facade.render(photo, plain(least + (greatest - least) * level / 4), maps=maps)
    got = facade.render(photo, driven, maps={**maps, "f": np.full(photo.shape[:2], level / 4, np.float32)})
    np.testing.assert_allclose(got, expected, atol=1e-6, rtol=0)


@pytest.mark.parametrize(("name", "field"), DRIVABLE)
def test_two_levels_mix_the_effect_at_the_least_and_the_greatest_value_by_the_field(name, field, photo):
    least, greatest = _range(name, field)
    driven, plain, maps = _driven_and_plain(name, field, 2, photo)
    ramp = np.tile(np.linspace(0, 1, photo.shape[1], dtype=np.float32), (photo.shape[0], 1))
    at_least, at_greatest = (facade.render(photo, plain(value), maps=maps) for value in (least, greatest))
    got = facade.render(photo, driven, maps={**maps, "f": ramp})
    np.testing.assert_allclose(got, (1 - ramp)[..., None] * at_least + ramp[..., None] * at_greatest, atol=1e-6, rtol=0)


@pytest.mark.parametrize(("name", "field"), DRIVABLE)
def test_a_drive_that_leaves_the_parameters_range_is_refused_naming_the_field(name, field):
    least, greatest = _range(name, field)
    for start, end in ((least - 1, greatest), (least, greatest + 1)):
        with pytest.raises(RecipeError, match=rf"{name}.*`{field}`"):
            Step(name, params={field: Drive(by="f", start=start, end=end)})


@pytest.mark.parametrize(("name", "field"), DRIVABLE)
def test_fx_builds_a_driven_step_from_a_drive(name, field):
    least, greatest = _range(name, field)
    step = getattr(fx, name)(**{field: Drive(by="depth", start=least, end=greatest)})
    assert drives(step) == {field: Drive(by="depth", start=least, end=greatest)}


# --- a field parameter: the recipe names a selection, the effect gets its array --------------------------------------


class FieldProbeParams(EffectParams):
    """A field and a gain: the probe of the field mechanism."""

    field: FieldName = param("depth", doc="The selection that darkens the picture: it is multiplied by 1 - field / 2.")
    gain: float = param(1.0, unit="ratio", min=0, max=4, drivable=True, doc="Multiplies every value after the field.")


@pytest.fixture
def field_probe(throwaway_effects):
    """`field_probe`, an effect with a field parameter: clip(rgb x (1 - field / 2) x gain)."""

    @effect(category="light", params=FieldProbeParams)
    def field_probe(rgb, params, *, field):
        """Darkens the picture by a field and multiplies it by a gain."""
        return np.clip(rgb * (1 - field[..., None] / 2) * params.gain, 0, 1).astype(np.float32)


# Rec. 709, written here again so that the test does not use the code's (float32 like the picture: a relief amplifies)
LUMA = np.asarray([0.2126, 0.7152, 0.0722], np.float32)
FIELD_REGIONS: dict[str, Kind] = {"g": Given(), "middle": Band(of="depth", low=0.3, high=0.7, feather=0)}


def field_maps(photo: np.ndarray, subject: np.ndarray) -> dict[str, np.ndarray]:
    """The maps a field probe recipe is given: a depth ramp across the picture (unlike the picture's own light and the
    subject disc), and a map `g` for a `Given` region."""
    depth = np.broadcast_to(np.linspace(0, 1, photo.shape[1], dtype=np.float32), photo.shape[:2]).copy()
    return {"depth": depth, "subject": subject, "g": 1 - depth}


def the_field(name: str, photo: np.ndarray, maps: dict[str, np.ndarray]) -> np.ndarray:
    """What the selection `name` is, by hand: the oracle for the array an effect should get."""
    return {
        "depth": maps["depth"],
        "subject": maps["subject"],
        "background": 1 - maps["subject"],
        # Rounded in the order the core sums in (red, then green, then blue, each product rounded to float32): `photo @
        # LUMA` runs in BLAS, whose order and fused multiply-add differ by platform, and an ulp of light is
        # amplified by effects such as relight to 4e-6, past this test's tolerance.
        "light": photo[..., 0] * LUMA[0] + photo[..., 1] * LUMA[1] + photo[..., 2] * LUMA[2],
        "g": maps["g"],
        "middle": (maps["depth"] > 0.3) & (maps["depth"] < 0.7),
    }[name].astype(np.float32)


@pytest.mark.parametrize("name", ["depth", "light", "subject", "background", "g", "middle"])
def test_a_field_named_in_a_recipe_gives_the_same_picture_as_the_equal_array_passed_directly(
    name, field_probe, photo, subject
):
    maps = field_maps(photo, subject)
    recipe = Recipe([Step("field_probe", params={"field": name})], name="named", size=None, regions=FIELD_REGIONS)
    named = facade.render(photo, recipe, maps=maps)
    direct = registry.apply("field_probe", photo, field=the_field(name, photo, maps))
    np.testing.assert_allclose(named, direct, atol=1e-6, rtol=0)
    assert np.abs(named - photo).max() > 0.01  # the field darkened something: this is not a comparison of two copies


def test_a_field_parameter_is_not_a_number_of_the_recipe_and_cannot_be_driven(field_probe):
    recipe = Recipe([Step("field_probe", params={"field": "light", "gain": 2.0})], name="plain", size=None)
    assert "field_probe.field" not in recipe.parameters()
    assert "field_probe.gain" in recipe.parameters()
    with pytest.raises(RecipeError, match="field"):
        Step("field_probe", params={"field": Drive(by="light", start=0, end=1)})


def test_a_field_written_in_a_recipe_file_is_the_field_of_the_step(field_probe, photo, subject, tmp_path):
    file = tmp_path / "lit.toml"
    file.write_text(
        'format = 3\nname = "lit"\nsize = 128\n[[step]]\neffect = "field_probe"\nfield = "light"\n', encoding="utf-8"
    )
    direct = registry.apply("field_probe", photo, field=the_field("light", photo, field_maps(photo, subject)))
    np.testing.assert_allclose(facade.render(photo, load_recipe(file)), direct, atol=1e-6, rtol=0)


# --- one convention for directions and lengths -------------------------------------------------------------------
# A field is a direction (cyclic degrees, 0 to 360, clockwise from the right), a rotation amount (cyclic, signed
# -180 to 180, clockwise when positive), a length (px, in design pixels) or a share of something named; the tables
# below hold every field that breaks the convention by its name, each with the reason. They hold for the parameters of
# every effect and for the models of the recipe's regions and its frame, by owner: the effect's name, or the model's
# class name (`Line`, `Edge`, `Framing`...).
MODELS: dict[str, type[EffectParams]] = {
    **{name: registry.parameters(name) for name in ALL},
    **{cls.__name__: cls for cls in (*typing.get_args(Kind), Line, Curve, Edge, Framing)},
}

# Degrees fields that are no direction and no rotation: the reason each is none.
NOT_A_DIRECTION = {
    ("disperse", "angle_spread"): "how far the wind's direction may differ, an amount",
    ("dust_scratches", "angle"): "the most a scratch leans from vertical, a lean",
    ("flow_lines", "twist"): "how far the flow turns per unit of noise, an amount",
    ("flow_warp", "variation"): "how far the flow's direction may differ, an amount",
    ("relight", "elevation"): "the light's height above the plane, an elevation",
    ("ripple", "phase"): "where the wave starts, a phase",
    ("selective_color", "feather"): "a width on the hue wheel",
    ("selective_color", "hue"): "a position on the hue wheel",
    ("selective_color", "hue_shift"): "a turn of the hue wheel",
    ("selective_color", "width"): "a width on the hue wheel",
    ("shatter", "angle"): "the most a piece is turned, an amount",
    ("swirl", "angle"): "how far the centre is turned, an amount of several turns",
}
# Fields named `angle` that are how far something is turned, not a direction: the reason each is a rotation.
ROTATIONS = {
    ("Framing", "angle"): "how far the photo is turned before the steps, as perspective.rotate turns the picture",
}
ROTATION_WORDS = {"turn", "rotate", "rotation"}  # a field named so is how far something is turned, not a direction
DIRECTION_WORDS = {"angle", "direction"}


def _degree_problems(name: str, model: type[EffectParams]) -> list[str]:
    """What is wrong with each degrees field of `model`, owned by `name`: a direction is cyclic 0..360 and a rotation
    cyclic -180..180, each with a doc that says clockwise; any other field must be on `NOT_A_DIRECTION`."""
    problems = []
    for doc in model.fields_doc():
        words = set(doc.name.split("_"))
        if doc.unit != "degrees" or (name, doc.name) in NOT_A_DIRECTION:
            continue
        where = f"{name}.{doc.name}"
        if words & ROTATION_WORDS or (name, doc.name) in ROTATIONS:
            if not doc.cyclic or (doc.minimum, doc.maximum) != (-180, 180):
                problems.append(f"{where} is a rotation: cyclic, -180 to 180, not {doc.minimum} to {doc.maximum}")
            if not re.search(r"(?<![a-z-])clockwise", doc.doc, re.I):
                problems.append(f"{where} is a rotation: its doc must say it runs clockwise")
        elif words & DIRECTION_WORDS:
            if not doc.cyclic or (doc.minimum, doc.maximum) != (0, 360):
                problems.append(f"{where} is a direction: cyclic, 0 to 360, not {doc.minimum} to {doc.maximum}")
            if not re.search(r"(?<![a-z-])clockwise", doc.doc, re.I):
                problems.append(f"{where} is a direction: its doc must say it runs clockwise")
        else:
            problems.append(f"{where} is in degrees but no direction or rotation: name it so or list it, with a reason")
    return problems


@pytest.mark.parametrize("name", MODELS)
def test_every_direction_is_cyclic_degrees_clockwise_from_the_right_and_every_rotation_signed(name):
    assert _degree_problems(name, MODELS[name]) == []


def test_the_direction_tables_name_only_degrees_fields_that_exist():
    known = {(name, doc.name) for name, model in MODELS.items() for doc in model.fields_doc() if doc.unit == "degrees"}
    assert (set(NOT_A_DIRECTION) | set(ROTATIONS)) - known == set()


class _Heading(EffectParams):
    """A direction as the convention wants it."""

    angle: float = param(0.0, unit="degrees", min=0, max=360, cyclic=True, doc="Clockwise from pointing right.")


class _HeadingSigned(EffectParams):
    """A direction over the old signed range."""

    angle: float = param(0.0, unit="degrees", min=-180, max=180, cyclic=True, doc="Clockwise from pointing right.")


class _HeadingNotCyclic(EffectParams):
    """A direction that does not wrap."""

    angle: float = param(0.0, unit="degrees", min=0, max=360, doc="Clockwise from pointing right.")


class _HeadingAnticlockwise(EffectParams):
    """A direction that runs the other way."""

    light_angle: float = param(0.0, unit="degrees", min=0, max=360, cyclic=True, doc="Anticlockwise from the right.")


class _HeadingNamedDirection(EffectParams):
    """A direction under another name, over a narrower range."""

    direction: float = param(0.0, unit="degrees", min=0, max=90, doc="Clockwise from the right.")


class _Turn(EffectParams):
    """A rotation as the convention wants it."""

    turn: float = param(0.0, unit="degrees", min=-180, max=180, cyclic=True, doc="Clockwise when positive.")


class _TurnAnticlockwise(EffectParams):
    """A rotation that runs the other way."""

    turn: float = param(0.0, unit="degrees", min=-180, max=180, cyclic=True, doc="Counter-clockwise when positive.")


class _TurnUnsigned(EffectParams):
    """A rotation over a turn from zero."""

    rotate: float = param(0.0, unit="degrees", min=0, max=360, cyclic=True, doc="Clockwise when positive.")


class _Lean(EffectParams):
    """A degrees field that is neither."""

    lean: float = param(0.0, unit="degrees", min=0, max=45, doc="How far it leans.")


DEGREE_PROBES: dict[str, tuple[type[EffectParams], str | None]] = {
    "a direction, cyclic 0..360, clockwise": (_Heading, None),
    "a rotation, cyclic -180..180": (_Turn, None),
    "a direction over -180..180": (_HeadingSigned, "angle is a direction"),
    "a direction that is not cyclic": (_HeadingNotCyclic, "angle is a direction"),
    "a direction whose doc says anticlockwise": (_HeadingAnticlockwise, "must say it runs clockwise"),
    "a direction named `direction` that is narrow and not cyclic": (_HeadingNamedDirection, "direction is a direction"),
    "a rotation over 0..360": (_TurnUnsigned, "rotate is a rotation"),
    "a rotation whose doc says counter-clockwise": (_TurnAnticlockwise, "must say it runs clockwise"),
    "an unlisted degrees field": (_Lean, "lean is in degrees"),
}


@pytest.mark.parametrize(("params", "wanted"), DEGREE_PROBES.values(), ids=DEGREE_PROBES.keys())
def test_the_direction_check_flags_what_breaks_the_convention_and_passes_what_keeps_it(params, wanted):
    problems = _degree_problems("probe", params)
    assert len(problems) == (wanted is not None)
    assert all(wanted in problem for problem in problems if wanted)


# Fields named for a length whose unit is a share, each with what the share is of. A share of the picture's long side
# is never right, since that is a length, in px.
LENGTHS_AS_SHARES = {
    ("vignette", "radius"): "the distance from the centre to a corner",
    ("vignette", "feather"): "the distance from the centre to a corner",
    ("gradient", "radius"): "the picture's shorter side",
    ("swirl", "radius"): "the picture's shorter side",
    ("pinch_bulge", "radius"): "the picture's shorter side",
    ("reflection", "height"): "the subject's height",
    ("light_rays", "length"): "each pixel's distance from the light",
    ("tile", "offset_x"): "one copy's width",
    ("tile", "offset_y"): "one copy's height",
    ("pattern", "thickness"): "one `cell`",
    ("pattern", "radius"): "one `cell`",
    ("pixel_grid", "gap"): "a stripe's width, a third of `cell`",
    ("scanlines", "thickness"): "`spacing`",
    ("focus_blur", "width"): "the field's range of values",
    ("focus_blur", "feather"): "the field's range of values",
    ("fog", "feather"): "the range of the noise that makes the clouds",
    ("disperse", "flare_span"): "a piece's flight",
    ("disperse", "reach"): "the figure, in the order its pieces leave",
    ("RadialGradient", "radius"): "the picture's shorter side",
    ("Band", "feather"): "the field's range of values",
    ("Framing", "margin"): "the longer side of the subject's box",
    ("Framing", "corner_radius"): "the picture's shorter side",
}
LENGTH_WORDS = {
    "length", "width", "size", "offset", "height", "spacing", "radius", "span", "gap", "cell", "thickness",
    "distance", "reach", "margin", "feather", "wavelength", "amplitude",
}  # fmt: skip
POSITIONS = {"x", "y", "at", "base"}  # a position is a fraction of each axis, not a length


def _length_shares(name: str, model: type[EffectParams]) -> list[tuple[str, str]]:
    """(owner, field) for each field of `model`, owned by `name`, that is a share of something and either is named for
    a length (the last word of its name, leaving out a trailing x, y, min or max, is a length word) or says in its doc
    that it is a share of the long side (a distance called `amount`, say)."""
    found = []
    for doc in model.fields_doc():
        words = [word for word in doc.name.split("_") if word not in {"x", "y", "min", "max"}] or [doc.name]
        named = words[-1] in LENGTH_WORDS and doc.name not in POSITIONS
        if doc.unit == "fraction" and (named or "long side" in doc.doc):
            found.append((name, doc.name))
    return found


def _length_problems(name: str, model: type[EffectParams]) -> list[str]:
    return [
        f"{where[0]}.{where[1]} is a length in a share: give it in px, or list it in LENGTHS_AS_SHARES"
        for where in _length_shares(name, model)
        if where not in LENGTHS_AS_SHARES
    ]


@pytest.mark.parametrize("name", MODELS)
def test_every_length_is_in_px_unless_it_is_a_share_of_a_named_reference(name):
    assert _length_problems(name, MODELS[name]) == []


def test_the_length_table_names_only_fields_that_are_still_lengths_in_shares():
    still = {where for name, model in MODELS.items() for where in _length_shares(name, model)}
    assert set(LENGTHS_AS_SHARES) - still == set()


class _WidthInPx(EffectParams):
    """A width in design pixels."""

    width: float = param(4.0, unit="px", min=0, max=100, doc="How wide, in px.")
    x: float = param(0.5, unit="fraction", min=0, max=1, doc="Where, as a fraction of the way across.")


class _WidthInShare(EffectParams):
    """A width as a share of the picture."""

    width: float = param(0.1, unit="fraction", min=0, max=1, doc="How wide, as a fraction of the width.")


class _ReachInShare(EffectParams):
    """A reach as a share of the picture."""

    band_min_reach: float = param(0.1, unit="fraction", min=0, max=1, doc="How far it goes, as a share.")


class _AmountOfLongSide(EffectParams):
    """A distance that is called an amount."""

    amount: float = param(0.02, unit="fraction", min=0, max=0.1, doc="How far, as a fraction of the long side.")


class _AmountOfStrength(EffectParams):
    """An amount that is a strength."""

    amount: float = param(0.5, unit="fraction", min=0, max=1, doc="How much of the effect to show.")


LENGTH_PROBES: dict[str, tuple[type[EffectParams], int]] = {
    "a width in px beside a position as a share": (_WidthInPx, 0),
    "an amount that is a strength": (_AmountOfStrength, 0),
    "a width as a share of the picture": (_WidthInShare, 1),
    "a reach as a share, with min in its name": (_ReachInShare, 1),
    "an amount that is a share of the long side": (_AmountOfLongSide, 1),
}


@pytest.mark.parametrize(("params", "problems"), LENGTH_PROBES.values(), ids=LENGTH_PROBES.keys())
def test_the_length_check_flags_a_length_in_a_share_and_passes_px_positions_and_strengths(params, problems):
    assert len(_length_problems("probe", params)) == problems


# --- a Python effect: the one registration path left, through the public plugin interface ---------------------------


class _BrightenParams(EffectParams):
    """A gain."""

    gain: float = param(0.5, unit="ratio", min=0, max=1, doc="How much of the remaining light is added.")


def test_a_python_effect_registers_runs_through_a_recipe_and_is_refused_when_it_breaks_the_contract(
    throwaway_effects, photo
):
    @effect(category="light", params=_BrightenParams)
    def brighten(rgb, params):
        """Adds light."""
        return rgb + (1 - rgb) * np.float32(params.gain)

    @effect(category="light", params=_BrightenParams)
    def broken(rgb, params):
        """Returns float64, which the contract forbids."""
        return rgb.astype(np.float64)

    out = facade.render(photo, Recipe([Step("brighten", params={"gain": 1.0})], name="lit", size=None))
    np.testing.assert_allclose(out, 1.0, atol=1e-6)
    assert "brighten" in registry.describe()
    with pytest.raises(ValueError, match="broken"):
        facade.render(photo, Recipe([Step("broken")], name="bad", size=None))
