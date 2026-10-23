"""Registering an effect with a parameter model: the rules, the wrapper it becomes, and the registry's queries.

Every effect here is the test's own (`throwaway_effects` removes it again), so nothing depends on the real effects.
"""

import importlib
import json
import subprocess
import sys
import textwrap
from typing import Any

import numpy as np
import pytest
from pydantic import BaseModel

from photo_fx.core import registry
from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect


class GainParams(EffectParams):
    """Gain and offset of the probe."""

    gain: float = param(1.0, unit="ratio", min=0, max=4, doc="Multiplies every value. Raising it brightens.")
    offset: float = param(0.0, unit="fraction", min=-1, max=1, doc="Added after the gain.")


def gain_probe(rgb: np.ndarray, params: GainParams) -> np.ndarray:
    """Brightens by a gain, then shifts by an offset. More text."""
    return np.clip(rgb * params.gain + params.offset, 0, 1).astype(np.float32)


GREY = np.full((4, 5, 3), 0.25, np.float32)


@pytest.fixture
def gain(throwaway_effects):
    return effect(category="light", params=GainParams)(gain_probe)


# --- the wrapper ---------------------------------------------------------------------------------------------------


def test_a_modelled_effect_can_be_called_with_keywords_a_model_or_nothing(gain):
    np.testing.assert_allclose(gain(GREY, gain=2.0, offset=0.1), 0.6, atol=1e-6)  # 0.25 * 2 + 0.1
    np.testing.assert_allclose(gain(GREY, GainParams(gain=2.0, offset=0.1)), 0.6, atol=1e-6)
    np.testing.assert_array_equal(gain(GREY), GREY)  # the defaults: gain 1, offset 0
    np.testing.assert_allclose(registry.apply("gain_probe", GREY, gain=3.0), 0.75, atol=1e-6)


def test_giving_both_a_model_and_keywords_is_an_error(gain):
    with pytest.raises(TypeError, match="gain_probe"):
        gain(GREY, GainParams(gain=2.0), offset=0.1)


def test_a_model_of_another_effect_is_an_error(gain):
    class OtherParams(EffectParams):
        """Another model."""

    with pytest.raises(TypeError, match="GainParams"):
        gain(GREY, OtherParams())


@pytest.mark.parametrize(
    ("keywords", "parts"),
    [({"gian": 2.0}, ["gian", "gain, offset"]), ({"gain": 9.0}, ["`gain`", "<= 4", "9.0"])],
    ids=["unknown: lists the parameters", "out of range: the field, the limit and the value"],
)
def test_a_bad_keyword_is_explained_naming_the_effect(gain, keywords, parts):
    with pytest.raises(ValueError, match="gain_probe") as raised:
        gain(GREY, **keywords)
    for part in parts:
        assert part in str(raised.value)


def test_the_wrapper_keeps_the_name_and_documentation_and_tells_its_model_category_and_inputs(gain):
    assert gain.__name__ == "gain_probe"
    assert gain.__doc__ == gain_probe.__doc__
    assert gain.params_model is GainParams
    assert gain.category == "light"
    assert gain.inputs == ()
    assert registry.get("gain_probe") is gain
    assert "gain_probe" in registry.categories()["light"]


def test_an_effect_that_declares_the_matte_gets_it_and_says_so(throwaway_effects):
    def matte_probe(rgb: np.ndarray, params: GainParams, *, matte: np.ndarray | None = None) -> np.ndarray:
        """Darkens where the matte is."""
        return rgb if matte is None else (rgb * (1 - matte[..., None])).astype(np.float32)

    probe = effect(category="subject", params=GainParams)(matte_probe)
    matte = np.zeros((4, 5), np.float32)
    matte[:, :2] = 1
    out = probe(GREY, gain=2.0, matte=matte)  # parameters and the matte together
    np.testing.assert_array_equal(out[:, :2], 0.0)
    np.testing.assert_array_equal(out[:, 2:], 0.25)
    assert probe.inputs == ("matte",)
    assert registry.inputs("matte_probe") == ("matte",)
    np.testing.assert_array_equal(probe(GREY), GREY)  # without a matte the effect decides what to do


# --- registration rules: each raises and registers nothing -----------------------------------------------------------


def _probe_with(signature: str):
    namespace: dict[str, Any] = {}
    # a function with exactly the signature under test; the signatures are this file's own literals
    exec(f"def shape_probe{signature}:\n    return rgb", namespace)  # noqa: S102  only a def is built from them
    return namespace["shape_probe"]


