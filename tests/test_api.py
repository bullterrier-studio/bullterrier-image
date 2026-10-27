import ast
import inspect
import json
import pydoc
import re
import subprocess
import sys
from pathlib import Path
from typing import Literal

import numpy as np
import pytest

import bullterrier_image
import bullterrier_image.core.runner
import bullterrier_image.effects
import bullterrier_image.plugin
from bullterrier_image import (
    Grid,
    ImageReadError,
    Pick,
    Project,
    Recipe,
    RecipeError,
    Step,
    fx,
    imageio,
    load_recipe,
    render,
    run,
    trace,
)
from bullterrier_image.cli import main
from bullterrier_image.core import registry
from bullterrier_image.core.params import Color, EffectParams, Picture, param
from bullterrier_image.core.registry import effect

# --- the public surface: explicit, one home per name ------------------------------------------------------------


@pytest.mark.parametrize(
    "module",
    [
        bullterrier_image,
        bullterrier_image.fx,
        bullterrier_image.imageio,
        bullterrier_image.recipe,
        bullterrier_image.plugin,
        bullterrier_image.core,
    ],
    ids=lambda m: m.__name__,
)
def test_every_public_module_lists_what_it_exports_and_has_it(module):
    assert isinstance(module.__all__, list)
    assert module.__all__, f"{module.__name__} exports nothing"
    missing = [name for name in module.__all__ if not hasattr(module, name)]
    assert missing == []


def test_a_recipe_is_framed_with_a_framing_from_the_public_names():
    framing = bullterrier_image.Framing(aspect=1.0, transparent="circle")
    assert bullterrier_image.Recipe("look", [fx.grade()], frame=framing).frame is framing
    assert bullterrier_image.recipe.Framing is bullterrier_image.Framing


def test_the_version_is_looked_up_when_asked_for_not_on_import():
    """Finding the installed version loads the packaging machinery (importlib.metadata), so `import bullterrier_image`
    waits
    until `__version__` is read; it is then there like any other public name."""
    code = (
        "import json, sys\n"
        "import bullterrier_image\n"
        "on_import = 'importlib.metadata' in sys.modules\n"
        "json.dump([on_import, bullterrier_image.__version__, '__version__' in dir(bullterrier_image)], sys.stdout)\n"
    )
    done = subprocess.run([sys.executable, "-P", "-c", code], capture_output=True, encoding="utf-8", check=False)
    assert done.returncode == 0, done.stderr
    on_import, version, listed = json.loads(done.stdout)
    assert on_import is False
    assert re.fullmatch(r"\d+\.\d+\.\d+\S*", version)  # pyproject's version (tests/test_cli.py compares it exactly)
    assert listed is True


@pytest.mark.parametrize("name", ["render", "trace", "render_all"])
def test_the_render_family_has_one_public_home_and_core_does_not_offer_its_own(name):
    """`bullterrier_image.render` is `bullterrier_image.recipe.render` (with the default subject finder); the core's
    version, which
    finds no subject by itself, is reachable only by its module path."""
    assert getattr(bullterrier_image, name) is getattr(bullterrier_image.recipe, name)
    assert getattr(bullterrier_image, name) is not getattr(bullterrier_image.core.runner, name)
    assert not hasattr(bullterrier_image.core, name)


@pytest.mark.parametrize("name", ["get", "apply", "describe", "categories", "summary", "CATEGORIES"])
def test_effect_queries_live_only_in_the_registry(name):
    assert hasattr(registry, name)
    assert not hasattr(bullterrier_image.effects, name)


# --- steps are checked when they are made, wherever they come from ------------------------------------------------


