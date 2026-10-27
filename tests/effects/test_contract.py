"""Contract tests every effect must pass, whatever it does: shape, range, determinism, documentation, parameter names,
categories, every parameter across its documented range, and a time and memory budget on a real-sized photo."""

import itertools
import re
import time
import tracemalloc
import typing
from typing import Any, Literal

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image import fx
from bullterrier_image.core import registry, runner
from bullterrier_image.core.params import UNITS, Drive, EffectParams, FieldName, param
from bullterrier_image.core.regions import Kind
from bullterrier_image.core.registry import effect
from bullterrier_image.core.steps import Each, Recipe, Step
from bullterrier_image.errors import DepthNotFoundError, MapNotFoundError, RecipeError, SubjectNotFoundError
from bullterrier_image.loader import load_recipe
from bullterrier_image.recipe import Band, Finder, Given, Grid

ALL = sorted(registry.describe())


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
    inputs = _inputs(name, photo)
    out = registry.apply(name, photo, **{**step.params, **inputs})  # a preset names a field; the array replaces it
    assert_a_picture_like(out, photo)
    assert not np.array_equal(out, registry.apply(name, photo, **inputs))


# Sizes no cell, tile or block divides: a single pixel, a single row or column, and two odd rectangles.
ODD_SIZES = [(1, 1), (1, 7), (7, 1), (13, 29), (61, 37)]


@pytest.mark.parametrize("size", ODD_SIZES, ids=[f"{h}x{w}" for h, w in ODD_SIZES])
@pytest.mark.parametrize("name", ALL)
def test_every_effect_handles_any_picture_size_and_leaves_its_input_alone(name, size, photo):
    rgb = np.ascontiguousarray(photo[: size[0], : size[1]])
    before = rgb.copy()
    assert_a_picture_like(registry.apply(name, rgb, **_inputs(name, rgb)), rgb)
    np.testing.assert_array_equal(rgb, before)


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
    fields = dict.fromkeys(registry.parameters(name).field_name_fields(), made["depth"])
    return {**fields, **{key: value for key, value in made.items() if key in registry.inputs(name)}}


def _bounds() -> list[tuple[str, str, str]]:
    """(effect, numeric field, "min" or "max") for every numeric field of every effect."""
    found = []
    for name in ALL:
        model = registry.parameters(name)
        found += [
            (name, doc.name, side) for doc in model.fields_doc() if doc.minimum is not None for side in ("min", "max")
        ]
    return found


def _choices() -> list[tuple[str, str, str]]:
    """(effect, choice field, choice) for every choice of every `Literal` field of every effect."""
    return [
        (name, field, choice)
        for name in ALL
        for field, prop in registry.parameters(name).model_json_schema()["properties"].items()
        for choice in prop.get("enum", ())
    ]


def _bound_value(name: str, field: str, side: str, beyond: bool = False) -> float:
    model = registry.parameters(name)
    doc = next(d for d in model.fields_doc() if d.name == field)
    bound = doc.minimum if side == "min" else doc.maximum
    assert bound is not None
    if isinstance(doc.default, int):
        return int(bound) + ((-1 if side == "min" else 1) if beyond else 0)
    # the very next float: a limit that is off by any amount lets it through
    return float(np.nextafter(bound, -np.inf if side == "min" else np.inf)) if beyond else float(bound)


def _setting(name: str, field: str, value: float) -> dict[str, float]:
    """`field` set to `value`. When the field is one side of a min/max pair (`band_min_height` and `band_max_height`:
    the same name with `min` and `max` swapped), the other side moves to the same value if its default would leave
    the pair the wrong way round, so each side still reaches its whole range."""
    fields = registry.parameters(name).model_fields
    words = field.split("_")
    for side, other_side in (("min", "max"), ("max", "min")):
        other = "_".join(other_side if word == side else word for word in words)
        if side in words and other in fields:
            partner = fields[other].default
            if (value > partner) if side == "min" else (value < partner):
                return {field: value, other: value}
    return {field: value}


@pytest.mark.parametrize("name", ALL)
def test_every_parameter_is_described_and_has_a_unit_entry(name):
    schema = registry.parameters(name).model_json_schema()
    for field, prop in schema["properties"].items():
        assert prop.get("description", "").strip(), f"{name}.{field} has no description"
        assert "unit" in prop, f"{name}.{field} has no unit entry"
        if prop.get("type") in ("number", "integer"):
            assert prop["unit"] in UNITS, f"{name}.{field}: unit {prop['unit']!r}"


@pytest.mark.parametrize(("name", "field", "side"), _bounds())
def test_every_effect_works_with_each_number_at_its_documented_limit(name, field, side, photo):
    small = _small(photo)
    out = registry.apply(name, small, **_setting(name, field, _bound_value(name, field, side)), **_inputs(name, small))
    assert_a_picture_like(out, small)


def _interior_values() -> list[tuple[str, str, float]]:
    """(effect, numeric field, value) for five values evenly inside the range of every int or float field of every
    effect (ints rounded, repeats dropped): the values a caller may set."""
    found = []
    for name in ALL:
        model = registry.parameters(name)
        for doc in model.fields_doc():
            if doc.minimum is None or doc.maximum is None or not isinstance(doc.default, int | float):
                continue  # not a number
            values = [doc.minimum + (doc.maximum - doc.minimum) * k / 6 for k in range(1, 6)]
            if isinstance(doc.default, int):
                values = sorted({round(value) for value in values})
            found += [(name, doc.name, value) for value in values]
    return found


@pytest.mark.parametrize(("name", "field", "value"), _interior_values())
def test_every_effect_works_with_each_number_anywhere_inside_its_range(name, field, value, photo):
    small = _small(photo)
    assert_a_picture_like(registry.apply(name, small, **_setting(name, field, value), **_inputs(name, small)), small)


def _cyclic() -> list[tuple[str, str]]:
    """(effect, field) for every cyclic angle of every effect."""
    return [(name, doc.name) for name in ALL for doc in registry.parameters(name).fields_doc() if doc.cyclic]


