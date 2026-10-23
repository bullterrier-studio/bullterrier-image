import ast
import inspect
import json
import pydoc
from typing import Literal

import numpy as np
import pytest

import photo_fx
import photo_fx.core.runner
import photo_fx.effects
from photo_fx import Project, Recipe, Step, fx, imageio, load_recipe, ramp, render, run, trace
from photo_fx.cli import main
from photo_fx.core import registry
from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.core.timing import Animated, Animation

# --- the public surface: explicit, one home per name ------------------------------------------------------------


@pytest.mark.parametrize(
    "module", [photo_fx, photo_fx.fx, photo_fx.imageio, photo_fx.recipe, photo_fx.core], ids=lambda m: m.__name__
)
def test_every_public_module_lists_what_it_exports_and_has_it(module):
    assert isinstance(module.__all__, list)
    assert module.__all__, f"{module.__name__} exports nothing"
    missing = [name for name in module.__all__ if not hasattr(module, name)]
    assert missing == []


@pytest.mark.parametrize("name", ["render", "render_frames", "trace", "render_all"])
def test_the_render_family_has_one_public_home_and_core_does_not_offer_its_own(name):
    """`photo_fx.render` is `photo_fx.recipe.render` (with the default subject finder); the core's version, which
    finds no subject by itself, is reachable only by its module path."""
    assert getattr(photo_fx, name) is getattr(photo_fx.recipe, name)
    assert getattr(photo_fx, name) is not getattr(photo_fx.core.runner, name)
    assert not hasattr(photo_fx.core, name)


@pytest.mark.parametrize("name", ["get", "apply", "describe", "categories", "summary", "CATEGORIES"])
def test_effect_queries_live_only_in_the_registry(name):
    assert hasattr(registry, name)
    assert not hasattr(photo_fx.effects, name)


# --- steps are checked when they are made, wherever they come from ------------------------------------------------


@pytest.mark.parametrize(
    ("make", "expect"),
    [
        (lambda: Step("sparkle"), "unknown effect.*ascii"),
        (lambda: Step("glitch", {"spilt": 3}), "spilt"),
        (lambda: Step("grain", mask="sideways"), "mask.*'sideways'"),
        (lambda: Step("grain", opacity=1.5), "opacity.*1.5"),
        (lambda: Step("grain", opacity=True), "opacity"),
        (lambda: Step("evaporate", {"matte": 1}), "matte"),
    ],
)
def test_a_step_built_in_code_fails_at_once_with_a_useful_message(make, expect):
    with pytest.raises(ValueError, match=expect):
        make()


def test_a_recipe_built_in_code_is_checked_too():
    with pytest.raises(ValueError, match="at least one step"):
        Recipe("empty", [])
    with pytest.raises(ValueError, match="no animation"):
        Recipe("x", [fx.grade(exposure=ramp(0, 1))])
    assert Recipe("ok", [fx.grain()]).steps == (fx.grain(),)  # a list is fine; it becomes a tuple


def test_steps_can_be_limited_to_a_region_and_blended_without_changing_the_original():
    base = fx.fill("#000000")
    masked = base.where("background").blend(0.5)
    assert (masked.mask, masked.opacity) == ("background", 0.5)
    assert (base.mask, base.opacity) == (None, 1.0)
    with pytest.raises(ValueError, match="left half"):
        base.where("left half")
    with pytest.raises(ValueError, match="opacity"):
        base.blend(2)


def test_a_step_describes_itself_briefly_for_labels():
    assert str(fx.fill("#000000").where("background")) == "fill(color='#000000') on background"
    assert str(fx.grade(exposure=0.5).blend(0.3)) == "grade(exposure=0.5) x0.3"
    assert str(fx.disperse(progress=ramp(0, 0.9))) == "disperse(progress=0->0.9)"
    assert len(fx.ascii(chars="xy" * 100).describe(width=30)) == 30


# --- fx: the effects as step builders ------------------------------------------------------------------------------


def test_fx_builds_steps_from_keyword_or_positional_arguments():
    assert fx.glitch(split=3, seed=2) == Step("glitch", {"split": 3, "seed": 2})
    assert fx.fill("#fff") == Step("fill", {"color": "#fff"})  # positional: the first parameter after the image
    assert fx.grain() == Step("grain", {})