@pytest.mark.parametrize(
    ("make", "expect"),
    [
        (lambda: Step("no_such_effect"), "unknown effect.*ascii"),
        (lambda: Step("rgb_split", {"spilt": 3}), "spilt"),
        (lambda: Step("grain", mask=3), "mask"),  # type: ignore[arg-type]
        (lambda: Step("grain", opacity=1.5), "opacity.*1.5"),
        (lambda: Step("grain", opacity=-0.1), "opacity.*-0.1"),
        (lambda: Step("grain", opacity=True), "opacity"),
        (lambda: Step("evaporate", {"subject": 1}), "subject"),
        (lambda: Step("grain", {"opacity": 0.5}), "`opacity` is a key of the step"),
    ],
)
def test_a_step_built_in_code_fails_at_once_with_a_useful_message(make, expect):
    with pytest.raises(ValueError, match=expect):
        make()


def test_a_recipe_built_in_code_is_checked_too():
    with pytest.raises(ValueError, match="at least one step"):
        Recipe("empty", [])
    assert Recipe("ok", [fx.grain()]).steps == (fx.grain(),)  # a list is fine; it becomes a tuple


@pytest.mark.parametrize(
    ("keys", "complaint"),
    [
        ({"mask": 3}, r"`mask`.*\b3\b"),
        ({"each": "panes"}, r"`each`"),
        ({"looks": ["none"]}, r"`looks`"),
        ({"partition": "panes"}, r"gradient_map.*takes no partition"),
        ({"opacity": 2}, r"`opacity`.*\b2\b"),
        ({"blend_mode": "lighten"}, r"`blend_mode`.*screen.*'lighten'"),
        ({"preset": "sepya"}, r"'sepya'.*sepia"),
        ({"effect": "blur"}, r"`effect`.*'gradient_map'.*'blur'"),
    ],
    ids=[
        "mask",
        "each",
        "looks",
        "partition",
        "opacity",
        "blend_mode",
        "preset",
        "effect: the function already names it",
    ],
)
def test_fx_refuses_a_bad_step_key_naming_it_and_the_value(keys, complaint):
    with pytest.raises(ValueError, match=complaint):
        fx.gradient_map(**keys)


def test_fx_takes_a_region_name_as_mask_and_the_recipe_that_holds_the_step_declares_it():
    step = fx.gradient(colors=["#ff0000"], mask="glass")
    assert Recipe("r", [step], regions={"glass": Pick(of="panes"), "panes": Grid()}).steps == (step,)
    with pytest.raises(RecipeError, match=r"step 1: `mask` names 'glass'.*subject, background.*declares no regions"):
        Recipe("r", [step])  # fx declares no regions: by itself it knows the built-in ones only


def test_the_fx_signature_offers_partition_only_to_an_effect_that_takes_it(partition_probe):
    assert "partition" in inspect.signature(fx.partition_probe).parameters
    assert "partition" not in inspect.signature(fx.grade).parameters
    assert fx.partition_probe(partition="panes") == Step("partition_probe", partition="panes")


def test_a_step_describes_itself_briefly_for_labels():
    assert str(fx.gradient(colors=["#000000"], mask="background")) == "gradient(colors=['#000000']) on background"
    assert str(fx.grade(exposure=0.5, opacity=0.3)) == "grade(exposure=0.5) x0.3"
    assert str(fx.bloom(radius=26, blend_mode="screen")) == "bloom(radius=26) screen"
    assert str(fx.disperse(progress=0.9)) == "disperse(progress=0.9)"
    assert len(fx.ascii(chars="xy" * 100).describe(width=30)) == 30


# --- fx: the effects as step builders ------------------------------------------------------------------------------


def test_fx_builds_steps_from_keyword_or_positional_arguments():
    assert fx.rgb_split(amount=3, angle=2) == Step("rgb_split", {"amount": 3, "angle": 2})
    assert fx.gradient(colors=["#fff"]) == Step(
        "gradient", {"colors": ["#fff"]}
    )  # positional: the first parameter after the image
    assert fx.grain() == Step("grain", {})


def test_fx_reports_a_misspelt_parameter_when_the_step_is_made():
    with pytest.raises(ValueError, match=r"rgb_split.*`spilt` is not a parameter"):
        fx.rgb_split(spilt=3)
    with pytest.raises(AttributeError, match=r"available: .*ascii"):
        fx.no_such_effect  # noqa: B018  (the lookup itself is what must raise)