@pytest.mark.parametrize(
    ("name", "field", "side"),
    [bound for bound in _bounds() if bound[:2] not in _cyclic()],
    ids=lambda value: str(value),
)
def test_every_number_beyond_its_documented_limit_is_refused(name, field, side, photo):
    small = _small(photo)
    with pytest.raises(ValueError, match=f"`{field}`"):
        registry.apply(name, small, **{field: _bound_value(name, field, side, beyond=True)}, **_inputs(name, small))


@pytest.mark.parametrize(("name", "field", "choice"), _choices())
def test_every_effect_works_with_each_choice_it_offers(name, field, choice, photo):
    small = _small(photo)
    assert_a_picture_like(registry.apply(name, small, **{field: choice}, **_inputs(name, small)), small)


@pytest.mark.parametrize(("name", "field"), sorted({(name, field) for name, field, _ in _choices()}))
def test_a_choice_that_is_not_offered_is_refused(name, field, photo):
    small = _small(photo)
    with pytest.raises(ValueError, match=f"`{field}`"):
        registry.apply(name, small, **{field: "not_a_choice"}, **_inputs(name, small))


@pytest.mark.parametrize("name", ALL)
def test_every_effect_refuses_an_unknown_parameter(name, photo):
    small = _small(photo)
    with pytest.raises(ValueError, match="not_a_parameter"):
        registry.apply(name, small, not_a_parameter=1, **_inputs(name, small))


@pytest.mark.parametrize("name", ALL)
def test_every_effect_takes_its_model_its_keywords_or_nothing(name, photo):
    small, model = _small(photo), registry.parameters(name)
    inputs = _inputs(name, small)
    defaults = {field: info.default for field, info in model.model_fields.items() if field not in inputs}
    by_model = registry.get(name)(small, model(), **inputs)
    np.testing.assert_array_equal(registry.get(name)(small, **inputs), by_model)
    np.testing.assert_array_equal(registry.get(name)(small, **defaults, **inputs), by_model)


# --- what holds for every effect that has the setting ----------------------------------------------------------------


# Effects whose `amount` of 0 is the identity only together with another setting.
AMOUNT_ZERO_NEEDS = {"tone_map": {"contrast": 1.0}}  # its `amount` is the detail on top of the contrast curve


def _amount_zero_cases() -> list[tuple[str, dict[str, object]]]:
    """(effect, settings) for every effect whose `amount` can be 0: its defaults, then each choice of each `Literal`
    field (a sampling or edge rule must not change a picture nothing moves in)."""
    found: list[tuple[str, dict[str, object]]] = []
    for name in ALL:
        if not any(doc.name == "amount" and doc.minimum == 0 for doc in registry.parameters(name).fields_doc()):
            continue
        found += [(name, {})] + [(name, {field: choice}) for n, field, choice in _choices() if n == name]
    return found


@pytest.mark.parametrize(("name", "setting"), _amount_zero_cases(), ids=lambda case: str(case) if case else "defaults")
def test_an_amount_of_zero_returns_the_picture_as_it_is_in_a_new_array(name, setting, photo):
    out = registry.apply(name, photo, amount=0.0, **AMOUNT_ZERO_NEEDS.get(name, {}), **setting, **_inputs(name, photo))
    np.testing.assert_allclose(out, photo, atol=1e-6)
    assert not np.shares_memory(out, photo)  # a new picture, not the input handed back


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


def _sampling_cases() -> list[tuple[str, dict[str, str]]]:
    """(effect, choices) for every combination of the choice fields (`mode`, `edge`, `interpolation`...) of the
    effects that read the picture between its pixels (those with an `interpolation` field)."""
    found = []
    for name in ALL:
        properties = registry.parameters(name).model_json_schema()["properties"]
        if "interpolation" in properties:
            choices = {field: prop["enum"] for field, prop in properties.items() if "enum" in prop}
            found += [
                (name, dict(zip(choices, picked, strict=True))) for picked in itertools.product(*choices.values())
            ]
    return found


@pytest.mark.parametrize(
    ("name", "setting"), _sampling_cases(), ids=lambda case: "-".join(case.values()) if isinstance(case, dict) else case
)
def test_a_constant_picture_stays_constant_under_every_way_of_reading_between_pixels(name, setting):
    flat = np.full((31, 47, 3), 0.4, np.float32)
    out = registry.apply(name, flat, **setting, **_inputs(name, flat))
    np.testing.assert_allclose(out, 0.4, atol=1e-6)


@pytest.mark.parametrize("name", sorted({name for name, _ in _sampling_cases()}))
def test_each_interpolation_reads_the_picture_its_own_way(name):
    noise = np.random.default_rng(4).random((41, 41, 3), dtype=np.float32)
    choices = registry.parameters(name).model_json_schema()["properties"]["interpolation"]["enum"]
    outs = {choice: registry.apply(name, noise, interpolation=choice, **_inputs(name, noise)) for choice in choices}
    for first, second in itertools.combinations(choices, 2):
        assert not np.array_equal(outs[first], outs[second]), f"{first} and {second} give the same picture"


SEED_NEEDS: dict[str, dict[str, float]] = {  # at their defaults nothing is random yet, whatever the seed
    "disperse": {"progress": 0.5},
    "jpeg_crush": {"hits": 40},
    "starburst": {"spacing": 2},
}


def _assert_the_seed_decides_the_picture(name: str, photo: np.ndarray) -> None:
    settings = SEED_NEEDS.get(name, {})
    first = registry.apply(name, photo, seed=1, **settings)
    np.testing.assert_array_equal(first, registry.apply(name, photo, seed=1, **settings))
    assert (first != registry.apply(name, photo, seed=2, **settings)).any()


@pytest.mark.parametrize("name", [name for name in ALL if "seed" in registry.parameters(name).model_fields])
def test_the_same_seed_gives_the_same_picture_and_another_seed_another(name, photo):
    _assert_the_seed_decides_the_picture(name, photo)


class _SeedProbeParams(EffectParams):
    """A seed and nothing else."""

    seed: int = param(0, unit="count", min=0, max=9, doc="Ignored by the probes.")