@pytest.mark.parametrize(
    ("signature", "complaint"),
    [
        ("(rgb)", "shape_probe(rgb, params"),
        ("(rgb, params, extra)", "shape_probe(rgb, params"),
        ("(rgb, params, *more)", "shape_probe(rgb, params"),
        ("(rgb, params, **more)", "shape_probe(rgb, params"),
        ("(rgb, params, matte=None)", "keyword-only"),
        ("(rgb, params, *, glow=None)", "'glow'"),
        ("(rgb, params, *, matte)", "default"),
    ],
    ids=["no-params", "extra-positional", "args", "kwargs", "positional-input", "unknown-input", "input-no-default"],
)
def test_an_effect_function_of_the_wrong_shape_is_refused(signature, complaint, throwaway_effects):
    with pytest.raises(ValueError, match="shape_probe") as raised:
        effect(category="light", params=GainParams)(_probe_with(signature))
    assert complaint in str(raised.value)
    assert "shape_probe" not in registry.describe()


@pytest.mark.parametrize("model", [dict, GainParams(), BaseModel], ids=["dict", "instance", "basemodel"])
def test_params_must_be_an_effect_params_model(model, throwaway_effects):
    with pytest.raises(ValueError, match=r"gain_probe.*EffectParams"):
        effect(category="light", params=model)(gain_probe)
    assert "gain_probe" not in registry.describe()


def test_a_parameter_may_not_be_named_like_an_input(throwaway_effects):
    class MatteParams(EffectParams):
        """Claims the matte as a parameter."""

        matte: float = param(0.5, unit="fraction", min=0, max=1, doc="Not allowed: the recipe supplies the matte.")

    def clash_probe(rgb, params, *, matte=None):
        return rgb

    with pytest.raises(ValueError, match=r"clash_probe.*`matte`"):
        effect(category="light", params=MatteParams)(clash_probe)
    assert "clash_probe" not in registry.describe()


@pytest.mark.parametrize("category", ["lights", "Light", ""], ids=["misspelt", "capitalised", "empty"])
def test_an_unknown_category_is_refused_naming_it_and_listing_the_valid_ones(category, throwaway_effects):
    with pytest.raises(ValueError, match=rf"gain_probe.*{category!r}.*colour, light, texture, .*, animation"):
        effect(category=category, params=GainParams)(gain_probe)
    assert "gain_probe" not in registry.describe()


def test_registering_an_effect_again_keeps_the_very_same_effect(gain):
    assert effect(category="light", params=GainParams)(gain) is gain
    assert registry.get("gain_probe") is gain


def test_registering_the_same_function_twice_is_allowed(throwaway_effects):
    effect(category="utilities", params=GainParams)(gain_probe)
    again = effect(category="utilities", params=GainParams)(gain_probe)
    assert registry.get("gain_probe") is again


def test_a_second_function_under_an_existing_name_is_an_error_naming_both_modules(throwaway_effects):
    def twin(rgb, params):
        """First."""
        return rgb

    first = twin
    first.__module__ = "photo_fx.effects.first_module"

    def twin(rgb, params):  # type: ignore[no-redef]  # the point of the test: a second function, same name
        """Second."""
        return 1 - rgb

    twin.__module__ = "photo_fx.effects.second_module"
    registered = effect(category="utilities", params=GainParams)(first)
    with pytest.raises(ValueError, match=r"twin.*first_module.*second_module"):
        effect(category="utilities", params=GainParams)(twin)
    assert registry.get("twin") is registered  # the first one stays
    np.testing.assert_array_equal(registry.apply("twin", GREY), GREY)


def test_reloading_an_effects_module_is_not_a_duplicate(tmp_path, monkeypatch, throwaway_effects):
    (tmp_path / "reload_probe_module.py").write_text(
        "from photo_fx.core.params import EffectParams\n"
        "from photo_fx.core.registry import effect\n\n"
        "class ReloadParams(EffectParams):\n"
        '    """None."""\n\n'
        '@effect(category="utilities", params=ReloadParams)\n'
        'def reload_probe(rgb, params):\n    """Reloaded."""\n    return rgb\n',
        encoding="utf-8",
    )
    monkeypatch.syspath_prepend(str(tmp_path))
    monkeypatch.delitem(sys.modules, "reload_probe_module", raising=False)  # and removed again afterwards
    first = importlib.import_module("reload_probe_module").reload_probe
    reloaded = importlib.reload(sys.modules["reload_probe_module"]).reload_probe
    # replaced, not rejected
    assert reloaded is not first
    assert registry.get("reload_probe") is reloaded