def test_fx_functions_document_themselves_and_can_be_found_in_a_repl():
    signature = inspect.signature(fx.rgb_split)
    assert list(signature.parameters)[:3] == ["pair", "angle", "amount"]  # no image parameter, no subject
    assert "subject" not in inspect.signature(fx.evaporate).parameters
    assert "along `angle`" in str(fx.rgb_split.__doc__)
    assert fx.rgb_split.__name__ == "rgb_split"
    assert {"ascii", "rgb_split", "disperse"} <= set(dir(fx))


def test_every_effect_is_available_as_a_step_builder():
    for name in registry.describe():
        needs = {"partition": "panes"} if "partition" in registry.inputs(name) else {}  # an effect that draws pieces
        assert getattr(fx, name)(**needs).effect == name


class LanternParams(EffectParams):
    """The lantern probe's settings."""

    radius: float = param(24.0, unit="px", min=0, max=500, doc="Glow radius of the paper lantern, quite distinctive.")
    style: Literal["paper", "silk"] = param("paper", doc="Shade: 'paper' is matte, 'silk' shines like a moth wing.")
    tint: Color = param("#ffaa00", doc="Lantern tint, the colour of candle light through rice paper.")


@pytest.fixture
def lantern(throwaway_effects):
    def lantern_probe(rgb: np.ndarray, params: LanternParams, *, subject: np.ndarray | None = None) -> np.ndarray:
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


def test_a_modelled_effects_builder_has_the_models_signature_then_the_step_keys(lantern):
    signature = inspect.signature(fx.lantern_probe)
    assert list(signature.parameters) == [
        "radius",
        "style",
        "tint",
        "mask",
        "opacity",
        "blend_mode",
        "name",
    ]  # no image, subject
    assert [p.default for p in signature.parameters.values()] == [24.0, "paper", "#ffaa00", None, 1.0, "normal", None]
    assert "tint: str | list[float] | tuple[float, float, float]" in str(signature)
    keyword_only = [p.name for p in signature.parameters.values() if p.kind is inspect.Parameter.KEYWORD_ONLY]
    assert keyword_only == ["mask", "opacity", "blend_mode", "name"]


def test_a_modelled_effects_builder_takes_positional_and_keyword_values(lantern):
    assert fx.lantern_probe(30, style="silk") == Step("lantern_probe", {"radius": 30, "style": "silk"})
    assert fx.lantern_probe() == Step("lantern_probe")
    with pytest.raises(TypeError, match="at most 3"):
        fx.lantern_probe(1, "silk", "#000", 4)
    with pytest.raises(TypeError, match="radius"):
        fx.lantern_probe(30, radius=31)


@pytest.mark.parametrize(
    ("values", "complaint"),
    [
        ({"raduis": 3}, r"`raduis`.*radius, style, tint"),
        ({"radius": -1}, r"`radius`.*>= 0.*-1"),
        ({"tint": "#zz0000"}, r"`tint`.*'#zz0000'"),
    ],
    ids=["unknown", "out of range", "malformed colour"],
)
def test_a_modelled_effects_builder_refuses_bad_values_like_a_recipe_file(lantern, values, complaint):
    with pytest.raises(ValueError, match=f"lantern_probe.*{complaint}"):
        fx.lantern_probe(**values)


def test_the_editor_stub_shows_a_modelled_effects_fields_and_documentation(lantern):
    tree = ast.parse(fx.stub_text())
    (function,) = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "lantern_probe"]
    assert [a.arg for a in function.args.args] == ["radius", "style", "tint"]
    assert ast.unparse(function.args.args[1].annotation) == "Literal['paper', 'silk']"  # type: ignore[arg-type]
    doc = ast.get_docstring(function)
    assert doc is not None
    assert "Glow radius of the paper lantern, quite distinctive." in doc
    assert "pydantic" not in fx.stub_text()