@pytest.mark.parametrize("probe", ["ignores_its_seed", "is_unseeded"])
def test_the_seed_check_flags_an_effect_that_ignores_its_seed_or_is_not_deterministic(probe, photo, throwaway_effects):
    @effect(category="utilities", params=_SeedProbeParams)
    def ignores_its_seed(rgb, params):
        """Never random."""
        return rgb

    @effect(category="utilities", params=_SeedProbeParams)
    def is_unseeded(rgb, params):
        """Random, but not from its seed."""
        return rgb + np.random.default_rng().random(rgb.shape, dtype=np.float32) * 0.01

    with pytest.raises(AssertionError):
        _assert_the_seed_decides_the_picture(probe, photo)


# --- the effects that scatter particles of light -----------------------------------------------------------------

# effect: the settings that make it draw many white particles
PARTICLES: dict[str, dict[str, Any]] = {
    "particles": {"density": 3.0, "color": "#ffffff"},
}
TINTED = ["particles"]


@pytest.mark.parametrize(
    ("name", "none"),
    [
        pytest.param("particles", {"density": 0.0}, id="particles"),
        pytest.param("particles", {"density": 0.0, "color": "#e6eefa"}, id="particles, tinted"),
        pytest.param("lens_drops", {"count": 0}, id="lens_drops"),
    ],
)
def test_no_particles_leave_the_picture_unchanged_in_a_new_array(name, none, photo):
    out = registry.apply(name, photo, **none)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)


@pytest.mark.parametrize("name", PARTICLES)
def test_particles_are_screened_over_the_picture_so_they_only_lighten_it(name, photo):
    """Screen over mid grey 0.5 with light l gives 1 - (1 - 0.5)(1 - l) = 0.5 + 0.5 l, and over white stays white."""
    settings = {**PARTICLES[name], "seed": 3}
    on_black = registry.apply(name, np.zeros_like(photo), **settings)[..., 0]
    assert on_black.max() > 0.4  # there is light to screen
    grey = registry.apply(name, np.full_like(photo, 0.5), **settings)
    np.testing.assert_allclose(grey[..., 0], 0.5 + 0.5 * on_black, atol=1e-6)
    np.testing.assert_array_equal(registry.apply(name, np.ones_like(photo), **settings), 1.0)
    assert (registry.apply(name, photo, **settings) >= photo).all()


@pytest.mark.parametrize("shape", [(60, 240), (240, 60)], ids=["wide", "tall"])
@pytest.mark.parametrize("name", PARTICLES)
def test_particles_are_spread_over_the_whole_of_a_wide_or_a_tall_picture(name, shape):
    lit = registry.apply(name, np.zeros((*shape, 3), np.float32), **PARTICLES[name], seed=1)[..., 0] > 0
    for axis in (0, 1):  # the picture's rows, then its columns: every third of them holds some particles
        thirds = np.array_split(lit.any(axis=1 - axis), 3)
        assert all(third.any() for third in thirds)


@pytest.mark.parametrize("name", TINTED)
def test_the_color_tints_the_light(name, photo):
    warm = registry.apply(name, np.zeros_like(photo), **{**PARTICLES[name], "color": "#ff8000"}, seed=1)
    assert warm[..., 0].max() > 0.5
    np.testing.assert_allclose(warm[..., 1], warm[..., 0] * 128 / 255, atol=0.003)  # green is 0x80 of red's 0xff
    assert warm[..., 2].max() == 0


# --- cost: time and memory on a real-sized photo -----------------------------------------------------------------

CPU_BUDGET = 3.0  # seconds of process CPU time per effect at 1 MP: about 10x the slowest measured (see test below)
MEMORY_BUDGET = 12  # picture-sized buffers of working memory held at once; the most measured is 7.3 (gradient_map)


@pytest.fixture(scope="module")
def megapixel_photo() -> np.ndarray:
    """A 1 MP picture (866 x 1155): gradients plus noise, like the small `photo` fixture but the size of a phone shot
    shrunk for the web."""
    rng = np.random.default_rng(1)
    y, x = np.mgrid[0:866, 0:1155].astype(np.float32)
    ramp = np.stack([x / 1154, y / 865, (x + y) / 2019], axis=-1)
    return (ramp * 0.8 + rng.random((866, 1155, 3), dtype=np.float32) * 0.2).astype(np.float32)


def _over_budget(name: str, rgb: np.ndarray) -> list[str]:
    """What `name` at its defaults spends on `rgb` beyond the budgets: process CPU time (not wall time, so other work
    on the machine barely matters) and the peak of memory allocated during the call (numpy reports its arrays to
    `tracemalloc`), in picture-sized buffers."""
    inputs = _inputs(name, rgb)
    tracemalloc.start()
    try:
        held = tracemalloc.get_traced_memory()[0]
        start = time.process_time()
        registry.apply(name, rgb, **inputs)
        cpu = time.process_time() - start
        buffers = (tracemalloc.get_traced_memory()[1] - held) / rgb.nbytes
    finally:
        tracemalloc.stop()
    problems = [f"{name}: {cpu:.2f} s of CPU time, budget {CPU_BUDGET} s"] if cpu > CPU_BUDGET else []
    if buffers > MEMORY_BUDGET:
        problems.append(f"{name}: {buffers:.1f} picture-sized buffers of memory, budget {MEMORY_BUDGET}")
    return problems


@pytest.mark.parametrize("name", ALL)
def test_every_effect_at_its_defaults_stays_within_the_time_and_memory_budget_on_a_1_mp_photo(name, megapixel_photo):
    """Measured on an Apple M4: the slowest effect (`flow_warp`) takes 0.35 s of CPU time, the hungriest holds 7.3
    picture-sized buffers; a shared GitHub runner is about 3.5x slower. The budgets leave room for a slower machine,
    never for a 10x regression."""
    assert _over_budget(name, megapixel_photo) == []


class _CostProbeParams(EffectParams):
    """No settings that matter."""

    amount: float = param(0.5, unit="fraction", min=0, max=1, doc="Unused.")