def test_fx_reports_a_misspelt_parameter_when_the_step_is_made():
    with pytest.raises(ValueError, match=r"glitch.*`spilt` is not a parameter"):
        fx.glitch(spilt=3)
    with pytest.raises(AttributeError, match=r"available: .*ascii"):
        fx.sparkle  # noqa: B018  (the lookup itself is what must raise)


def test_fx_functions_document_themselves_and_can_be_found_in_a_repl():
    signature = inspect.signature(fx.glitch)
    assert list(signature.parameters)[:3] == ["split", "bands", "shift"]  # no image parameter, no matte
    assert "matte" not in inspect.signature(fx.evaporate).parameters
    assert "slide sideways" in str(fx.glitch.__doc__)
    assert fx.glitch.__name__ == "glitch"
    assert {"ascii", "glitch", "disperse"} <= set(dir(fx))


def test_every_effect_is_available_as_a_step_builder():
    for name in registry.describe():
        assert getattr(fx, name)().effect == name


class LanternParams(EffectParams):
    """The lantern probe's settings."""

    radius: float = param(24.0, unit="px", min=0, max=500, doc="Glow radius of the paper lantern, quite distinctive.")
    style: Literal["paper", "silk"] = param("paper", doc="Shade: 'paper' is matte, 'silk' shines like a moth wing.")
    tint: Color = param("#ffaa00", doc="Lantern tint, the colour of candle light through rice paper.")


@pytest.fixture
def lantern(throwaway_effects):
    def lantern_probe(rgb: np.ndarray, params: LanternParams, *, matte: np.ndarray | None = None) -> np.ndarray:
        """Hangs a lantern over the picture. Then more words."""
        return rgb

    return effect(category="light", params=LanternParams)(lantern_probe)


def test_help_on_a_modelled_effects_builder_shows_every_parameters_description(lantern):
    text = pydoc.plain(pydoc.render_doc(fx.lantern_probe))  # what help() prints, without the bold overstrikes
    assert "Hangs a lantern over the picture." in text
    for description in (
        "Glow radius of the paper lantern, quite distinctive.",
        "Shade: 'paper' is matte, 'silk' shines like a moth wing.",
        "Lantern tint, the colour of candle light through rice paper.",
    ):
        assert description in " ".join(text.split()), description  # help() may wrap long lines


def test_a_modelled_effects_builder_has_the_models_signature(lantern):
    signature = inspect.signature(fx.lantern_probe)
    assert list(signature.parameters) == ["radius", "style", "tint"]  # no image, no matte
    assert [p.default for p in signature.parameters.values()] == [24.0, "paper", "#ffaa00"]
    assert "tint: str | list[float] | tuple[float, float, float]" in str(signature)


def test_a_modelled_effects_builder_takes_positional_and_keyword_values(lantern):
    assert fx.lantern_probe(30, style="silk") == Step("lantern_probe", {"radius": 30, "style": "silk"})
    assert fx.lantern_probe() == Step("lantern_probe")
    with pytest.raises(TypeError, match="at most 3"):
        fx.lantern_probe(1, "silk", "#000", 4)
    with pytest.raises(TypeError, match="radius"):
        fx.lantern_probe(30, radius=31)


def test_a_modelled_effects_builder_refuses_bad_values_like_a_recipe_file(lantern):
    with pytest.raises(ValueError, match=r"lantern_probe.*`raduis`.*radius, style, tint"):
        fx.lantern_probe(raduis=3)
    with pytest.raises(ValueError, match=r"lantern_probe.*`radius`.*>= 0.*-1"):
        fx.lantern_probe(radius=-1)
    with pytest.raises(ValueError, match=r"`tint`.*'#zz0000'"):
        fx.lantern_probe(tint="#zz0000")


def test_the_editor_stub_shows_a_modelled_effects_fields_and_documentation(lantern):
    tree = ast.parse(fx.stub_text())
    (function,) = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "lantern_probe"]
    assert [a.arg for a in function.args.args] == ["radius", "style", "tint"]
    assert ast.unparse(function.args.args[1].annotation) == "Literal['paper', 'silk']"  # type: ignore[arg-type]
    doc = ast.get_docstring(function)
    assert doc is not None
    assert "Glow radius of the paper lantern, quite distinctive." in doc
    assert "pydantic" not in fx.stub_text()


