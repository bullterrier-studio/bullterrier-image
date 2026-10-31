"""Registering an effect with a parameter model: the rules, the wrapper it becomes, and the registry's queries.

Every effect here is the test's own (`throwaway_effects` removes it again), so nothing depends on the real effects.
"""

import importlib
import inspect
import json
import re
import subprocess
import sys
import textwrap
from typing import Any

import numpy as np
import pytest
from pydantic import BaseModel

from bullterrier_image.core import looks, registry
from bullterrier_image.core.params import EffectParams, FieldName, param
from bullterrier_image.core.registry import effect
from bullterrier_image.errors import MapNotFoundError, UnknownEffectError


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


def test_an_effect_that_declares_size_ratio_gets_it_by_keyword_and_1_when_a_call_gives_none(throwaway_effects):
    def ratio_probe(rgb: np.ndarray, params: GainParams, *, size_ratio: float = 1.0) -> np.ndarray:
        """Fills the picture with its size ratio over 10."""
        return np.full_like(rgb, size_ratio / 10)

    probe = effect(category="light", params=GainParams)(ratio_probe)
    np.testing.assert_allclose(probe(GREY, size_ratio=2.0), 0.2)
    np.testing.assert_allclose(registry.apply("ratio_probe", GREY, size_ratio=3.0, gain=2.0), 0.3)
    np.testing.assert_allclose(probe(GREY), 0.1)
    assert registry.inputs("ratio_probe") == ("size_ratio",)


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
        ("(rgb, params, size_ratio=1.0)", "keyword-only"),
        ("(rgb, params, *, glow=None)", "'glow'"),
        ("(rgb, params, *, subject=None)", "'subject'"),
        ("(rgb, params, *, size_ratio)", "default"),
    ],
    ids=[
        "no-params",
        "extra-positional",
        "args",
        "kwargs",
        "positional-input",
        "unknown-input",
        "a map is a field, not an input",
        "input-no-default",
    ],
)
def test_an_effect_function_of_the_wrong_shape_is_refused(signature, complaint, throwaway_effects):
    with pytest.raises(ValueError, match="shape_probe") as raised:
        effect(category="light", params=GainParams)(_probe_with(signature))
    assert complaint in str(raised.value)
    assert "shape_probe" not in registry.describe()


class ShadeParams(EffectParams):
    """A field and a gain: the probe with a field parameter."""

    field: FieldName = param("depth", doc="The selection that darkens the picture.")
    gain: float = param(1.0, unit="ratio", min=0, max=4, doc="Multiplies every value.")


@pytest.mark.parametrize(
    ("signature", "complaint"),
    [
        ("(rgb, params)", "keyword-only argument `field`"),
        ("(rgb, params, *, field=None)", "no default"),
        ("(rgb, params, *, size_ratio=1.0)", "keyword-only argument `field`"),
    ],
    ids=["no argument", "a default", "another input instead"],
)
def test_a_field_parameter_needs_a_keyword_only_argument_of_its_name_without_a_default(
    signature, complaint, throwaway_effects
):
    with pytest.raises(ValueError, match="shape_probe") as raised:
        effect(category="light", params=ShadeParams)(_probe_with(signature))
    assert complaint in str(raised.value)
    assert "shape_probe" not in registry.describe()


@pytest.fixture
def shade(throwaway_effects):
    @effect(category="light", params=ShadeParams)
    def shade_probe(rgb, params, *, field):
        """Darkens the picture by a field, then multiplies by a gain."""
        return np.clip(rgb * (1 - field[..., None] / 2) * params.gain, 0, 1)  # float32 only if the field is

    return shade_probe


def test_a_direct_call_gives_the_field_as_an_array_whatever_its_float_type(shade):
    field = np.full(GREY.shape[:2], 0.5)  # float64: the effect still gets a float32
    out = shade(GREY, gain=2.0, field=field)
    np.testing.assert_allclose(out, 0.25 * 0.75 * 2.0, atol=1e-7)
    assert out.dtype == np.float32
    assert inspect.signature(shade).parameters["field"].default is inspect.Parameter.empty


def test_a_direct_call_with_a_misaligned_picture_gives_the_result_for_the_aligned_one(photo, misaligned):
    grade = registry.get("grade")  # an effect the Rust core renders, which reads the picture in place
    expected = grade(photo, exposure=0.5)
    np.testing.assert_array_equal(grade(misaligned(photo), exposure=0.5), expected)
    assert not np.array_equal(expected, photo)