def test_the_budget_flags_an_effect_that_spends_more_cpu_time_than_allowed(throwaway_effects, megapixel_photo):
    @effect(category="utilities", params=_CostProbeParams)
    def slow_probe(rgb, params):
        """Busy for longer than the budget."""
        start = time.process_time()
        while time.process_time() - start <= CPU_BUDGET:
            pass
        return rgb

    problems = _over_budget("slow_probe", megapixel_photo)
    assert len(problems) == 1
    assert "CPU time" in problems[0]


def test_the_budget_flags_an_effect_that_holds_more_memory_than_allowed(throwaway_effects, megapixel_photo):
    @effect(category="utilities", params=_CostProbeParams)
    def hungry_probe(rgb, params):
        """Holds one more picture-sized buffer than the budget at once."""
        copies = [rgb.copy() for _ in range(MEMORY_BUDGET + 1)]
        return copies[-1]

    problems = _over_budget("hungry_probe", megapixel_photo)
    assert len(problems) == 1
    assert "buffers of memory" in problems[0]


# --- every drivable parameter: a field may vary it pixel by pixel, the effect itself unchanged ------------------------

DRIVABLE = sorted((name, field) for name in ALL for field in registry.parameters(name).drivable_fields())


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
        "driven",
        [Step(name, {field: Drive(by="f", start=least, end=greatest, levels=levels)})],
        size=None,
        regions={"f": Given()},
    )

    def plain(value: float) -> Recipe:
        return Recipe("plain", [Step(name, {field: value})], size=None)

    return driven, plain, _inputs(name, photo)


def test_some_parameters_are_drivable():
    assert DRIVABLE  # else every test below is parametrized over nothing and passes without testing anything


@pytest.mark.parametrize(("name", "field"), DRIVABLE)
def test_a_drivable_field_says_so_in_the_documentation_and_the_signature(name, field):
    assert f"{field} : number (drivable)" in registry.documentation(name)
    assert Drive in typing.get_args(registry.parameters(name).signature().parameters[field].annotation)


@pytest.mark.parametrize("level", [0, 2, 4])
@pytest.mark.parametrize(("name", "field"), DRIVABLE)
def test_a_constant_field_gives_the_plain_effect_at_that_levels_value(name, field, level, photo):
    least, greatest = _range(name, field)
    driven, plain, maps = _driven_and_plain(name, field, 5, photo)
    expected = runner.render(photo, plain(least + (greatest - least) * level / 4), maps=maps)
    got = runner.render(photo, driven, maps={**maps, "f": np.full(photo.shape[:2], level / 4, np.float32)})
    np.testing.assert_allclose(got, expected, atol=1e-6, rtol=0)


@pytest.mark.parametrize(("name", "field"), DRIVABLE)
def test_two_levels_mix_the_effect_at_the_least_and_the_greatest_value_by_the_field(name, field, photo):
    least, greatest = _range(name, field)
    driven, plain, maps = _driven_and_plain(name, field, 2, photo)
    ramp = np.tile(np.linspace(0, 1, photo.shape[1], dtype=np.float32), (photo.shape[0], 1))
    at_least, at_greatest = (runner.render(photo, plain(value), maps=maps) for value in (least, greatest))
    got = runner.render(photo, driven, maps={**maps, "f": ramp})
    np.testing.assert_allclose(got, (1 - ramp)[..., None] * at_least + ramp[..., None] * at_greatest, atol=1e-6, rtol=0)


@pytest.mark.parametrize(("name", "field"), [drivable for drivable in DRIVABLE if drivable not in _cyclic()])
def test_a_drive_that_leaves_the_parameters_range_is_refused_naming_the_field(name, field):
    least, greatest = _range(name, field)
    for start, end in ((least - 1, greatest), (least, greatest + 1)):
        with pytest.raises(RecipeError, match=rf"{name}.*`{field}`"):
            Step(name, {field: Drive(by="f", start=start, end=end)})


@pytest.mark.parametrize(("name", "field"), DRIVABLE)
def test_fx_builds_a_driven_step_from_a_drive(name, field):
    least, greatest = _range(name, field)
    step = getattr(fx, name)(**{field: Drive(by="depth", start=least, end=greatest)})
    assert step.drives == {field: Drive(by="depth", start=least, end=greatest)}


@pytest.mark.parametrize("name", [name for name in ALL if name not in {n for n, _ in DRIVABLE}])
def test_an_effect_with_no_drivable_parameter_refuses_a_drive_for_every_number_it_has(name):
    fields = registry.parameters(name)
    numbers = [
        doc.name for doc in fields.fields_doc() if doc.minimum is not None and doc.name not in fields.drivable_fields()
    ]
    for field in numbers:
        with pytest.raises(RecipeError, match=rf"{name}.*`{field}` cannot be driven"):
            Step(name, {field: Drive(by="f", start=_range(name, field)[0], end=_range(name, field)[1])})


# What an effect needs for its angle to show: at its defaults it changes nothing, so any angle would look cyclic.
CYCLIC_NEEDS = {
    ("disperse", "angle"): {"progress": 0.5},
    ("particles", "angle"): {"length": 40.0},
    ("selective_color", "hue"): {"hue_shift": 90.0, "width": 120.0},
}

# The share of pixels allowed to differ a turn apart: a hard-edged screen flips the dots that sit on an exact cell edge.
EDGE_FLIPS = {("halftone", "angle"): 0.1}


@pytest.mark.parametrize(("name", "field"), _cyclic())
def test_a_cyclic_angle_gives_the_same_picture_a_turn_further_and_takes_any_real(name, field, photo):
    small = _small(photo)
    doc = next(d for d in registry.parameters(name).fields_doc() if d.name == field)
    low, high = doc.minimum, doc.maximum
    assert low is not None
    assert high is not None
    needs = {**CYCLIC_NEEDS.get((name, field), {})}
    at_low = registry.apply(name, small, **needs, **{field: low}, **_inputs(name, small))
    at_high = registry.apply(name, small, **needs, **{field: high}, **_inputs(name, small))
    at_45 = registry.apply(name, small, **needs, **{field: low + 45}, **_inputs(name, small))
    assert np.abs(at_45 - at_low).max() > 1e-3, f"{name}.{field} shows nothing: the test would pass for any field"
    differing = (np.abs(at_low - at_high).max(axis=-1) > 1e-3).mean()
    assert differing <= EDGE_FLIPS.get((name, field), 0), f"{name}.{field}: min and max are a turn apart"
    # four turns and a degree past the bottom of the turn is the same direction as one degree past it
    beyond = registry.apply(name, small, **needs, **{field: low + 4 * 360 + 1}, **_inputs(name, small))
    inside = registry.apply(name, small, **needs, **{field: low + 1}, **_inputs(name, small))
    np.testing.assert_array_equal(beyond, inside)