def test_the_editor_stub_is_valid_python_with_one_function_per_effect(lantern):
    """fx.pyi itself is regenerated at integration (`poe regenerate`), so only its generator is checked here."""
    tree = ast.parse(fx.stub_text())
    functions = {node.name: node for node in tree.body if isinstance(node, ast.FunctionDef)}
    assert "lantern_probe" in functions
    assert sorted(functions) == sorted(registry.describe())  # one per effect, no more, no less


class OldParams(EffectParams):
    """The first version's settings."""

    old: float = param(1.0, unit="ratio", min=0, max=2, doc="Unused.")


class NewParams(EffectParams):
    """The second version's settings."""

    new: float = param(1.0, unit="ratio", min=0, max=2, doc="Unused.")


def test_fx_follows_the_effect_registered_under_a_name_now(throwaway_effects):
    from photo_fx.core.registry import effect, unregister

    @effect(category="utilities", params=OldParams)
    def renamed_probe(rgb, params):
        """First version."""
        return rgb

    assert list(inspect.signature(fx.renamed_probe).parameters) == ["old"]
    unregister("renamed_probe")

    # a different effect under the same name, on purpose (mypy reports a redefinition at the decorator)
    @effect(category="utilities", params=NewParams)  # type: ignore[no-redef]  # the point of the test
    def renamed_probe(rgb, params):  # noqa: F811  (the point of the test)
        """Second version."""
        return rgb

    assert list(inspect.signature(fx.renamed_probe).parameters) == ["new"]
    assert fx.renamed_probe(new=2.0) == Step("renamed_probe", {"new": 2.0})


def test_python_and_toml_recipes_are_the_same_thing(tmp_path):
    (tmp_path / "r.toml").write_text(
        'name = "r"\nsize = 64\n[[step]]\neffect = "fill"\ncolor = "#000000"\nmask = "background"\n'
        '[[step]]\neffect = "glitch"\nsplit = 4\nseed = 1\n[[step]]\neffect = "glow"\nopacity = 0.5\n'
    )
    built = Recipe(
        "r", size=64, steps=[fx.fill("#000000").where("background"), fx.glitch(split=4, seed=1), fx.glow().blend(0.5)]
    )
    assert built == load_recipe(tmp_path / "r.toml")


def test_ramp_makes_an_animated_value():
    assert ramp(0, 1, "gentle") == Animated(0.0, 1.0, "gentle")
    assert ramp(0, 1, [0.4, 0, 0.2, 1]).ease == (0.4, 0, 0.2, 1)
    assert ramp(0, 5, over="clip").over == "clip"


# --- trace: the picture after every step -----------------------------------------------------------------------


def disc(shape, radius=25):
    y, x = np.mgrid[0 : shape[0], 0 : shape[1]]
    return (np.hypot(x - shape[1] / 2, y - shape[0] / 2) <= radius).astype(np.float32)


def test_trace_gives_the_input_and_the_picture_after_each_step_in_order(photo):
    look = Recipe("t", size=None, steps=[fx.grade(exposure=0.5), fx.glitch(seed=1), fx.grain(seed=2)])
    stages = trace(photo, look)
    assert [s.label.split("(")[0] for s in stages] == ["input", "1. grade", "2. glitch", "3. grain"]
    np.testing.assert_array_equal(stages[0].image, photo)
    assert not np.array_equal(stages[1].image, stages[0].image)
    assert not np.array_equal(stages[3].image, stages[2].image)


def test_the_last_stage_of_a_trace_is_exactly_what_render_returns(photo):
    matte = disc(photo.shape[:2])
    look = Recipe("t", size=64, steps=[fx.gradient_map(), fx.fill("#102030").where("background"), fx.glow().blend(0.7)])
    np.testing.assert_array_equal(trace(photo, look, matte)[-1].image, render(photo, look, matte))


