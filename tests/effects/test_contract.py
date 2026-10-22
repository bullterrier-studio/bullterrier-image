"""Contract tests every effect must pass, whatever it does, and the rules of the effect registry."""

import importlib
import inspect
import json
import os
import shutil
import subprocess
import sys
import textwrap
from pathlib import Path

import numpy as np
import pytest

import photo_fx
from photo_fx import effects
from photo_fx.core.registry import effect

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


def test_unknown_effect_lists_choices(photo, fresh_registry, register_probe):
    register_probe("listed_probe")
    with pytest.raises(KeyError, match="unknown effect 'nope'; available: listed_probe"):
        effects.apply("nope", photo)


@pytest.mark.parametrize("name", ALL)
def test_every_effect_can_run_on_the_image_alone(name):
    image, *params = inspect.signature(effects.get(name)).parameters.values()
    assert [p.name for p in params if p.default is inspect.Parameter.empty] == []


# --- parameter vocabulary ----------------------------------------------------------------------------------------


def _naming_problems(name: str) -> list[str]:
    """Every parameter of the effect `name` that uses a discouraged name, with the name to use instead."""
    from photo_fx.core.vocabulary import DISCOURAGED

    return [
        f"effect {name!r}: parameter `{parameter}` should be called `{DISCOURAGED[parameter]}`"
        for parameter in inspect.signature(effects.get(name)).parameters
        if parameter in DISCOURAGED
    ]


@pytest.mark.parametrize("name", ALL)
def test_no_effect_uses_a_discouraged_parameter_name(name):
    assert _naming_problems(name) == []


def test_the_vocabulary_check_flags_discouraged_names_and_passes_preferred_ones(throwaway_effects):
    @effect(category="light")
    def bad_probe(rgb, strength: float = 0.5, radius: float = 3.0, rng: int = 0):
        """Uses two discouraged names."""
        return rgb

    @effect(category="light")
    def good_probe(rgb, amount: float = 0.5, radius: float = 3.0, seed: int = 0):
        """Uses the preferred names."""
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


# --- discovery ---------------------------------------------------------------------------------------------------

_NEW_MODULES = {
    "zz_probe.py": '''
        from photo_fx.core.registry import effect

        @effect(category="utilities")
        def probe_effect(rgb, amount: float = 0.5):
            """A brand-new effect in a brand-new module."""
            return rgb
    ''',
    "_hidden.py": '''
        from photo_fx.core.registry import effect

        @effect(category="utilities")
        def hidden_probe(rgb, amount: float = 0.5):
            """Lives in a private module, which is not imported automatically."""
            return rgb
    ''',
    "_order_log.py": "seen = []",
    "b_order.py": "from . import _order_log\n_order_log.seen.append(__name__)",
    "a_order.py": "from . import _order_log\n_order_log.seen.append(__name__)",
}

_REPORT = """
    import contextlib, io, json, sys
    import photo_fx
    from photo_fx import effects, fx
    from photo_fx.cli import main
    from photo_fx.effects import _order_log
    from photo_fx.recipe import Step

    listing = io.StringIO()
    with contextlib.redirect_stdout(listing):
        main(["effects"])
    json.dump({
        "package": photo_fx.__file__,
        "registered": sorted(effects.describe()),
        "fx": fx.probe_effect(amount=0.2).effect,
        "recipe_step": Step("probe_effect", {"amount": 0.2}).effect,
        "cli_lists_it": "probe_effect" in listing.getvalue(),
        "import_order": _order_log.seen,
    }, sys.stdout)
"""


def test_a_new_module_in_the_effects_package_is_found_with_no_other_edit(tmp_path):
    """On a copy of the package: drop modules into effects/ and nothing else, then look from a fresh interpreter."""
    package = tmp_path / "photo_fx"
    shutil.copytree(Path(photo_fx.__file__).parent, package, ignore=shutil.ignore_patterns("__pycache__"))
    for name, source in _NEW_MODULES.items():
        (package / "effects" / name).write_text(textwrap.dedent(source))
    environment = {**os.environ, "PYTHONPATH": str(tmp_path)}
    done = subprocess.run(
        [sys.executable, "-c", textwrap.dedent(_REPORT)], env=environment, capture_output=True, text=True, check=False
    )
    assert done.returncode == 0, done.stderr
    report = json.loads(done.stdout)
    assert Path(report["package"]).is_relative_to(tmp_path)  # it really looked at the copy
    assert set(report["registered"]) == set(ALL) | {"probe_effect"}  # found; the private module's effect is not
    assert report["fx"] == report["recipe_step"] == "probe_effect" and report["cli_lists_it"]
    assert report["import_order"] == ["photo_fx.effects.a_order", "photo_fx.effects.b_order"]  # sorted, not as found


# --- categories --------------------------------------------------------------------------------------------------


def test_the_categories_are_exactly_the_agreed_slugs_in_order():
    assert effects.CATEGORIES == (
        "colour", "light", "texture", "stylised", "text", "distortion", "glitch", "atmosphere", "subject",
        "compositing", "backgrounds", "utilities", "animation",
    )  # fmt: skip


def test_categories_maps_every_category_in_order_to_its_sorted_effects(fresh_registry, register_probe):
    for name, category in [("zeta", "animation"), ("beta", "colour"), ("alpha", "colour"), ("gamma", "light")]:
        register_probe(name, category)  # registered out of order, on purpose
    expected = {category: [] for category in effects.CATEGORIES}
    expected |= {"colour": ["alpha", "beta"], "light": ["gamma"], "animation": ["zeta"]}
    assert effects.categories() == expected
    assert list(effects.categories()) == list(effects.CATEGORIES)  # dict order is the catalogue order


def test_categories_lists_a_category_with_no_effects_as_empty(fresh_registry, register_probe):
    register_probe("lamp", "light")
    assert effects.categories()["distortion"] == []  # listed, although it has nothing yet
    register_probe("swirl", "distortion")
    assert effects.categories()["distortion"] == ["swirl"]


def test_the_real_effects_are_each_in_exactly_one_category():
    grouped = effects.categories()
    assert list(grouped) == list(effects.CATEGORIES)
    assert sorted(name for names in grouped.values() for name in names) == ALL
    assert all(names == sorted(names) for names in grouped.values())


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


def test_reloading_an_effects_module_is_not_a_duplicate(tmp_path, monkeypatch, throwaway_effects):
    (tmp_path / "reload_probe_module.py").write_text(
        "from photo_fx.core.registry import effect\n\n"
        '@effect(category="utilities")\n'
        'def reload_probe(rgb, amount: float = 0.5):\n    """Reloaded."""\n    return rgb\n'
    )
    monkeypatch.syspath_prepend(str(tmp_path))
    monkeypatch.delitem(sys.modules, "reload_probe_module", raising=False)  # and removed again afterwards
    first = importlib.import_module("reload_probe_module").reload_probe
    reloaded = importlib.reload(sys.modules["reload_probe_module"]).reload_probe
    assert reloaded is not first and effects.get("reload_probe") is reloaded  # replaced, not rejected


# --- summary -----------------------------------------------------------------------------------------------------


def test_summary_is_the_first_sentence_of_the_docstring(fresh_registry):
    @effect(category="utilities")
    def plain(rgb, amount: float = 0.5):
        """Basic probe controls. Then a second sentence."""
        return rgb

    assert effects.summary("plain") == "Basic probe controls."


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