def test_an_effect_is_given_its_picture_and_field_aligned_whatever_the_caller_passed(throwaway_effects, misaligned):
    arrived: dict[str, np.ndarray] = {}

    @effect(category="light", params=ShadeParams)
    def shade_probe(rgb, params, *, field):
        """Records the arrays it is given."""
        arrived.update(rgb=rgb, field=field)
        return rgb

    picture = np.full((4, 5, 3), 0.25, np.float32)
    shade_probe(misaligned(picture), field=misaligned(np.full((4, 5), 0.5, np.float32)))
    for name, array in arrived.items():
        assert array.flags.aligned, name
        assert array.flags.c_contiguous, name
    np.testing.assert_array_equal(arrived["field"], 0.5)


def test_a_picture_that_is_already_in_the_layout_is_given_to_the_effect_without_a_copy(throwaway_effects):
    arrived: list[np.ndarray] = []

    @effect(category="light", params=ShadeParams)
    def shade_probe(rgb, params, *, field):
        """Records the picture it is given."""
        arrived.append(rgb)
        return rgb

    picture = np.full((4, 5, 3), 0.25, np.float32)
    shade_probe(picture, field=np.full((4, 5), 0.5, np.float32))
    assert np.shares_memory(arrived[0], picture)


@pytest.mark.parametrize(
    "field",
    [
        pytest.param(None, id="none given"),
        pytest.param("light", id="a name: only a recipe can resolve it"),
        pytest.param(np.zeros((4, 4), np.float32), id="not the picture's size"),
        pytest.param(np.full((4, 5), 1.5, np.float32), id="above 1"),
        pytest.param(np.full((4, 5), np.nan, np.float32), id="not finite"),
        pytest.param(np.zeros((4, 5), np.int32), id="not a float array"),
    ],
)
def test_a_direct_call_without_a_usable_field_array_is_a_map_not_found_error_naming_the_parameter(field, shade):
    given = {} if field is None else {"field": field}
    with pytest.raises(MapNotFoundError, match="`field`"):
        shade(GREY, **given)


@pytest.mark.parametrize("model", [dict, GainParams(), BaseModel], ids=["dict", "instance", "basemodel"])
def test_params_must_be_an_effect_params_model(model, throwaway_effects):
    with pytest.raises(ValueError, match=r"gain_probe.*EffectParams"):
        effect(category="light", params=model)(gain_probe)
    assert "gain_probe" not in registry.describe()


@pytest.mark.parametrize(
    "name",
    [
        "partition",
        "size_ratio",
        "effect",
        "mask",
        "opacity",
        "blend_mode",
        "preset",
        "each",
        "looks",
    ],
)
def test_a_parameter_may_not_be_named_like_an_input_or_a_step_key(name, throwaway_effects):
    field = param(0.5, unit="fraction", min=0, max=1, doc="Not allowed: the recipe supplies it.")
    clashing = type(
        "ClashParams", (EffectParams,), {"__doc__": "Claims a name.", "__annotations__": {name: float}, name: field}
    )

    def clash_probe(rgb, params):
        return rgb

    with pytest.raises(ValueError, match=rf"clash_probe.*`{name}`"):
        effect(category="light", params=clashing)(clash_probe)
    assert "clash_probe" not in registry.describe()


@pytest.mark.parametrize("opacity", [1.5, -0.1, float("nan"), True, "0.5"])
def test_a_step_opacity_outside_0_to_1_is_refused_at_registration(opacity, throwaway_effects):
    with pytest.raises(ValueError, match=rf"gain_probe.*opacity.*{re.escape(repr(opacity))}"):
        effect(category="light", params=GainParams, opacity=opacity)(gain_probe)
    assert "gain_probe" not in registry.describe()


def test_an_effect_may_declare_the_opacity_of_its_steps_and_its_documentation_says_so(gain, throwaway_effects):
    @effect(category="light", params=GainParams, opacity=0.4)
    def faint_probe(rgb, params):
        """Brightens faintly."""
        return rgb

    assert (registry.opacity("faint_probe"), registry.opacity("gain_probe")) == (0.4, 1.0)
    assert "\nStep opacity\n------------\n0.4" in registry.documentation("faint_probe")
    assert "Step opacity" not in registry.documentation("gain_probe")  # 1, the rule: nothing to say


