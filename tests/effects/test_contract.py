"""Contract tests every effect must pass, whatever it does: shape, range, determinism, documentation, parameter names,
categories, every parameter across its documented range, and a time and memory budget on a real-sized photo."""

import time
import tracemalloc

import numpy as np
import pytest

from photo_fx.core import registry
from photo_fx.core.params import UNITS, EffectParams, param
from photo_fx.core.registry import effect

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
    from photo_fx.core.vocabulary import DISCOURAGED

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


# --- categories --------------------------------------------------------------------------------------------------


def test_categories_maps_every_category_in_catalogue_order_to_its_sorted_effects(fresh_registry, register_probe):
    """A category with no effects is listed too, empty. The catalogue order is `registry.CATEGORIES`, the published
    list effect authors choose from."""
    for name, category in [("zeta", "animation"), ("beta", "colour"), ("alpha", "colour"), ("gamma", "light")]:
        register_probe(name, category)  # registered out of order, on purpose
    expected: dict[str, list[str]] = {category: [] for category in registry.CATEGORIES}
    expected |= {"colour": ["alpha", "beta"], "light": ["gamma"], "animation": ["zeta"]}
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
    """The injected inputs the effect declares, made for `photo`: a soft disc as the subject's matte."""
    y, x = np.mgrid[0 : photo.shape[0], 0 : photo.shape[1]].astype(np.float32)
    disc = np.clip(min(photo.shape[:2]) / 3 - np.hypot(x - photo.shape[1] / 2, y - photo.shape[0] / 2), 0, 1)
    return {"matte": disc.astype(np.float32)} if "matte" in registry.inputs(name) else {}


def _bounds() -> list[tuple[str, str, str]]:
    """(effect, numeric field, "min" or "max") for every numeric field of every effect."""
    found = []
    for name in ALL:
        model = registry.parameters(name)
        found += [
            (name, doc.name, side) for doc in model.fields_doc() if doc.minimum is not None for side in ("min", "max")
        ]
    return found


def _bound_value(name: str, field: str, side: str, beyond: bool = False) -> float:
    model = registry.parameters(name)
    doc = next(d for d in model.fields_doc() if d.name == field)
    bound = doc.minimum if side == "min" else doc.maximum
    assert bound is not None
    value = bound + ((-1 if side == "min" else 1) if beyond else 0)
    return int(value) if isinstance(doc.default, int) else float(value)


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
    schema = registry.schema(name)
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
    effect (ints rounded, repeats dropped): the values an animation passes through."""
    found = []
    for name in ALL:
        model = registry.parameters(name)
        for doc in model.fields_doc():
            if doc.minimum is None or doc.maximum is None or not isinstance(doc.default, int | float):
                continue  # not a number (a list of numbers is not animated as a whole)
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


# --- cost: time and memory on a real-sized photo -----------------------------------------------------------------

CPU_BUDGET = 1.0  # seconds of process CPU time per effect at 1 MP: about 10x the slowest measured (see test below)
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
    """Measured on an Apple M4 (load 4 to 18): the slowest effects take 0.1 s of CPU time, the hungriest holds 7.3
    picture-sized buffers; the budgets leave room for a slower machine, never for a 10x regression."""
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