def test_registering_an_effect_again_with_another_model_is_refused(gain):
    class OtherParams(EffectParams):
        """Another model."""

    with pytest.raises(ValueError, match=r"gain_probe.*OtherParams"):
        effect(category="light", params=OtherParams)(gain)
    assert registry.get("gain_probe").params_model is GainParams


def test_an_effect_without_a_model_is_refused(throwaway_effects):
    def keywords_probe(rgb, amount: float = 0.5):
        """Parameters as keyword arguments, with no model."""
        return rgb

    with pytest.raises(TypeError, match="params"):
        effect(category="utilities")(keywords_probe)  # type: ignore[call-arg]  # the point of the test
    assert "keywords_probe" not in registry.describe()


# --- queries ---------------------------------------------------------------------------------------------------------


def test_parameters_and_schema_describe_the_model(gain):
    assert registry.parameters("gain_probe") is GainParams
    properties = registry.schema("gain_probe")["properties"]
    assert properties["gain"]["unit"] == "ratio"
    assert properties["offset"]["description"] == "Added after the gain."


def test_validate_returns_the_model_or_explains_what_is_wrong(gain):
    assert registry.validate("gain_probe", {"gain": 2}) == GainParams(gain=2.0)
    with pytest.raises(ValueError, match=r"gain_probe.*`offset`.*>= -1"):
        registry.validate("gain_probe", {"offset": -2})


def _summary_of(docstring: str) -> str:
    """The summary of a throwaway effect with this docstring."""

    def summary_probe(rgb, params):
        return rgb

    summary_probe.__doc__ = docstring
    effect(category="utilities", params=GainParams)(summary_probe)
    return registry.summary("summary_probe")


@pytest.mark.parametrize(
    ("docstring", "first_sentence"),
    [
        ("Basic probe controls. Then a second sentence.", "Basic probe controls."),
        (
            "Blend 0.5 of it over 0..1 of the\n        width, e.g. half! Then more. And more.",
            "Blend 0.5 of it over 0..1 of the width, e.g. half!",
        ),
        ("Does a thing", "Does a thing"),
    ],
    ids=["plain", "decimals-and-abbreviations", "no-full-stop"],
)
def test_summary_is_the_first_sentence_of_the_docstring_on_one_line(docstring, first_sentence, throwaway_effects):
    assert _summary_of(docstring) == first_sentence


def test_describe_gives_the_documentation_and_every_parameter(gain):
    text = registry.describe()["gain_probe"]
    assert text.startswith("Brightens by a gain, then shifts by an offset. More text.")
    assert "Parameters" in text
    assert "Multiplies every value. Raising it brightens." in text
    assert "Added after the gain." in text
    assert registry.summary("gain_probe") == "Brightens by a gain, then shifts by an offset."


def test_queries_about_an_unknown_effect_list_the_known_ones(gain):
    for query in (registry.parameters, registry.schema, registry.inputs):
        with pytest.raises(KeyError, match="gain_probe"):
            query("no_such_effect")


# --- ordering and loading ------------------------------------------------------------------------------------------


def test_every_listing_is_sorted_by_name_whatever_the_order_of_registration(throwaway_effects):
    def zz_order_probe(rgb, params):
        """Registered first."""
        return rgb

    def aa_order_probe(rgb, params):
        """Registered second."""
        return rgb

    effect(category="utilities", params=GainParams)(zz_order_probe)
    effect(category="utilities", params=GainParams)(aa_order_probe)
    for listing in (list(registry.all_effects()), list(registry.describe()), registry.categories()["utilities"]):
        assert listing == sorted(listing)
        assert listing.index("aa_order_probe") < listing.index("zz_order_probe")


def test_threads_making_the_first_query_together_all_see_every_effect():
    """The first query loads the effects once, under a lock: no thread sees a half-loaded table. Run in a fresh
    interpreter, where nothing has been loaded yet; loading takes long enough for an unlocked race to show."""
    code = textwrap.dedent("""
        import json, sys, threading
        from photo_fx.core import registry

        start = threading.Barrier(16)
        seen = []

        def first_query():
            start.wait()
            seen.append(sorted(registry.all_effects()))

        threads = [threading.Thread(target=first_query) for _ in range(16)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        json.dump(seen, sys.stdout)
    """)
    done = subprocess.run([sys.executable, "-c", code], capture_output=True, encoding="utf-8", check=False)
    assert done.returncode == 0, done.stderr
    seen = json.loads(done.stdout)
    assert len(seen) == 16
    assert {"glow", "grade", "halftone"} <= set(seen[0])  # the real effects, loaded by the first query
    assert all(names == seen[0] for names in seen)