@pytest.mark.parametrize("category", ["lights", "Light", ""], ids=["misspelt", "capitalised", "empty"])
def test_an_unknown_category_is_refused_naming_it_and_listing_the_valid_ones(category, throwaway_effects):
    with pytest.raises(ValueError, match=rf"gain_probe.*{category!r}.*colour, light, texture, .*, utilities"):
        effect(category=category, params=GainParams)(gain_probe)
    assert "gain_probe" not in registry.describe()


class CoreGradeParams(EffectParams, core="grade"):
    pass


def probe_for_the_core(rgb: np.ndarray, params: CoreGradeParams) -> np.ndarray:
    """Stands in for grade."""
    return rgb


@pytest.mark.parametrize("given", [{"category": "colour"}, {"opacity": 0.5}], ids=["category", "opacity"])
def test_an_effect_the_core_runs_takes_its_category_and_opacity_from_the_core_and_refuses_its_own(
    given, throwaway_effects
):
    def grade(rgb: np.ndarray, params: CoreGradeParams) -> np.ndarray:
        return rgb

    with pytest.raises(ValueError, match=r"grade.*core.*declares"):
        effect(params=CoreGradeParams, **given)(grade)


def test_an_effect_the_core_runs_is_named_like_its_core_effect(throwaway_effects):
    with pytest.raises(ValueError, match=r"probe_for_the_core.*'grade'"):
        effect(params=CoreGradeParams)(probe_for_the_core)
    assert "probe_for_the_core" not in registry.describe()


def test_an_effect_with_a_python_model_needs_a_category(throwaway_effects):
    with pytest.raises(ValueError, match=r"gain_probe.*category"):
        effect(params=GainParams)(gain_probe)
    assert "gain_probe" not in registry.describe()


def test_registering_an_effect_again_keeps_the_very_same_effect_with_what_it_now_declares(gain):
    assert effect(category="colour", params=GainParams, opacity=0.3)(gain) is gain
    assert registry.get("gain_probe") is gain
    assert (registry.category("gain_probe"), registry.opacity("gain_probe")) == ("colour", 0.3)


def test_registering_the_same_function_twice_is_allowed(throwaway_effects):
    effect(category="utilities", params=GainParams)(gain_probe)
    again = effect(category="utilities", params=GainParams)(gain_probe)
    assert registry.get("gain_probe") is again


def test_a_second_function_under_an_existing_name_is_an_error_naming_both_modules(throwaway_effects):
    def twin(rgb, params):
        """First."""
        return rgb

    first = twin
    first.__module__ = "bullterrier_image.effects.first_module"

    def twin(rgb, params):  # type: ignore[no-redef]  # the point of the test: a second function, same name
        """Second."""
        return 1 - rgb

    twin.__module__ = "bullterrier_image.effects.second_module"
    registered = effect(category="utilities", params=GainParams)(first)
    with pytest.raises(ValueError, match=r"twin.*first_module.*second_module"):
        effect(category="utilities", params=GainParams)(twin)
    assert registry.get("twin") is registered  # the first one stays
    np.testing.assert_array_equal(registry.apply("twin", GREY), GREY)