@pytest.mark.parametrize(("name", "field"), [drivable for drivable in DRIVABLE if drivable in _cyclic()])
def test_a_drive_of_a_cyclic_angle_may_run_past_the_turn_each_level_being_the_plain_effect_at_its_direction(
    name, field, photo
):
    least, _ = _range(name, field)
    driven = Recipe(
        "driven",
        [Step(name, {field: Drive(by="f", start=least, end=least + 720, levels=5)})],
        size=None,
        regions={"f": Given()},
    )
    maps = _inputs(name, photo)
    for level, written in enumerate([least, least + 180, least + 360, least + 540, least + 720]):
        got = runner.render(photo, driven, maps={**maps, "f": np.full(photo.shape[:2], level / 4, np.float32)})
        plain = Recipe("plain", [Step(name, {field: written})], size=None)
        np.testing.assert_allclose(got, runner.render(photo, plain, maps=maps), atol=1e-6, rtol=0)


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
        "light": photo @ LUMA,
        "g": maps["g"],
        "middle": (maps["depth"] > 0.3) & (maps["depth"] < 0.7),
    }[name].astype(np.float32)


@pytest.mark.parametrize("name", ["depth", "light", "subject", "background", "g", "middle"])
def test_a_field_named_in_a_recipe_gives_the_same_picture_as_the_equal_array_passed_directly(
    name, field_probe, photo, subject
):
    maps = field_maps(photo, subject)
    recipe = Recipe("named", [Step("field_probe", {"field": name})], size=None, regions=FIELD_REGIONS)
    named = runner.render(photo, recipe, maps=maps)
    direct = registry.apply("field_probe", photo, field=the_field(name, photo, maps))
    np.testing.assert_allclose(named, direct, atol=1e-6, rtol=0)
    assert np.abs(named - photo).max() > 0.01  # the field darkened something: this is not a comparison of two copies


FIELD_EFFECTS = [(name, field) for name in ALL for field in registry.parameters(name).field_name_fields()]


@pytest.mark.parametrize(("name", "field"), FIELD_EFFECTS, ids=[f"{n}-{f}" for n, f in FIELD_EFFECTS])
@pytest.mark.parametrize("selection", ["depth", "light", "subject", "background", "g", "middle"])
def test_every_effect_with_a_field_parameter_gives_the_same_picture_for_its_name_and_for_the_equal_array(
    name, field, selection, photo, subject
):
    maps = field_maps(photo, subject)
    recipe = Recipe("named", [Step(name, {field: selection})], size=max(photo.shape[:2]), regions=FIELD_REGIONS)
    named = runner.render(photo, recipe, maps=maps)
    direct = registry.apply(name, photo, **{field: the_field(selection, photo, maps)})
    assert_a_picture_like(named, photo)
    np.testing.assert_allclose(named, direct, atol=1e-6, rtol=0)


def test_some_effects_have_a_field_parameter():
    assert FIELD_EFFECTS  # else the test above is parametrized over nothing and passes without testing anything


def test_a_field_is_made_for_the_picture_the_recipe_works_on_not_for_the_photo(field_probe, photo, subject):
    recipe = Recipe("small", [Step("field_probe", {"field": "g"})], size=48, regions=FIELD_REGIONS)
    assert runner.render(photo, recipe, maps=field_maps(photo, subject)).shape[:2] == (36, 48)


FIELD_PARAMETERS = [(name, field) for name in ALL for field in registry.parameters(name).field_name_fields()]


@pytest.mark.parametrize(("name", "field"), FIELD_PARAMETERS)
# Not `light`: the hand-made brightness differs from the runner's by float32 rounding, which steep contour lines
# (topographic) amplify past any tolerance worth having; the probe test above covers `light`.
@pytest.mark.parametrize("selection", ["depth", "subject", "background", "g", "middle"])
def test_every_effect_with_a_field_parameter_gives_the_same_picture_for_a_named_field_and_the_equal_array(
    name, field, selection, photo, subject
):
    maps = field_maps(photo, subject)
    recipe = Recipe("named", [Step(name, {field: selection})], size=max(photo.shape[:2]), regions=FIELD_REGIONS)
    named = runner.render(photo, recipe, maps=maps)
    direct = registry.apply(name, photo, **{field: the_field(selection, photo, maps)})
    np.testing.assert_allclose(named, direct, atol=1e-6, rtol=0)


@pytest.mark.parametrize(
    ("step_params", "asked"),
    [
        pytest.param({"field": "light"}, {"depth": 0, "subject": 0}, id="light is made from the picture: no finder"),
        pytest.param({"field": "depth"}, {"depth": 1, "subject": 0}, id="depth asks the depth finder once"),
        pytest.param({}, {"depth": 1, "subject": 0}, id="the default field counts like one written"),
        pytest.param({"field": "background"}, {"depth": 0, "subject": 1}, id="background asks for the subject"),
        pytest.param({"field": "middle"}, {"depth": 1, "subject": 0}, id="a band asks for what it is made of"),
    ],
)
def test_the_runner_builds_only_the_fields_a_step_names(step_params, asked, field_probe, photo, subject):
    maps = field_maps(photo, subject)
    calls = {"depth": 0, "subject": 0}

    def stand_in(map_name: str) -> Finder:
        def find(rgb: np.ndarray) -> np.ndarray:
            calls[map_name] += 1
            return maps[map_name]

        return Finder(f"{map_name} stand-in", find)

    recipe = Recipe("only", [Step("field_probe", step_params)], size=None, regions=FIELD_REGIONS)
    runner.render(photo, recipe, finders={"depth": stand_in("depth"), "subject": stand_in("subject")})
    assert calls == asked