def _stub_functions() -> dict[str, ast.FunctionDef]:
    return {node.name: node for node in ast.parse(fx.stub_text()).body if isinstance(node, ast.FunctionDef)}


MASK = ("str | None", None)
BLEND_MODE = ("Literal['normal', 'multiply', 'screen', 'overlay', 'soft_light', 'difference', 'color']", "normal")


def _keywords(function: ast.FunctionDef) -> dict[str, tuple[str, object]]:
    """A stub function's keyword-only arguments: name -> (annotation, default); the stub writes both for each."""
    pairs = zip(function.args.kwonlyargs, function.args.kw_defaults, strict=True)
    return {a.arg: (ast.unparse(a.annotation), ast.literal_eval(d)) for a, d in pairs}  # type: ignore[arg-type]


def test_the_stub_shows_the_step_keys_and_a_preset_or_a_partition_exactly_where_an_effect_has_one():
    for name, function in _stub_functions().items():
        names = list(registry.presets(name))
        presets = {"preset": (f"Literal{names!r} | None", None)} if names else {}
        step_keys = {"mask": MASK, "opacity": ("float", registry.opacity(name)), "blend_mode": BLEND_MODE}
        partition = {"partition": ("str | None", None)} if "partition" in registry.inputs(name) else {}
        called = {"name": ("str | None", None)}
        assert _keywords(function) == {**step_keys, **partition, **presets, **called}, name
        if names:
            assert "preset" in inspect.signature(getattr(fx, name)).parameters
        else:
            assert "preset" not in inspect.signature(getattr(fx, name)).parameters


def test_a_preset_is_chosen_with_the_keyword_preset_and_the_other_values_win_over_it():
    assert fx.gradient_map(preset="sepia") == Step("gradient_map", preset="sepia")
    assert fx.gradient_map(preset="sepia", shadow_clip=0.2).params["shadow_clip"] == 0.2
    with pytest.raises(ValueError, match=r"gradient_map.*'nope'.*sepia"):
        fx.gradient_map(preset="nope")
    with pytest.raises(ValueError, match=r"equalize.*'sepia'.*no presets"):
        fx.equalize(preset="sepia")


def test_the_editor_stub_is_valid_python_with_one_function_per_effect(lantern):
    """fx.pyi itself is regenerated at integration (`poe regenerate`), so only its generator is checked here."""
    tree = ast.parse(fx.stub_text())
    functions = {node.name: node for node in tree.body if isinstance(node, ast.FunctionDef)}
    assert "lantern_probe" in functions
    assert sorted(functions) == sorted(registry.describe())  # one per effect, no more, no less


class SceneParams(EffectParams):
    """A picture field, for the stub."""

    image: Picture = param("#808080", doc="A picture.")


def test_the_editor_stub_names_a_picture_field_by_its_forms_and_every_name_it_uses_resolves(throwaway_effects):
    from bullterrier_image.core.registry import effect

    def scene_probe(rgb, params: SceneParams):
        """Does nothing."""
        return rgb

    effect(category="compositing", params=SceneParams)(scene_probe)
    text = fx.stub_text()
    (function,) = [n for n in ast.parse(text).body if isinstance(n, ast.FunctionDef) and n.name == "scene_probe"]
    annotation = ast.unparse(function.args.args[0].annotation)  # type: ignore[arg-type]
    assert annotation.startswith("Picture | os.PathLike[str] | numpy.ndarray | str")
    exec(compile(text, "fx.pyi", "exec"), {"__package__": "bullterrier_image"})  # noqa: S102  (our own generated stub)


class OldParams(EffectParams):
    """The first version's settings."""

    old: float = param(1.0, unit="ratio", min=0, max=2, doc="Unused.")


class NewParams(EffectParams):
    """The second version's settings."""

    new: float = param(1.0, unit="ratio", min=0, max=2, doc="Unused.")