def test_trace_shows_the_subject_matte_when_the_recipe_uses_one(photo):
    matte = disc(photo.shape[:2])
    stages = trace(photo, Recipe("t", size=None, steps=[fx.fill("#000000").where("background")]), matte)
    assert [s.label.split(" (")[0] for s in stages][:2] == ["input", "subject matte"]
    np.testing.assert_allclose(stages[1].image[..., 0], matte)  # white where the subject is
    assert "subject matte" not in " ".join(s.label for s in trace(photo, Recipe("t", [fx.grain()], size=None)))


def test_trace_of_an_animation_is_a_still_from_the_chosen_moment(photo):
    look = Recipe("a", size=None, animation=Animation(seconds=2), steps=[fx.grade(exposure=ramp(0, 1))])
    start, end = trace(photo, look, at=0.0)[-1].image, trace(photo, look, at=1.0)[-1].image
    np.testing.assert_allclose(start, photo, atol=1e-6)
    np.testing.assert_allclose(end, np.clip(photo * 2, 0, 1), atol=1e-5)


# --- Python recipes and per-step output in the pipeline ---------------------------------------------------------


def make_project(tmp_path, photo, recipes, **options):
    (tmp_path / "photos").mkdir(exist_ok=True)
    imageio.save(photo, tmp_path / "photos" / "one.png")
    return Project(
        photos=(tmp_path / "photos" / "one.png",), recipes=tuple(recipes), output=tmp_path / "out", **options
    )


def test_the_pipeline_runs_recipes_defined_in_code_and_notices_when_they_change(tmp_path, photo):
    a = Recipe("a", size=64, steps=[fx.grain(seed=1)])
    project = make_project(tmp_path, photo, [a])
    assert [i.status for i in run(project)] == ["done"]
    assert (tmp_path / "out" / "one" / "a.jpg").is_file()
    assert [i.status for i in run(project)] == ["skipped"]
    changed = Recipe("a", size=64, steps=[fx.grain(seed=2)])
    assert [i.status for i in run(make_project(tmp_path, photo, [changed]))] == ["done"]


def test_steps_option_saves_the_picture_after_every_step_and_links_it_in_the_report(tmp_path, photo):
    look = Recipe("look", size=64, steps=[fx.grade(exposure=0.4), fx.glitch(seed=1)])
    project = make_project(tmp_path, photo, [look], steps=True)
    (item,) = run(project)
    folder = tmp_path / "out" / "one" / "steps" / "look"
    assert sorted(p.name for p in folder.iterdir()) == ["00-input.jpg", "01-grade.jpg", "02-glitch.jpg", "_sheet.jpg"]
    assert item.steps == "one/steps/look/_sheet.jpg"
    final = imageio.load(tmp_path / "out" / "one" / "look.jpg")
    assert np.abs(final - imageio.load(folder / "02-glitch.jpg")).max() < 0.03  # the last step is the exported result
    assert 'href="one/steps/look/_sheet.jpg"' in (tmp_path / "out" / "index.html").read_text()
    manifest = json.loads((tmp_path / "out" / "manifest.json").read_text())
    assert manifest["items"][0]["steps"] == "one/steps/look/_sheet.jpg"


def test_turning_steps_on_redoes_finished_work_so_the_pictures_exist(tmp_path, photo):
    look = Recipe("look", size=64, steps=[fx.grain(seed=1)])
    assert [i.status for i in run(make_project(tmp_path, photo, [look]))] == ["done"]
    assert [i.status for i in run(make_project(tmp_path, photo, [look], steps=True))] == ["done"]


def test_the_steps_command_writes_one_picture_per_step_and_a_sheet(tmp_path, photo, capsys):
    imageio.save(photo, tmp_path / "p.png")
    (tmp_path / "r.toml").write_text(
        'name = "r"\nsize = 64\n[[step]]\neffect = "grade"\nexposure = 0.2\n[[step]]\neffect = "grain"\n'
    )
    assert main(["steps", str(tmp_path / "p.png"), str(tmp_path / "r.toml"), "-o", str(tmp_path / "s")]) == 0
    assert sorted(p.name for p in (tmp_path / "s").iterdir()) == [
        "00-input.jpg",
        "01-grade.jpg",
        "02-grain.jpg",
        "_sheet.jpg",
    ]
    assert "1. grade(exposure=0.2)" in capsys.readouterr().err