def test_reloading_an_effects_module_is_not_a_duplicate(tmp_path, monkeypatch, throwaway_effects):
    (tmp_path / "reload_probe_module.py").write_text(
        "from bullterrier_image.core.params import EffectParams\n"
        "from bullterrier_image.core.registry import effect\n\n"
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
    properties = registry.parameters("gain_probe").model_json_schema()["properties"]
    assert properties["gain"]["unit"] == "ratio"
    assert properties["offset"]["description"] == "Added after the gain."


def test_validate_returns_the_model_or_explains_what_is_wrong(gain):
    assert registry.get("gain_probe").validate({"gain": 2}) == GainParams(gain=2.0)
    with pytest.raises(ValueError, match=r"gain_probe.*`offset`.*>= -1"):
        registry.get("gain_probe").validate({"offset": -2})


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


class TunedParams(EffectParams):
    """A gain with a technique option."""

    gain: float = param(1.0, unit="ratio", min=0, max=4, doc="Multiplies every value.")
    softness: float = param(2.0, unit="px", min=0, max=10, advanced=True, doc="Blur of the technique's mask.")


def test_documentation_lists_the_advanced_parameters_unless_told_not_to(throwaway_effects):
    @effect(category="light", params=TunedParams)
    def tuned_probe(rgb, params):
        """Gains, tuned."""
        return rgb

    full, short = registry.documentation("tuned_probe"), registry.documentation("tuned_probe", advanced=False)
    assert full.startswith("Gains, tuned.")
    assert "softness : " in full
    assert short.startswith("Gains, tuned.")
    assert "softness" not in short
    assert "(+1 advanced)" in short


def test_documentation_lists_the_presets_of_an_effect_that_has_some_and_only_then(gain):
    listed = registry.documentation("gradient_map")
    assert "\nPresets\n-------\n" in listed
    for name, preset in registry.presets("gradient_map").items():
        assert f"{name} : {preset.description}" in listed
    assert "Presets" not in registry.documentation("gain_probe")


def test_queries_about_an_unknown_effect_list_the_known_ones(gain):
    for query in (registry.parameters, registry.inputs):
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
        from bullterrier_image.core import registry

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
    assert {"bloom", "grade", "halftone"} <= set(seen[0])  # the real effects, loaded by the first query
    assert all(names == seen[0] for names in seen)


# --- a name that is no effect says where it lives --------------------------------------------------------------------


@pytest.mark.parametrize(
    ("name", "says"),
    [
        pytest.param("glitch", ['fx.slice_shift(preset="glitch")'], id="a-preset-names-its-effect-and-itself"),
        pytest.param("sepia", ['fx.gradient_map(preset="sepia")'], id="a-preset-of-another-effect"),
        pytest.param("crt", ["look", 'load_recipe("crt")'], id="a-look-says-how-to-load-it"),
        pytest.param("vibrance", ["is a parameter of grade"], id="a-parameter-names-its-effect"),
        pytest.param("temperature", ["is a parameter of grade"], id="a-parameter-of-one-effect"),
        pytest.param("radius", ["is a parameter of", "gaussian_blur", "more"], id="a-parameter-of-many-lists-some"),
        pytest.param(
            "depth_of_field", ["summary of focus_blur", "depth of field"], id="a-summary-mention-with-underscores"
        ),
        pytest.param(
            "Tilt-Shift", ["summary of focus_blur", "tilt-shift"], id="a-summary-mention-ignores-case-and-hyphens"
        ),
        pytest.param("glich", ["did you mean", "glitch", "slice_shift"], id="a-misspelt-preset-offers-the-close-match"),
        pytest.param("gradient_mapp", ["did you mean", "gradient_map"], id="a-misspelt-effect-offers-the-close-match"),
    ],
)
def test_an_unknown_effect_name_says_where_the_name_lives(name, says):
    with pytest.raises(UnknownEffectError) as raised:
        registry.get(name)
    message = str(raised.value)
    assert repr(name) in message
    for text in says:
        assert text in message


@pytest.mark.parametrize(
    ("name", "says", "absent"),
    [
        pytest.param(
            "glitch",
            [
                'a preset (fx.slice_shift(preset="glitch"))',
                'a look (load_recipe("glitch"), `bt-image render PHOTO glitch`)',
            ],
            ["parameter"],
            id="a-preset-and-a-look-are-both-named",
        ),
        pytest.param(
            "saturation",
            ["a parameter of", "named in the summary of grade: "],
            ["a preset", "a look"],
            id="a-parameter-and-a-summary-mention-are-both-named",
        ),
        pytest.param("sepia", ['preset="sepia"'], ["a look", "a parameter", "the summary of"], id="only-a-preset"),
    ],
)
def test_a_name_that_is_several_things_lists_all_of_them_and_nothing_else(name, says, absent):
    with pytest.raises(UnknownEffectError) as raised:
        registry.get(name)
    message = str(raised.value)
    for text in says:
        assert text in message
    for text in absent:
        assert text not in message


def test_a_look_is_named_with_its_own_description():
    with pytest.raises(UnknownEffectError) as raised:
        registry.get("glitch")
    assert looks.read()["glitch"].description in str(raised.value)


def test_a_summary_mention_quotes_the_whole_first_sentence_of_the_summary():
    with pytest.raises(UnknownEffectError) as raised:
        registry.get("depth_of_field")
    assert registry.summary("focus_blur") in str(raised.value)
    assert len(registry.summary("focus_blur")) > 90  # longer than any cut would keep


def test_a_name_that_is_nothing_gets_no_location_and_no_close_match():
    with pytest.raises(UnknownEffectError) as raised:
        registry.get("zzqxv")
    message = str(raised.value)
    assert "zzqxv" in message
    assert "preset" not in message
    assert "look" not in message
    assert "did you mean" not in message
    assert "grade" in message  # the effects are listed