@pytest.mark.parametrize(
    ("step_params", "regions", "parts"),
    [
        pytest.param({"field": "nope"}, {}, ["field_probe", "field", "'nope'", "depth"], id="no such region"),
        pytest.param(
            {"field": "grid"}, {"grid": Grid()}, ["field_probe", "field", "'grid'", "selection"], id="partition"
        ),
        pytest.param({"field": ""}, {}, ["field", "''"], id="a blank name"),
    ],
)
def test_a_field_that_is_no_selection_fails_when_the_recipe_is_made_naming_effect_field_and_value(
    step_params, regions, parts, field_probe
):
    with pytest.raises(RecipeError) as raised:
        Recipe("bad", [Step("field_probe", step_params)], size=None, regions=regions)
    for part in parts:
        assert part in str(raised.value)


def test_a_field_parameter_is_not_a_number_of_the_recipe_and_cannot_be_driven(field_probe):
    recipe = Recipe("plain", [Step("field_probe", {"field": "light", "gain": 2.0})], size=None)
    assert "field_probe.field" not in recipe.parameters()
    assert "field_probe.gain" in recipe.parameters()
    with pytest.raises(RecipeError, match="field"):
        Step("field_probe", {"field": Drive(by="light", start=0, end=1)})


@pytest.mark.parametrize("by", ["g", "depth"], ids=["the drive varies by another field", "by the field itself"])
def test_a_field_and_a_drive_on_another_parameter_equal_the_mix_of_two_renders_at_the_drives_ends(
    by, field_probe, photo, subject
):
    maps = field_maps(photo, subject)
    recipe = Recipe(
        "both",
        [Step("field_probe", {"field": "depth", "gain": Drive(by=by, start=0.5, end=2.0, levels=2)})],
        size=None,
        regions={"g": Given()},
    )
    weight = maps[by][..., None]
    at_start = registry.apply("field_probe", photo, field=maps["depth"], gain=0.5)
    at_end = registry.apply("field_probe", photo, field=maps["depth"], gain=2.0)
    expected = (1 - weight) * at_start + weight * at_end
    np.testing.assert_allclose(runner.render(photo, recipe, maps=maps), expected, atol=1e-6, rtol=0)


@pytest.mark.parametrize(
    ("name", "error", "naming"),
    [
        ("depth", DepthNotFoundError, "field_probe"),
        ("subject", SubjectNotFoundError, "subject"),
        ("g", MapNotFoundError, "'g'"),
    ],
    ids=["depth", "subject", "a given map"],
)
def test_a_field_whose_map_nobody_gave_is_a_map_not_found_error_naming_the_map(name, error, naming, field_probe, photo):
    recipe = Recipe("lost", [Step("field_probe", {"field": name})], size=None, regions=FIELD_REGIONS)
    with pytest.raises(error, match=naming):
        runner.render(photo, recipe)


def test_a_field_written_in_a_recipe_file_is_the_field_of_the_step(field_probe, photo, subject, tmp_path):
    file = tmp_path / "lit.toml"
    file.write_text(
        'format = 3\nname = "lit"\nsize = 128\n[[step]]\neffect = "field_probe"\nfield = "light"\n', encoding="utf-8"
    )
    direct = registry.apply("field_probe", photo, field=the_field("light", photo, field_maps(photo, subject)))
    np.testing.assert_allclose(runner.render(photo, load_recipe(file)), direct, atol=1e-6, rtol=0)


def test_the_look_of_an_each_can_work_from_a_field(field_probe, photo, subject):
    maps = field_maps(photo, subject)
    recipe = Recipe(
        "pieces",
        [Each("halves", [Step("field_probe", {"field": "g"}), "none"], feather=0)],
        size=None,
        regions={"halves": Grid(columns=2), "g": Given()},
    )
    got = runner.render(photo, recipe, maps=maps)
    np.testing.assert_allclose(got[:, :64], registry.apply("field_probe", photo, field=maps["g"])[:, :64], atol=1e-6)
    np.testing.assert_array_equal(got[:, 64:], photo[:, 64:])


# --- one convention for directions and lengths -------------------------------------------------------------------
# A field is a direction (cyclic degrees, 0 to 360, clockwise from the right), a rotation amount (cyclic, signed
# -180 to 180), a length (px, in design pixels) or a share of something named; the tables below hold every field that
# breaks the convention by its name, each with the reason.

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
ROTATION_WORDS = {"turn", "rotate", "rotation"}  # a field named so is how far something is turned, not a direction
DIRECTION_WORDS = {"angle", "direction"}


def _degree_problems(name: str) -> list[str]:
    """What is wrong with each degrees field of the effect `name`: a direction is cyclic 0..360 and its doc says
    clockwise, a rotation is cyclic -180..180, any other field must be on `NOT_A_DIRECTION`."""
    problems = []
    for doc in registry.parameters(name).fields_doc():
        words = set(doc.name.split("_"))
        if doc.unit != "degrees" or (name, doc.name) in NOT_A_DIRECTION:
            continue
        where = f"{name}.{doc.name}"
        if words & ROTATION_WORDS:
            if not doc.cyclic or (doc.minimum, doc.maximum) != (-180, 180):
                problems.append(f"{where} is a rotation: cyclic, -180 to 180, not {doc.minimum} to {doc.maximum}")
        elif words & DIRECTION_WORDS:
            if not doc.cyclic or (doc.minimum, doc.maximum) != (0, 360):
                problems.append(f"{where} is a direction: cyclic, 0 to 360, not {doc.minimum} to {doc.maximum}")
            if not re.search(r"(?<![a-z-])clockwise", doc.doc, re.I):
                problems.append(f"{where} is a direction: its doc must say it runs clockwise")
        else:
            problems.append(f"{where} is in degrees but no direction or rotation: name it so or list it, with a reason")
    return problems


