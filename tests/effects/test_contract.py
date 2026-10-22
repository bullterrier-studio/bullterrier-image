"""Contract tests every effect must pass, whatever it does, and the rules of the effect registry."""

import importlib
import inspect

import numpy as np
import pytest

from photo_fx import effects
from photo_fx.effects._registry import effect

ALL = sorted(effects.describe())


@pytest.mark.parametrize("name", ALL)
def test_every_effect_keeps_shape_dtype_and_range(name, photo):
    out = effects.apply(name, photo)
    assert out.shape == photo.shape
    assert out.dtype == np.float32
    assert np.isfinite(out).all()
    assert out.min() >= -1e-6 and out.max() <= 1 + 1e-6


@pytest.mark.parametrize("name", ALL)
def test_every_effect_is_deterministic_and_leaves_its_input_alone(name, photo):
    before = photo.copy()
    first, second = effects.apply(name, photo), effects.apply(name, photo)
    np.testing.assert_array_equal(first, second)
    np.testing.assert_array_equal(photo, before)


@pytest.mark.parametrize("name", ALL)
def test_every_effect_documents_itself(name):
    assert effects.describe()[name]


def test_unknown_effect_lists_choices(photo):
    with pytest.raises(KeyError, match="ascii"):
        effects.apply("nope", photo)


@pytest.mark.parametrize("name", ALL)
def test_every_effect_can_run_on_the_image_alone(name):
    image, *params = inspect.signature(effects.get(name)).parameters.values()
    assert [p.name for p in params if p.default is inspect.Parameter.empty] == []


# --- categories --------------------------------------------------------------------------------------------------


def test_every_existing_effect_has_its_category():
    assert {name: effects.category(name) for name in ALL} == {
        "grade": "colour",
        "gradient_map": "colour",
        "split_tone": "colour",
        "hue_shift": "colour",
        "clarity": "light",
        "glow": "light",
        "grain": "texture",
        "halftone": "stylised",
        "ascii": "text",
        "glitch": "glitch",
        "fill": "subject",
        "evaporate": "animation",
        "disperse": "animation",
    }


def test_the_categories_are_exactly_the_agreed_slugs_in_order():
    assert effects.CATEGORIES == (
        "colour", "light", "texture", "stylised", "text", "distortion", "glitch", "atmosphere", "subject",
        "compositing", "backgrounds", "utilities", "animation",
    )  # fmt: skip


def test_categories_maps_every_category_in_order_to_its_sorted_effects():
    grouped = effects.categories()
    assert list(grouped) == list(effects.CATEGORIES)
    assert grouped["colour"] == ["grade", "gradient_map", "hue_shift", "split_tone"]
    assert grouped["animation"] == ["disperse", "evaporate"]
    assert grouped["distortion"] == []  # no effect yet, but the category is listed
    assert sorted(name for names in grouped.values() for name in names) == ALL


def _probe(rgb, amount: float = 0.5):
    """A throwaway effect."""
    return rgb


@pytest.mark.parametrize(
    "register",
    [
        lambda fn: effect(fn),  # the old bare `@effect`, without a category
        lambda fn: effect()(fn),
        lambda fn: effect(category=None)(fn),
        lambda fn: effect(category="colours")(fn),
        lambda fn: effect(category="Colour")(fn),
    ],
    ids=["bare", "empty", "none", "misspelt", "capitalised"],
)
def test_registering_without_a_valid_category_lists_the_valid_ones(register, throwaway_effects):
    with pytest.raises(ValueError, match=r"_probe.*colour, light, texture, stylised, text, .*, animation"):
        register(_probe)
    assert "_probe" not in effects.describe()


def test_an_unknown_category_names_what_was_given(throwaway_effects):
    with pytest.raises(ValueError, match="'colours'"):
        effect(category="colours")(_probe)


def test_a_valid_category_registers_the_effect_unchanged(throwaway_effects):
    registered = effect(category="utilities")(_probe)
    assert registered is _probe  # the decorator does not wrap the function
    assert effects.get("_probe") is _probe and effects.category("_probe") == "utilities"
    assert "_probe" in effects.categories()["utilities"]


# --- every parameter has a default -------------------------------------------------------------------------------


def test_a_parameter_without_a_default_is_rejected_naming_the_effect_and_the_parameter(throwaway_effects):
    def needs_radius(rgb, radius, amount=1.0):
        """Not callable on the image alone."""
        return rgb

    with pytest.raises(ValueError, match=r"needs_radius.*`radius`.*default"):
        effect(category="light")(needs_radius)
    assert "needs_radius" not in effects.describe()


@pytest.mark.parametrize(
    "source",
    [
        "def loose(rgb, *args):\n    return rgb",
        "def loose(rgb, **params):\n    return rgb",
        "def loose():\n    return 0",
    ],
    ids=["args", "kwargs", "no-image"],
)
def test_an_effect_must_take_the_image_and_name_every_parameter(source, throwaway_effects):
    namespace = {}
    exec(source, namespace)  # builds a function with exactly the signature under test
    with pytest.raises(ValueError, match="loose"):
        effect(category="utilities")(namespace["loose"])
    assert "loose" not in effects.describe()


# --- duplicate names ---------------------------------------------------------------------------------------------


def test_a_second_function_under_an_existing_name_is_an_error_naming_both_modules(throwaway_effects):
    def twin(rgb, amount: float = 0.5):
        """First."""
        return rgb

    first = twin
    first.__module__ = "photo_fx.effects.first_module"

    def twin(rgb, amount: float = 0.5):  # noqa: F811  (the point of the test: a second function, same name)
        """Second."""
        return 1 - rgb

    twin.__module__ = "photo_fx.effects.second_module"
    effect(category="utilities")(first)
    with pytest.raises(ValueError, match=r"twin.*first_module.*second_module"):
        effect(category="utilities")(twin)
    assert effects.get("twin") is first  # the first one stays


def test_registering_the_same_function_twice_is_allowed(throwaway_effects):
    effect(category="utilities")(_probe)
    effect(category="utilities")(_probe)
    assert effects.get("_probe") is _probe


def test_reloading_an_effects_module_is_not_a_duplicate():
    before = effects.get("grade")
    color = importlib.reload(importlib.import_module("photo_fx.effects.color"))
    assert effects.get("grade") is color.grade and color.grade is not before  # the reloaded function replaced it


# --- summary -----------------------------------------------------------------------------------------------------


def test_summary_is_the_first_sentence_of_the_docstring():
    assert effects.summary("grade") == "Basic tone controls."
    assert effects.summary("fill") == "Replace the whole picture with one colour."


def test_summary_is_the_first_sentence_even_with_decimals_and_abbreviations(throwaway_effects):
    def blend_probe(rgb, amount: float = 0.5):
        """Blend 0.5 of it over 0..1 of the
        width, e.g. half! Then more. And more."""
        return rgb

    effect(category="utilities")(blend_probe)
    assert effects.summary("blend_probe") == "Blend 0.5 of it over 0..1 of the width, e.g. half!"


def test_summary_of_a_one_sentence_docstring_without_a_full_stop_is_all_of_it(throwaway_effects):
    def terse(rgb, amount: float = 0.5):
        """Does a thing"""
        return rgb

    effect(category="utilities")(terse)
    assert effects.summary("terse") == "Does a thing"