def test_fx_follows_the_effect_registered_under_a_name_now(throwaway_effects):
    from bullterrier_image.core.registry import effect, unregister

    @effect(category="utilities", params=OldParams)
    def renamed_probe(rgb, params):
        """First version."""
        return rgb

    assert list(inspect.signature(fx.renamed_probe).parameters)[:1] == ["old"]
    unregister("renamed_probe")

    # a different effect under the same name, on purpose (mypy reports a redefinition at the decorator)
    @effect(category="utilities", params=NewParams)  # type: ignore[no-redef]  # the point of the test
    def renamed_probe(rgb, params):  # noqa: F811  (the point of the test)
        """Second version."""
        return rgb

    assert list(inspect.signature(fx.renamed_probe).parameters)[:1] == ["new"]
    assert fx.renamed_probe(new=2.0) == Step("renamed_probe", {"new": 2.0})


@pytest.mark.parametrize(
    ("toml", "python"),
    [
        pytest.param(
            'effect = "gradient"\ncolors = ["#000000"]\nmask = "background"',
            lambda: fx.gradient(colors=["#000000"], mask="background"),
            id="mask",
        ),
        pytest.param(
            'effect = "bloom"\nradius = 26\nopacity = 0.8', lambda: fx.bloom(radius=26, opacity=0.8), id="opacity"
        ),
        pytest.param(
            'effect = "gradient"\nblend_mode = "screen"', lambda: fx.gradient(blend_mode="screen"), id="blend_mode"
        ),
        pytest.param('effect = "gradient_map"\npreset = "sepia"', lambda: fx.gradient_map(preset="sepia"), id="preset"),
        pytest.param(
            'effect = "rgb_split"\namount = 4\nmask = "subject"\nopacity = 0.5\nblend_mode = "difference"',
            lambda: fx.rgb_split(amount=4, mask="subject", opacity=0.5, blend_mode="difference"),
            id="every key at once",
        ),
    ],
)
def test_a_step_is_written_with_the_same_keys_in_a_recipe_file_and_in_fx(tmp_path, toml, python):
    (tmp_path / "r.toml").write_text(f"[[step]]\n{toml}\n", encoding="utf-8")
    assert load_recipe(tmp_path / "r.toml").steps == (python(),)


# --- trace: the picture after every step -----------------------------------------------------------------------


def test_trace_gives_the_input_and_the_picture_after_each_step_in_order(photo):
    look = Recipe("t", size=None, steps=[fx.grade(exposure=0.5), fx.slice_shift(seed=1), fx.grain(seed=2)])
    stages = trace(photo, look)
    assert [s.label.split("(")[0] for s in stages] == ["input", "1. grade", "2. slice_shift", "3. grain"]
    np.testing.assert_array_equal(stages[0].image, photo)
    assert not np.array_equal(stages[1].image, stages[0].image)
    assert not np.array_equal(stages[3].image, stages[2].image)


def test_the_last_stage_of_a_trace_is_exactly_what_render_returns(photo, subject):
    steps = [
        fx.gradient_map(),
        fx.gradient(colors=["#102030"], mask="background"),
        fx.bloom(opacity=0.7, blend_mode="screen"),
    ]
    look = Recipe("t", size=64, steps=steps)
    np.testing.assert_array_equal(
        trace(photo, look, maps={"subject": subject})[-1].image, render(photo, look, maps={"subject": subject})
    )