@pytest.mark.parametrize("name", ALL)
def test_every_direction_is_cyclic_degrees_clockwise_from_the_right_and_every_rotation_signed(name):
    assert _degree_problems(name) == []


def test_the_direction_table_names_only_degrees_fields_that_exist():
    known = {
        (name, doc.name) for name in ALL for doc in registry.parameters(name).fields_doc() if doc.unit == "degrees"
    }
    assert set(NOT_A_DIRECTION) - known == set()


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
    "an unlisted degrees field": (_Lean, "lean is in degrees"),
}


@pytest.mark.parametrize(("params", "wanted"), DEGREE_PROBES.values(), ids=DEGREE_PROBES.keys())
def test_the_direction_check_flags_what_breaks_the_convention_and_passes_what_keeps_it(
    params, wanted, throwaway_effects
):
    @effect(category="light", params=params)
    def degree_probe(rgb, params):
        """Has one degrees field."""
        return rgb

    problems = _degree_problems("degree_probe")
    assert len(problems) == (wanted is not None)
    assert all(wanted in problem for problem in problems if wanted)


# Fields named for a length whose unit is a share, each with what the share is of. A share of the picture's long side
# is never right, since that is a length, in px; those listed "BUG" are waiting to be converted.
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
    ("blend", "margin"): "BUG: the photo's long side, a length: wants px",
    ("parallax", "amount"): "BUG: the picture's long side, a length: wants px",
}
LENGTH_WORDS = {
    "length", "width", "size", "offset", "height", "spacing", "radius", "span", "gap", "cell", "thickness",
    "distance", "reach", "margin", "feather", "wavelength", "amplitude",
}  # fmt: skip
POSITIONS = {"x", "y", "at", "base"}  # a position is a fraction of each axis, not a length


def _length_shares(name: str) -> list[tuple[str, str]]:
    """(effect, field) for each field of `name` that is a share of something and either is named for a length (the
    last word of its name, leaving out a trailing x, y, min or max, is a length word) or says in its doc that it is a
    share of the long side (a distance called `amount`, say)."""
    found = []
    for doc in registry.parameters(name).fields_doc():
        words = [word for word in doc.name.split("_") if word not in {"x", "y", "min", "max"}] or [doc.name]
        named = words[-1] in LENGTH_WORDS and doc.name not in POSITIONS
        if doc.unit == "fraction" and (named or "long side" in doc.doc):
            found.append((name, doc.name))
    return found


def _length_problems(name: str) -> list[str]:
    return [
        f"{where[0]}.{where[1]} is a length in a share: give it in px, or list it in LENGTHS_AS_SHARES"
        for where in _length_shares(name)
        if where not in LENGTHS_AS_SHARES
    ]


@pytest.mark.parametrize("name", ALL)
def test_every_length_is_in_px_unless_it_is_a_share_of_a_named_reference(name):
    assert _length_problems(name) == []


def test_the_length_table_names_only_fields_that_are_still_lengths_in_shares():
    still = {where for name in ALL for where in _length_shares(name)}
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
def test_the_length_check_flags_a_length_in_a_share_and_passes_px_positions_and_strengths(
    params, problems, throwaway_effects
):
    @effect(category="light", params=params)
    def length_probe(rgb, params):
        """Has one length-like field."""
        return rgb

    assert len(_length_problems("length_probe")) == problems


# --- the same look at the design size and at four times it --------------------------------------------------------

DESIGN_SIZE = 240  # the recipe's size: the px its numbers are written for
LARGE_SIZE = 960  # the size rendered again, four times as many px along each side
LOOK_TOLERANCE = 0.02  # the most the mean of any colour may differ: 2% of the range
CHANGE_SHARE = 0.2  # the most the mean change of any colour may differ: 20% of its value at the design size ...
CHANGE_FLOOR = 0.001  # ... or this, when that is less: a tenth of a percent of the range

# Effects whose default look legitimately differs between the two sizes, each with the reason; "BUG: ..." marks a
# defect waiting for its fix (remove the entry with the fix: a test fails while a passing effect is listed).
SIZE_EXCEPTIONS: dict[str, str] = {
    "jpeg_crush": "BUG: the 8 x 8 block grid of the codec is in output px, so at 4 times the size the blocks are 4 "
    "times finer and change the picture less",
}


@pytest.fixture(scope="module")
def design_photo() -> tuple[np.ndarray, dict[str, np.ndarray]]:
    """A 960 x 720 picture (smooth colour with some fine detail) and the subject and depth maps for it: a recipe of
    size 240 renders it four times smaller, one of size 960 as it is."""
    rng = np.random.default_rng(1)
    height, width = 720, 960
    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    ramp = np.stack([x / (width - 1), y / (height - 1), (x + y) / (width + height - 2)], axis=-1)
    blobs = ndimage.zoom(rng.random((9, 12, 3)), (height / 9, width / 12, 1), order=3)
    fine = ndimage.zoom(rng.random((72, 96, 3)), (10, 10, 1), order=1)
    lights = np.zeros(
        (height, width), np.float32
    )  # a few white highlights, for the effects that pick out bright points
    lights[rng.integers(0, height, 24), rng.integers(0, width, 24)] = 1
    lights = ndimage.gaussian_filter(lights, 14) * 14 * 14 * 2 * np.pi
    picture = np.clip(ramp * 0.5 + blobs * 0.35 + fine * 0.15 + lights[..., None], 0, 1).astype(np.float32)
    off_centre = np.hypot(x - width / 2, y - height / 2)
    maps = {
        "subject": np.clip((height / 3 - off_centre) / 2, 0, 1).astype(np.float32),
        "depth": (1 - off_centre / off_centre.max()).astype(np.float32),
    }
    return picture, maps


def _renders(name: str, picture: np.ndarray, maps: dict[str, np.ndarray]) -> dict[int, tuple[np.ndarray, np.ndarray]]:
    """For the sizes 240 and 960: the picture at that size and the effect `name` at its defaults applied to it, in a
    recipe of size 240. The picture is the same photo scaled the same way (a step with no opacity leaves it be)."""
    step = Step(name, partition="subject") if "partition" in registry.inputs(name) else Step(name)
    untouched = Recipe("untouched", [Step("grade", opacity=0)], size=DESIGN_SIZE)
    look = Recipe(name, [step], size=DESIGN_SIZE)
    return {
        size: (
            runner.render(picture, untouched, maps=maps, size=size),
            runner.render(picture, look, maps=maps, size=size),
        )
        for size in (DESIGN_SIZE, LARGE_SIZE)
    }


