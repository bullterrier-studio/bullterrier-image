"""Registering an effect with a parameter model: the rules, the wrapper it becomes, and the registry's queries.

Every effect here is the test's own (`throwaway_effects` removes it again), so nothing depends on the real effects.
"""

from typing import Any

import numpy as np
import pytest
from pydantic import BaseModel

from photo_fx import effects
from photo_fx.core import registry
from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import INPUTS, effect


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
    np.testing.assert_allclose(effects.apply("gain_probe", GREY, gain=3.0), 0.75, atol=1e-6)


def test_giving_both_a_model_and_keywords_is_an_error(gain):
    with pytest.raises(TypeError, match="gain_probe"):
        gain(GREY, GainParams(gain=2.0), offset=0.1)


def test_a_model_of_another_effect_is_an_error(gain):
    class OtherParams(EffectParams):
        """Another model."""

    with pytest.raises(TypeError, match="GainParams"):
        gain(GREY, OtherParams())


def test_an_unknown_keyword_names_the_effect_and_lists_the_parameters(gain):
    with pytest.raises(ValueError, match="gian") as raised:
        gain(GREY, gian=2.0)
    message = str(raised.value)
    assert "gain_probe" in message
    assert "gain, offset" in message


def test_an_out_of_range_keyword_names_the_effect_the_field_the_limit_and_the_value(gain):
    with pytest.raises(ValueError, match="gain_probe") as raised:
        gain(GREY, gain=9.0)
    message = str(raised.value)
    assert "`gain`" in message
    assert "<= 4" in message
    assert "9.0" in message


def test_the_wrapper_keeps_the_name_and_documentation_and_tells_its_model_category_and_inputs(gain):
    assert gain.__name__ == "gain_probe"
    assert gain.__doc__ == gain_probe.__doc__
    assert gain.params_model is GainParams
    assert gain.category == "light"
    assert gain.inputs == ()
    assert effects.get("gain_probe") is gain


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


def test_inputs_are_the_agreed_list():
    assert INPUTS == ("matte",)


# --- registration rules: each raises and registers nothing -----------------------------------------------------------


def _probe_with(signature: str):
    namespace: dict[str, Any] = {}
    exec(f"def shape_probe{signature}:\n    return rgb", namespace)  # a function with exactly the signature under test
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
    assert "shape_probe" not in effects.describe()


@pytest.mark.parametrize("model", [dict, GainParams(), BaseModel], ids=["dict", "instance", "basemodel"])
def test_params_must_be_an_effect_params_model(model, throwaway_effects):
    with pytest.raises(ValueError, match=r"gain_probe.*EffectParams"):
        effect(category="light", params=model)(gain_probe)
    assert "gain_probe" not in effects.describe()


def test_a_parameter_may_not_be_named_like_an_input(throwaway_effects):
    class MatteParams(EffectParams):
        """Claims the matte as a parameter."""

        matte: float = param(0.5, unit="fraction", min=0, max=1, doc="Not allowed: the recipe supplies the matte.")

    def clash_probe(rgb, params, *, matte=None):
        return rgb

    with pytest.raises(ValueError, match=r"clash_probe.*`matte`"):
        effect(category="light", params=MatteParams)(clash_probe)
    assert "clash_probe" not in effects.describe()


def test_an_unknown_category_is_refused_for_a_modelled_effect_too(throwaway_effects):
    with pytest.raises(ValueError, match="'lights'"):
        effect(category="lights", params=GainParams)(gain_probe)
    assert "gain_probe" not in effects.describe()


def test_registering_an_effect_again_keeps_the_very_same_effect(gain):
    assert effect(category="light")(gain) is gain
    assert effect(category="light", params=GainParams)(gain) is gain
    assert effects.get("gain_probe") is gain


def test_registering_an_effect_again_with_another_model_is_refused(gain):
    class OtherParams(EffectParams):
        """Another model."""

    with pytest.raises(ValueError, match=r"gain_probe.*OtherParams"):
        effect(category="light", params=OtherParams)(gain)
    assert effects.get("gain_probe").params_model is GainParams  # type: ignore[attr-defined]  # a modelled effect


def test_the_legacy_keyword_style_still_registers_unchanged(throwaway_effects):
    """TRANSITIONAL: until every effect module declares a model, a plain keyword-argument effect still registers as
    it is. Delete with the legacy path."""

    def legacy_probe(rgb, amount: float = 0.5, matte=None):
        """Old style."""
        return rgb

    assert effect(category="utilities")(legacy_probe) is legacy_probe
    assert registry.parameters("legacy_probe") is None
    assert registry.schema("legacy_probe") is None
    assert registry.inputs("legacy_probe") == ("matte",)


# --- queries ---------------------------------------------------------------------------------------------------------


def test_parameters_and_schema_describe_the_model(gain):
    assert registry.parameters("gain_probe") is GainParams
    properties = registry.schema("gain_probe")["properties"]  # type: ignore[index]  # a modelled effect has one
    assert properties["gain"]["unit"] == "ratio"
    assert properties["offset"]["description"] == "Added after the gain."


def test_validate_returns_the_model_or_explains_what_is_wrong(gain):
    assert registry.validate("gain_probe", {"gain": 2}) == GainParams(gain=2.0)
    with pytest.raises(ValueError, match=r"gain_probe.*`offset`.*>= -1"):
        registry.validate("gain_probe", {"offset": -2})


def test_validate_checks_a_legacy_effects_parameter_names(throwaway_effects):
    """TRANSITIONAL: a legacy effect has no model, so only its parameter names can be checked."""

    @effect(category="utilities")
    def legacy_names(rgb, amount: float = 0.5):
        """Old style."""
        return rgb

    assert registry.validate("legacy_names", {"amount": 0.2}) is None
    with pytest.raises(ValueError, match="amuont"):
        registry.validate("legacy_names", {"amuont": 0.2})


def test_describe_gives_the_documentation_and_every_parameter(gain):
    text = effects.describe()["gain_probe"]
    assert text.startswith("Brightens by a gain, then shifts by an offset. More text.")
    assert "Parameters" in text
    assert "Multiplies every value. Raising it brightens." in text
    assert "Added after the gain." in text
    assert effects.summary("gain_probe") == "Brightens by a gain, then shifts by an offset."


def test_queries_about_an_unknown_effect_list_the_known_ones(gain):
    for query in (registry.parameters, registry.schema, registry.inputs):
        with pytest.raises(KeyError, match="gain_probe"):
            query("no_such_effect")
