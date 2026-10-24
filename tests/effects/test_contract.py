"""Contract tests every effect must pass, whatever it does: shape, range, determinism, documentation, parameter names,
categories, every parameter across its documented range, and a time and memory budget on a real-sized photo."""

import itertools
import time
import tracemalloc
from typing import Any, Literal

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image.core import registry
from bullterrier_image.core.params import UNITS, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.core.steps import Step

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
    first, second = registry.apply(name, photo), registry.apply(name, photo)
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
    out = registry.apply(name, photo, **step.params, **inputs)
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
    """The injected inputs the effect declares, made for `photo`: a soft disc as the subject's matte, the four quarters
    of the picture as its partition."""
    height, width = photo.shape[:2]
    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    disc = np.clip(min(height, width) / 3 - np.hypot(x - width / 2, y - height / 2), 0, 1)
    quarters = (y * 2 // height) * 2 + x * 2 // width
    made = {"matte": disc.astype(np.float32), "partition": quarters.astype(np.int32)}
    return {key: value for key, value in made.items() if key in registry.inputs(name)}


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


@pytest.mark.parametrize(("name", "field", "side"), _bounds())
def test_every_number_beyond_its_documented_limit_is_refused(name, field, side, photo):
    with pytest.raises(ValueError, match=f"`{field}`"):
        registry.apply(name, _small(photo), **{field: _bound_value(name, field, side, beyond=True)})


@pytest.mark.parametrize(("name", "field", "choice"), _choices())
def test_every_effect_works_with_each_choice_it_offers(name, field, choice, photo):
    small = _small(photo)
    assert_a_picture_like(registry.apply(name, small, **{field: choice}, **_inputs(name, small)), small)


@pytest.mark.parametrize(("name", "field"), sorted({(name, field) for name, field, _ in _choices()}))
def test_a_choice_that_is_not_offered_is_refused(name, field, photo):
    with pytest.raises(ValueError, match=f"`{field}`"):
        registry.apply(name, _small(photo), **{field: "not_a_choice"})


@pytest.mark.parametrize("name", ALL)
def test_every_effect_refuses_an_unknown_parameter(name, photo):
    with pytest.raises(ValueError, match="not_a_parameter"):
        registry.apply(name, _small(photo), not_a_parameter=1)


@pytest.mark.parametrize("name", ALL)
def test_every_effect_takes_its_model_its_keywords_or_nothing(name, photo):
    small, model = _small(photo), registry.parameters(name)
    inputs = _inputs(name, small)
    defaults = {field: info.default for field, info in model.model_fields.items()}
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


# Effects that change a flat picture by design, unless the parts that do are off: a screen's mask, scanlines, vignette
# and the border beyond its curved glass.
FLAT_NEEDS = {"crt": {"curve": 0.0, "shadow_mask": 0.0, "scanlines": 0.0, "bloom": 0.0, "vignette": 0.0}}


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
    np.testing.assert_allclose(registry.apply(name, flat, **FLAT_NEEDS.get(name, {}), **setting), 0.4, atol=1e-6)


@pytest.mark.parametrize("name", sorted({name for name, _ in _sampling_cases()}))
def test_each_interpolation_reads_the_picture_its_own_way(name):
    noise = np.random.default_rng(4).random((41, 41, 3), dtype=np.float32)
    choices = registry.parameters(name).model_json_schema()["properties"]["interpolation"]["enum"]
    outs = {choice: registry.apply(name, noise, interpolation=choice) for choice in choices}
    for first, second in itertools.combinations(choices, 2):
        assert not np.array_equal(outs[first], outs[second]), f"{first} and {second} give the same picture"


SEED_NEEDS = {"disperse": {"progress": 0.5}}  # at its default progress nothing has moved yet, whatever the seed


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

# effect: the settings that make it draw many white particles (snow has no colour of its own)
PARTICLES: dict[str, dict[str, Any]] = {
    "embers": {"count": 300, "color": "#ffffff"},
    "floating_dust": {"count": 300, "color": "#ffffff"},
    "rain": {"density": 3.0, "color": "#ffffff"},
    "snow": {"density": 3.0},
}
TINTED = ["embers", "floating_dust", "rain"]


@pytest.mark.parametrize(
    ("name", "none"),
    [
        pytest.param("embers", {"count": 0}, id="embers"),
        pytest.param("floating_dust", {"count": 0}, id="floating_dust"),
        pytest.param("rain", {"density": 0.0}, id="rain"),
        pytest.param("rain", {"density": 0.0, "color": "#e6eefa"}, id="rain, tinted"),
        pytest.param("snow", {"density": 0.0}, id="snow"),
        pytest.param("sparkle", {"count": 0, "threshold": 0.0}, id="sparkle"),
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


# well separated particles, each big and sharp enough to reach full brightness at its middle (snow: its layers 1/4..1)
SEPARATE = {
    "embers": {"count": 40, "size": 8.0, "length": 0.0, "softness": 0.0},
    "floating_dust": {"count": 40, "size": 8.0, "softness": 0.0},
    "rain": {"density": 0.3, "length": 8.0, "thickness": 8.0},
    "snow": {"density": 0.1, "size": 40.0, "layers": 4, "softness": 0.0},
}


@pytest.mark.parametrize("name", SEPARATE)
def test_depth_spreads_the_brightness_of_the_particles_from_all_alike_to_the_farthest_nearly_gone(name):
    def peaks(depth):
        settings = {**PARTICLES[name], **SEPARATE[name], "depth": depth, "seed": 1}
        light = registry.apply(name, np.zeros((400, 400, 3), np.float32), **settings)[..., 0]
        patches, found = ndimage.label(light > 0, structure=np.ones((3, 3)))
        return np.array(ndimage.maximum(light, patches, range(1, found + 1)))

    np.testing.assert_allclose(peaks(0.0), 1.0, atol=0.01)  # no depth: every one at full brightness
    spread = peaks(1.0)  # brightness is the particle's nearness, which runs from 0 (or 1/4 for snow) to 1
    assert spread.min() < 0.3
    assert spread.max() > 0.9


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