def _mean_drift(name: str, picture: np.ndarray, maps: dict[str, np.ndarray]) -> float:
    """How far the mean colour of the effect `name` differs between the render at 240 and the render at 960 shrunk
    back to 240 by averaging each 4 x 4 block: the largest difference of the three channels. Blind to marks that
    lighten as much as they darken."""
    renders = _renders(name, picture, maps)
    at_design, at_large = renders[DESIGN_SIZE][1], renders[LARGE_SIZE][1]
    ratio = LARGE_SIZE // DESIGN_SIZE
    height, width = at_design.shape[:2]
    shrunk = at_large.reshape(height, ratio, width, ratio, 3).mean(axis=(1, 3))
    return float(np.abs(shrunk.mean(axis=(0, 1)) - at_design.mean(axis=(0, 1))).max())


def _change_agrees(name: str, picture: np.ndarray, maps: dict[str, np.ndarray]) -> bool:
    """Whether the effect `name` changes the picture as much at 960 as at 240: per colour, the mean of |output - input|
    at 960 is within `CHANGE_SHARE` of its value at 240 (at least `CHANGE_FLOOR`). It ignores where the changes are,
    so marks drawn at other places at each size agree, and it sees marks become smaller or fewer, which the mean
    does not when light marks and dark marks cancel."""
    change = {size: np.abs(out - rgb).mean(axis=(0, 1)) for size, (rgb, out) in _renders(name, picture, maps).items()}
    at_design = change[DESIGN_SIZE]
    return bool((np.abs(change[LARGE_SIZE] - at_design) <= np.maximum(CHANGE_FLOOR, CHANGE_SHARE * at_design)).all())


def _looks_alike(name: str, picture: np.ndarray, maps: dict[str, np.ndarray]) -> bool:
    """Whether the effect `name` passes both the mean check and the change check."""
    return _mean_drift(name, picture, maps) <= LOOK_TOLERANCE and _change_agrees(name, picture, maps)


@pytest.mark.parametrize("name", [name for name in ALL if name not in SIZE_EXCEPTIONS])
def test_every_effect_at_its_defaults_looks_the_same_at_its_design_size_and_at_four_times_it(name, design_photo):
    assert _mean_drift(name, *design_photo) <= LOOK_TOLERANCE
    assert _change_agrees(name, *design_photo)


def test_the_size_exceptions_name_only_effects_that_still_differ(design_photo):
    assert set(SIZE_EXCEPTIONS) - set(ALL) == set()
    assert [name for name in SIZE_EXCEPTIONS if _looks_alike(name, *design_photo)] == []


class _DotsInPx(EffectParams):
    """Dots whose size and spacing are both design pixels."""

    size: float = param(4.0, unit="px", min=1, max=64, doc="Width of a dot, in px.")
    spacing: float = param(8.0, unit="px", min=2, max=64, doc="Distance between dot centres, in px.")


class _DotsSpacedInCount(EffectParams):
    """The spacing is declared as a count, so it is not scaled with the size."""

    size: float = param(4.0, unit="px", min=1, max=64, doc="Width of a dot, in px.")
    spacing: float = param(8.0, unit="count", min=2, max=64, doc="Distance between dot centres, in px.")


@pytest.mark.parametrize(
    ("params", "same_look"),
    [
        pytest.param(_DotsInPx, True, id="dots whose size and spacing are px"),
        pytest.param(_DotsSpacedInCount, False, id="dots whose spacing is not scaled: 16 times as many at 4 times"),
    ],
)
def test_the_size_check_flags_marks_that_do_not_scale_and_passes_marks_that_do(
    params, same_look, design_photo, throwaway_effects
):
    @effect(category="light", params=params)
    def dot_probe(rgb, params):
        """White dots on a grid."""
        y, x = np.mgrid[0 : rgb.shape[0], 0 : rgb.shape[1]]
        dots = (y % params.spacing < params.size) & (x % params.spacing < params.size)
        return np.where(dots[..., None], np.float32(1), rgb)

    assert _looks_alike("dot_probe", *design_photo) == same_look


class _SpecksInPx(EffectParams):
    """Specks whose size is design pixels."""

    count: int = param(40, unit="count", min=0, max=500, doc="How many specks there are.")
    size: float = param(8.0, unit="px", min=1, max=64, doc="Width of a speck, in px.")


class _SpecksSizedInCount(EffectParams):
    """The size is declared as a count, so it is not scaled with the size."""

    count: int = param(40, unit="count", min=0, max=500, doc="How many specks there are.")
    size: float = param(8.0, unit="count", min=1, max=64, doc="Width of a speck, in px.")


@pytest.mark.parametrize(
    ("params", "change_agrees"),
    [
        pytest.param(_SpecksInPx, True, id="specks whose size is px"),
        pytest.param(_SpecksSizedInCount, False, id="specks whose size is not scaled: 16 times less of the picture"),
    ],
)
def test_the_change_check_flags_scattered_marks_that_shrink_though_their_mean_holds(
    params, change_agrees, design_photo, throwaway_effects
):
    @effect(category="light", params=params)
    def speck_probe(rgb, params):
        """Black and white specks at random places, as many of each."""
        rng = np.random.default_rng(0)  # the places depend on the picture's size, as for a scattering effect
        out = rgb.copy()
        side = round(params.size)
        for index in range(params.count):
            y = rng.integers(0, rgb.shape[0] - side)
            x = rng.integers(0, rgb.shape[1] - side)
            out[y : y + side, x : x + side] = index % 2
        return out

    assert _mean_drift("speck_probe", *design_photo) <= LOOK_TOLERANCE
    assert _change_agrees("speck_probe", *design_photo) == change_agrees
