"""Contract tests every effect must pass, whatever it does: shape, range, determinism, documentation, parameter names,
categories, and every parameter across its documented range."""

import numpy as np
import pytest

from photo_fx import effects
from photo_fx.core import registry
from photo_fx.core.params import UNITS, EffectParams, param
from photo_fx.core.registry import effect

ALL = sorted(effects.describe())


@pytest.mark.parametrize("name", ALL)
def test_every_effect_keeps_shape_dtype_and_range(name, photo):
    out = effects.apply(name, photo)
    assert out.shape == photo.shape
    assert out.dtype == np.float32
    assert np.isfinite(out).all()
    assert out.min() >= -1e-6
    assert out.max() <= 1 + 1e-6


@pytest.mark.parametrize("name", ALL)
def test_every_effect_is_deterministic_and_leaves_its_input_alone(name, photo):
    before = photo.copy()
    first, second = effects.apply(name, photo), effects.apply(name, photo)
    np.testing.assert_array_equal(first, second)
    np.testing.assert_array_equal(photo, before)


# Sizes no cell, tile or block divides: a single pixel, a single row or column, and two odd rectangles.
ODD_SIZES = [(1, 1), (1, 7), (7, 1), (13, 29), (61, 37)]


@pytest.mark.parametrize("size", ODD_SIZES, ids=[f"{h}x{w}" for h, w in ODD_SIZES])
@pytest.mark.parametrize("name", ALL)
def test_every_effect_handles_any_picture_size_and_leaves_its_input_alone(name, size, photo):
    rgb = np.ascontiguousarray(photo[: size[0], : size[1]])
    before = rgb.copy()
    inputs = _inputs(name, rgb)
    out = effects.apply(name, rgb, **inputs)
    assert out.shape == rgb.shape
    assert out.dtype == np.float32
    assert np.isfinite(out).all()
    assert out.min() >= -1e-6
    assert out.max() <= 1 + 1e-6
    np.testing.assert_array_equal(rgb, before)


@pytest.mark.parametrize("name", ALL)
def test_every_effect_documents_itself(name):
    assert effects.describe()[name]


def test_unknown_effect_lists_choices(photo, throwaway_effects, register_probe):
    register_probe("listed_probe")
    with pytest.raises(KeyError) as raised:
        effects.apply("nope", photo)
    message = str(raised.value)
    # the given name and the choices
    assert "'nope'" in message
    assert "listed_probe" in message
    assert "ascii" in message


def test_an_emptied_registry_holds_only_what_the_test_registers_and_gets_the_real_effects_back(
    registry_emptied, register_probe
):
    """The helper behind `fresh_registry`: real effects come back, in their order, even when the block fails."""
    real = effects.all_effects()
    real_categories = {name: effects.category(name) for name in real}

    def use_the_emptied_registry_then_fail():
        with registry_emptied():
            assert effects.all_effects() == {}
            register_probe("only_probe", "light")
            assert list(effects.all_effects()) == ["only_probe"]
            raise RuntimeError("test failed")

    with pytest.raises(RuntimeError, match="test failed"):
        use_the_emptied_registry_then_fail()
    assert list(effects.all_effects().items()) == list(real.items())
    assert {name: effects.category(name) for name in real} == real_categories


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


def test_the_vocabulary_is_the_agreed_one():
    from photo_fx.core.vocabulary import DISCOURAGED

    assert DISCOURAGED == {
        "strength": "amount", "intensity": "amount", "factor": "amount", "colour": "color", "rotation": "angle",
        "random_state": "seed", "rng": "seed",
    }  # fmt: skip


# --- categories --------------------------------------------------------------------------------------------------


AGREED_CATEGORIES = (
    "colour", "light", "texture", "stylised", "text", "distortion", "glitch", "atmosphere", "subject",
    "compositing", "backgrounds", "utilities", "animation",
)  # fmt: skip


def test_the_categories_are_exactly_the_agreed_slugs_in_order():
    assert effects.CATEGORIES == AGREED_CATEGORIES  # the published list effect authors choose from


def test_categories_maps_every_category_in_order_to_its_sorted_effects(fresh_registry, register_probe):
    for name, category in [("zeta", "animation"), ("beta", "colour"), ("alpha", "colour"), ("gamma", "light")]:
        register_probe(name, category)  # registered out of order, on purpose
    expected: dict[str, list[str]] = {category: [] for category in AGREED_CATEGORIES}
    expected |= {"colour": ["alpha", "beta"], "light": ["gamma"], "animation": ["zeta"]}
    assert effects.categories() == expected
    assert list(effects.categories()) == list(AGREED_CATEGORIES)  # dict order is the catalogue order


def test_categories_lists_a_category_with_no_effects_as_empty(fresh_registry, register_probe):
    register_probe("lamp", "light")
    assert effects.categories()["distortion"] == []  # listed, although it has nothing yet
    register_probe("swirl", "distortion")
    assert effects.categories()["distortion"] == ["swirl"]


def test_the_real_effects_are_each_in_exactly_one_category():
    grouped = effects.categories()
    assert list(grouped) == list(AGREED_CATEGORIES)
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
    out = effects.apply(name, small, **{field: _bound_value(name, field, side)}, **_inputs(name, small))
    assert out.shape == small.shape
    assert np.isfinite(out).all()
    assert out.min() >= -1e-6
    assert out.max() <= 1 + 1e-6


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
    out = effects.apply(name, small, **{field: value}, **_inputs(name, small))
    assert out.shape == small.shape
    assert np.isfinite(out).all()
    assert out.min() >= -1e-6
    assert out.max() <= 1 + 1e-6


@pytest.mark.parametrize(("name", "field", "side"), _bounds())
def test_every_number_beyond_its_documented_limit_is_refused(name, field, side, photo):
    with pytest.raises(ValueError, match=f"`{field}`"):
        effects.apply(name, _small(photo), **{field: _bound_value(name, field, side, beyond=True)})


@pytest.mark.parametrize("name", ALL)
def test_every_effect_refuses_an_unknown_parameter(name, photo):
    with pytest.raises(ValueError, match="not_a_parameter"):
        effects.apply(name, _small(photo), not_a_parameter=1)


@pytest.mark.parametrize("name", ALL)
def test_every_effect_takes_its_model_its_keywords_or_nothing(name, photo):
    small, model = _small(photo), registry.parameters(name)
    inputs = _inputs(name, small)
    defaults = {field: info.default for field, info in model.model_fields.items()}
    by_model = effects.get(name)(small, model(), **inputs)
    np.testing.assert_array_equal(effects.get(name)(small, **inputs), by_model)
    np.testing.assert_array_equal(effects.get(name)(small, **defaults, **inputs), by_model)