def test_trace_shows_the_subject_matte_when_the_recipe_uses_one(photo, subject):
    stages = trace(
        photo,
        Recipe("t", size=None, steps=[fx.gradient(colors=["#000000"], mask="background")]),
        maps={"subject": subject},
    )
    assert [s.label.split(" (")[0] for s in stages][:2] == ["input", "subject matte"]
    np.testing.assert_allclose(stages[1].image[..., 0], subject)  # white where the subject is
    assert "subject matte" not in " ".join(s.label for s in trace(photo, Recipe("t", [fx.grain()], size=None)))


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
    look = Recipe("look", size=64, steps=[fx.grade(exposure=0.4), fx.slice_shift(seed=1)])
    project = make_project(tmp_path, photo, [look], steps=True)
    (item,) = run(project)
    folder = tmp_path / "out" / "one" / "steps" / "look"
    assert sorted(p.name for p in folder.iterdir()) == [
        "00-input.jpg",
        "01-grade.jpg",
        "02-slice_shift.jpg",
        "_sheet.jpg",
    ]
    assert item.steps == "one/steps/look/_sheet.jpg"
    assert item.outputs == ("one/look.jpg", "one/steps/look/_sheet.jpg")  # links: `/` on every system, Windows too
    final = imageio.load(tmp_path / "out" / "one" / "look.jpg")
    assert (
        np.abs(final - imageio.load(folder / "02-slice_shift.jpg")).max() < 0.03
    )  # the last step is the exported result
    assert 'href="one/steps/look/_sheet.jpg"' in (tmp_path / "out" / "index.html").read_text(encoding="utf-8")
    manifest = json.loads((tmp_path / "out" / "manifest.json").read_text(encoding="utf-8"))
    assert manifest["items"][0]["steps"] == "one/steps/look/_sheet.jpg"


def test_turning_steps_on_redoes_finished_work_so_the_pictures_exist(tmp_path, photo):
    look = Recipe("look", size=64, steps=[fx.grain(seed=1)])
    assert [i.status for i in run(make_project(tmp_path, photo, [look]))] == ["done"]
    assert [i.status for i in run(make_project(tmp_path, photo, [look], steps=True))] == ["done"]


def test_the_steps_command_writes_one_picture_per_step_and_a_sheet(tmp_path, photo, capsys):
    imageio.save(photo, tmp_path / "p.png")
    (tmp_path / "r.toml").write_text(
        'name = "r"\nsize = 64\n[[step]]\neffect = "grade"\nexposure = 0.2\n[[step]]\neffect = "grain"\n',
        encoding="utf-8",
    )
    assert main(["steps", str(tmp_path / "p.png"), str(tmp_path / "r.toml"), "-o", str(tmp_path / "s")]) == 0
    assert sorted(p.name for p in (tmp_path / "s").iterdir()) == [
        "00-input.jpg",
        "01-grade.jpg",
        "02-grain.jpg",
        "_sheet.jpg",
    ]
    assert "1. grade(exposure=0.2)" in capsys.readouterr().err


# --- a picture parameter in fx: a path, an array, a Picture or a colour ---------------------------------------------


def test_fx_turns_a_path_or_an_array_into_a_picture_and_leaves_a_picture_or_a_colour_alone(
    layer_probe, tmp_path, monkeypatch
):
    pixels = np.random.default_rng(2).random((5, 7, 4), dtype=np.float32)
    imageio.save(pixels, tmp_path / "tex.png")
    monkeypatch.chdir(tmp_path)  # a path is relative to the folder the program runs in
    from_path, from_pathlib = fx.layer_probe("tex.png"), fx.layer_probe(image=Path("tex.png"))
    from_array, picture = fx.layer_probe(pixels), Picture(pixels)
    assert from_path.params["image"].label == "tex.png"
    assert from_array.params["image"] == picture == fx.layer_probe(picture).params["image"]
    assert from_path.params["image"] == from_pathlib.params["image"]
    np.testing.assert_allclose(from_path.params["image"].pixels, pixels, atol=1 / 255)
    assert fx.layer_probe("#102030").params["image"] == "#102030"  # a colour stays as written


@pytest.mark.parametrize(
    ("image", "kind", "complaint"),
    [
        pytest.param("missing.png", ImageReadError, r"layer_probe.*`image`.*missing\.png", id="missing file"),
        pytest.param(np.zeros((4, 4, 3), np.uint8), RecipeError, r"picture.*uint8", id="integer array"),
        pytest.param(np.full((4, 4, 3), 2.0, np.float32), RecipeError, r"picture.*0\.\.1", id="values above one"),
    ],
)
def test_fx_refuses_a_picture_it_cannot_use_naming_the_problem(layer_probe, image, kind, complaint):
    with pytest.raises(kind, match=complaint):
        fx.layer_probe(image)
