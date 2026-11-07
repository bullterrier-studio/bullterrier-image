import json
import pickle
import subprocess
import sys
from collections.abc import Callable
from contextlib import nullcontext
from pathlib import Path

import numpy as np
import pytest

from bullterrier_image import (
    Each,
    Finder,
    Recipe,
    Step,
    fx,
    imageio,
    load_recipe,
    render,
    render_all,
    trace,
)
from bullterrier_image import recipe as facade
from bullterrier_image.core import looks
from bullterrier_image.core.bridge import maps_needed
from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.core.steps import runs
from bullterrier_image.core.values import FieldName
from bullterrier_image.errors import BackendUnavailableError, RecipeError
from bullterrier_image.finding.depth import depth_map
from bullterrier_image.finding.subject import subject_matte
from bullterrier_image.ops.resample import resize
from bullterrier_image.recipe import Band, Grid, Subject

EXAMPLE_PHOTO = Path(__file__).parent.parent / "examples" / "photos" / "bullterrier.jpeg"


class AmountParams(EffectParams):
    """The one parameter of a probe effect."""

    amount: float = param(0.5, unit="fraction", min=0, max=1, doc="Recorded or ignored by the probe.")


def write(tmp_path, text, name="r.toml"):
    path = tmp_path / name
    path.write_text(text, encoding="utf-8")
    return path


def test_load_recipe_reads_steps_params_and_masks(tmp_path):
    r = load_recipe(
        write(tmp_path, 'name="x"\nsize=64\n[[step]]\neffect="gradient"\ncolors=["#fff"]\nmask="background"\n')
    )
    assert r == Recipe((Step("gradient", params={"colors": ["#fff"]}, mask="background"),), name="x", size=64)
    assert "subject" in maps_needed(r)


def test_recipe_without_masks_does_not_need_a_matte(tmp_path):
    assert "subject" not in maps_needed(load_recipe(write(tmp_path, '[[step]]\neffect="grain"\n')))


def test_recipe_name_defaults_to_the_file_name(tmp_path):
    assert load_recipe(write(tmp_path, '[[step]]\neffect="grain"\n', "my-look.toml")).name == "my-look"


@pytest.mark.parametrize(
    ("text", "expect"),
    [
        ('[[step]]\neffect="no_such_effect"\n', "step 1.*unknown effect"),
        ('[[step]]\neffect="grain"\nbogus=1\n', "step 1.*bogus"),
        (
            '[[step]]\neffect="grain"\n[[step]]\neffect="gradient"\ncolors=["#000000"]\nmask="sideways"\n',
            "step 2.*mask",
        ),
        ('[[step]]\ncolor="red"\n', "missing `effect`"),
        ("name='empty'\n", "at least one"),
    ],
)
def test_bad_recipes_fail_at_load_time_and_name_the_problem(tmp_path, text, expect):
    with pytest.raises(ValueError, match=expect):
        load_recipe(write(tmp_path, text))


@pytest.mark.parametrize(
    ("effect_name", "old", "new"),
    [
        pytest.param("torn_edge", "depth", "width", id="torn_edge"),
    ],
)
def test_a_renamed_field_is_read_by_its_new_name_and_refused_by_the_old_one(tmp_path, effect_name, old, new):
    """The old name is no alias: it is an unknown parameter."""
    (step,) = load_recipe(write(tmp_path, f'[[step]]\neffect="{effect_name}"\n{new}=0.5\n')).steps
    assert runs(step)[0].params == {new: 0.5}
    with pytest.raises(RecipeError, match=rf"step 1.*`{old}` is not a parameter"):
        load_recipe(write(tmp_path, f'[[step]]\neffect="{effect_name}"\n{old}=0.5\n', "old.toml"))


@pytest.mark.parametrize(
    ("key", "good", "bad"),
    [pytest.param("opacity", 0.4, 1.5, id="opacity"), pytest.param("blend_mode", "screen", "lighten", id="blend_mode")],
)
def test_a_step_key_is_read_from_the_file_onto_the_step_and_validated(tmp_path, key, good, bad):
    (step,) = load_recipe(write(tmp_path, f'[[step]]\neffect="grain"\n{key}={json.dumps(good)}\n')).steps
    assert getattr(step, key) == good
    assert key not in runs(step)[0].params
    with pytest.raises(ValueError, match=rf"step 1.*{key}.*{bad}"):
        load_recipe(write(tmp_path, f'[[step]]\neffect="grain"\n{key}={json.dumps(bad)}\n', "bad.toml"))


# --- blend_mode: the step's result is laid over its input as a layer is in an image editor ----------------------


def test_a_step_says_its_mode_and_by_default_it_is_normal():
    assert Step("grain").blend_mode == "normal"
    assert Step("grain", blend_mode="screen") != Step("grain")
    assert Step("bloom", blend_mode="screen", opacity=0.8).describe() == "bloom() screen x0.8"


class SilhouetteParams(EffectParams):
    """A field that is the subject unless a step names another."""

    field: FieldName = param("subject", doc="The field recorded.")


def test_an_effect_whose_field_is_the_subject_gets_the_silhouette(photo, subject, throwaway_effects):
    seen = []

    @effect(category="utilities", params=SilhouetteParams)
    def silhouette_probe(rgb, params, *, field):
        """Records the silhouette it is given."""
        seen.append(field)
        return rgb

    r = Recipe((Step("silhouette_probe"),), name="s", size=None)
    assert "subject" in maps_needed(r)  # a field that is the subject makes the recipe look for it
    render(photo, r, maps={"subject": subject})
    assert len(seen) == 1
    np.testing.assert_array_equal(seen[0], subject)


def test_an_effect_that_changes_the_image_shape_is_an_error_not_a_silent_mismatch(photo, throwaway_effects):
    @effect(category="utilities", params=AmountParams)
    def halve_for_shape_test(rgb, params):
        return rgb[::2, ::2]

    recipe = Recipe((Step("halve_for_shape_test"),), name="shapeshift", size=None)
    for run in (render, trace):
        with pytest.raises(RuntimeError) as raised:
            run(photo, recipe)
        message = str(raised.value)  # names the effect and both shapes
        assert "halve_for_shape_test" in message
        assert "(96, 128, 3)" in message
        assert "(48, 64, 3)" in message


def test_an_effect_must_return_a_float32_array_a_float64_one_is_a_broken_effect(photo, throwaway_effects):
    @effect(category="utilities", params=AmountParams)
    def float32_for_contract_test(rgb, params):
        return rgb.copy()

    @effect(category="utilities", params=AmountParams)
    def float64_for_contract_test(rgb, params):
        return rgb.astype(np.float64)

    np.testing.assert_array_equal(render(photo, Recipe((Step("float32_for_contract_test"),), size=None)), photo)
    with pytest.raises(ValueError, match=r"float64_for_contract_test.*must return a float32 ndarray"):
        render(photo, Recipe((Step("float64_for_contract_test"),), size=None))


@pytest.mark.parametrize("size", [0, -1, 16385, 10**8, 1.5, True], ids=repr)
def test_a_render_size_that_is_no_whole_number_from_1_to_16384_is_refused_before_anything_is_made(photo, size):
    with pytest.raises(RecipeError, match="size"):
        render(photo, Recipe([Step("grade")], name="s"), size=size)


def test_shipped_recipes_all_load():
    examples = Path(__file__).parent.parent / "examples"
    files = sorted([*(examples / "recipes").glob("*.toml"), *(examples / "experiments").glob("*.toml")])
    assert files
    assert all(load_recipe(f).steps for f in files)


def test_the_facade_finds_the_subject_with_matte_subject_matte_by_default(echo_model, monkeypatch):
    """Without a subject map or finder, `bullterrier_image.recipe`'s functions find the subject with `subject_matte`:
    the results must equal the core's given `subject_matte`'s answer. The portable backend with a stand-in model (the
    external boundary, see `echo_model`) is chosen through the environment, so the test is the same on every OS."""
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "onnx")
    monkeypatch.setenv("BT_IMAGE_SUBJECT_MODEL", str(echo_model(0)))
    photo = resize(imageio.load(EXAMPLE_PHOTO), 160)
    masking = Step("gradient", params={"colors": ["#000000"]}, mask="background")
    masked = Recipe((masking,), name="m", size=None)
    alpha = subject_matte(photo)
    expected = facade.render(photo, masked, maps={"subject": alpha})
    assert not np.array_equal(
        expected, facade.render(photo, masked, maps={"subject": np.ones_like(alpha)})
    )  # the subject matters
    np.testing.assert_array_equal(facade.render(photo, masked), expected)
    np.testing.assert_array_equal(facade.trace(photo, masked)[-1].image, expected)
    np.testing.assert_array_equal(facade.render_all(photo, [masked])[0], expected)
    np.testing.assert_array_equal(masking.apply(photo), expected)  # one step alone, with the same finder
    np.testing.assert_array_equal(facade.render(photo, masking), expected)


STEP_ALONE = {
    "an effect": (fx.bloom(radius=12, threshold=0.3), {}),
    "at a size": (fx.slice_shift(seed=3, amount=20), {"size": 64}),
    "masked, given the subject": (fx.gradient(colors=["#000000"], mask="background"), {"maps": "subject"}),
}


@pytest.mark.parametrize(("step", "given"), STEP_ALONE.values(), ids=STEP_ALONE.keys())
def test_one_step_applied_alone_is_the_recipe_of_that_step_rendered(photo, subject, step, given):
    options = {**given, "maps": {"subject": subject}} if "maps" in given else given
    expected = render(photo, Recipe([step]), **options)
    np.testing.assert_array_equal(step.apply(photo, **options), expected)
    np.testing.assert_array_equal(render(photo, step, **options), expected)
    np.testing.assert_array_equal(trace(photo, step, **options)[-1].image, expected)


STEP, ONE = fx.grade(), Recipe([fx.grade()])
WRONG_RECIPES: dict[str, tuple[Callable[..., object], Callable[[np.ndarray], tuple], str]] = {
    "a list of steps": (render, lambda photo: (photo, [STEP]), r"Recipe\(\["),
    "a look's name": (render, lambda photo: (photo, "glitch"), r"load_recipe"),
    "an each step": (render, lambda photo: (photo, Each("panes", looks=[None])), r"Recipe\(\["),
    "the photo and the recipe swapped": (render, lambda photo: (ONE, photo), r"photo comes first"),
    "a trace of a list": (trace, lambda photo: (photo, [STEP]), r"Recipe\(\["),
    "render_all of a list of steps": (render_all, lambda photo: (photo, [STEP]), r"sequence of Recipe.*Step"),
    "render_all of one recipe": (render_all, lambda photo: (photo, ONE), r"sequence of Recipe"),
}


@pytest.mark.parametrize(("run", "arguments", "fix"), WRONG_RECIPES.values(), ids=WRONG_RECIPES.keys())
def test_render_refuses_what_is_no_recipe_naming_the_fix(photo, run, arguments, fix):
    with pytest.raises(RecipeError, match=fix):
        run(*arguments(photo))


def test_render_takes_a_recipe_a_step_and_render_all_a_sequence_of_recipes(photo):
    step = fx.grade(exposure=0.3)
    recipe = Recipe([step])
    expected = render(photo, recipe)
    np.testing.assert_array_equal(render(photo, step), expected)
    assert trace(photo, recipe)[-1].image.shape == expected.shape
    for recipes in ([recipe], (recipe,)):
        (rendered,) = render_all(photo, recipes)
        np.testing.assert_array_equal(rendered, expected)


FRESH_STEPS = {
    "a step unpickled in a worker": "import pickle; step = pickle.load(open(sys.argv[1], 'rb'))",
    "a step read from a recipe file": (
        "from bullterrier_image.loader import load_recipe; step = load_recipe(sys.argv[2]).steps[0]"
    ),
}


@pytest.mark.parametrize("make_step", FRESH_STEPS.values(), ids=FRESH_STEPS.keys())
def test_a_step_applies_in_a_fresh_process_that_imported_nothing_else(tmp_path, photo, make_step):
    """A process pool's worker unpickles a step without ever importing the public names (`bullterrier_image.render`)."""
    step = fx.bloom(radius=12, threshold=0.3)
    (tmp_path / "step.pickle").write_bytes(pickle.dumps(step))
    (tmp_path / "bloom.toml").write_text(
        'name = "b"\n[[step]]\neffect = "bloom"\nradius = 12\nthreshold = 0.3\n', encoding="utf-8"
    )
    np.save(tmp_path / "photo.npy", photo)
    code = f"import sys; import numpy as np; {make_step}; np.save(sys.argv[3], step.apply(np.load(sys.argv[4])))"
    done = subprocess.run(
        [
            sys.executable,
            "-c",
            code,
            tmp_path / "step.pickle",
            tmp_path / "bloom.toml",
            tmp_path / "out.npy",
            tmp_path / "photo.npy",
        ],
        capture_output=True,
        encoding="utf-8",
        check=False,
    )
    assert done.returncode == 0, done.stderr
    np.testing.assert_array_equal(np.load(tmp_path / "out.npy"), step.apply(photo))


def test_the_facade_uses_a_given_finder_instead_of_the_default(photo, subject):
    masked = Recipe((Step("gradient", params={"colors": ["#000000"]}, mask="background"),), name="m", size=None)
    found = facade.render(photo, masked, finders={"subject": Finder("stand-in", lambda rgb: 1.0 - subject)})
    np.testing.assert_array_equal(found, facade.render(photo, masked, maps={"subject": 1.0 - subject}))
    assert not np.array_equal(found, facade.render(photo, masked, maps={"subject": subject}))


PLAIN = Recipe((Step("grade", params={"exposure": 0.5}),), name="plain", size=None)
MASKED = Recipe((Step("gradient", params={"colors": ["#000000"]}, mask="background"),), name="masked", size=None)
PIECE_BY_PIECE = Recipe(
    (Step("gradient", params={"colors": ["#000000"]}, mask="found"),),
    name="pieces",
    size=None,
    regions={"cells": Grid(columns=2), "found": Subject(per="cells")},
)
# With a segmenter setting that names no segmenter, the default finder is built only for a recipe that will ask it:
# (recipe, given a subject, what the refusal names; None: it renders).
LAZY_DEFAULT = {
    "a recipe that needs no subject renders": (PLAIN, False, None),
    "a recipe given the subject it needs renders": (MASKED, True, None),
    "a recipe that needs the subject names the choices": (MASKED, False, "onnx, vision"),
    "a recipe finding the subject piece by piece asks even with a subject": (PIECE_BY_PIECE, True, "onnx, vision"),
}
CALLS = {
    "render": lambda photo, recipe, **keys: facade.render(photo, recipe, **keys),
    "trace": lambda photo, recipe, **keys: facade.trace(photo, recipe, **keys),
    "render_all": lambda photo, recipe, **keys: facade.render_all(photo, [recipe], **keys),
}


@pytest.mark.parametrize("call", CALLS.values(), ids=CALLS.keys())
@pytest.mark.parametrize(("recipe", "given", "refused"), LAZY_DEFAULT.values(), ids=LAZY_DEFAULT.keys())
def test_the_default_finder_is_built_only_when_a_recipe_asks_for_the_subject(
    photo, subject, monkeypatch, call, recipe, given, refused
):
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "nonsense")
    with pytest.raises(BackendUnavailableError, match=refused) if refused else nullcontext():
        call(photo, recipe, **({"maps": {"subject": subject}} if given else {}))


DEEP = Recipe(
    (Step("gradient", params={"colors": ["#000000"]}, mask="near"),),
    name="deep",
    size=None,
    regions={"near": Band(of="depth", low=0.5, high=1.0)},
)
# With the suite's `BT_IMAGE_DEPTH_MODEL` naming no file, the default depth finder is built only for a recipe that needs
# depth and is not given it: (recipe, given a depth map, what the refusal names; None: it renders).
LAZY_DEPTH_DEFAULT = {
    "a recipe that needs no depth renders": (PLAIN, False, None),
    "a recipe given the depth it needs renders": (DEEP, True, None),
    "a recipe that needs depth names the model's variable": (DEEP, False, "BT_IMAGE_DEPTH_MODEL"),
}


@pytest.mark.parametrize("call", CALLS.values(), ids=CALLS.keys())
@pytest.mark.parametrize(("recipe", "given", "refused"), LAZY_DEPTH_DEFAULT.values(), ids=LAZY_DEPTH_DEFAULT.keys())
def test_the_default_depth_finder_is_built_only_when_a_recipe_needs_depth(photo, call, recipe, given, refused):
    ramp = np.tile(np.linspace(0, 1, 128, dtype=np.float32), (96, 1))
    with pytest.raises(BackendUnavailableError, match=refused) if refused else nullcontext():
        call(photo, recipe, **({"maps": {"depth": ramp}} if given else {}))


def test_the_facade_finds_the_depth_with_depth_map_by_default(depth_echo_model):
    """Without a `depth` map or finder, `bullterrier_image.recipe`'s functions find the depth with `depth_map`, here
    run by a stand-in model (the external boundary, see `depth_echo_model`)."""
    depth_echo_model(0)
    photo = resize(imageio.load(EXAMPLE_PHOTO), 160)
    expected = facade.render(photo, DEEP, maps={"depth": depth_map(photo)})
    flat = np.zeros(photo.shape[:2], np.float32)
    assert not np.array_equal(expected, facade.render(photo, DEEP, maps={"depth": flat}))  # the depth matters
    np.testing.assert_array_equal(facade.render(photo, DEEP), expected)
    np.testing.assert_array_equal(facade.trace(photo, DEEP)[-1].image, expected)
    np.testing.assert_array_equal(facade.render_all(photo, [DEEP])[0], expected)


# --- the built-in looks --------------------------------------------------------------------------------------------


@pytest.mark.parametrize("name", sorted(looks.read()))
def test_every_look_loads_by_name_and_renders_something_other_than_the_photo(name, photo, subject):
    recipe = load_recipe(name)
    assert recipe.name == name
    assert len(recipe.steps) >= 2  # one step is a preset, not a look
    depth = np.repeat(np.linspace(0, 1, photo.shape[0], dtype=np.float32)[:, None], photo.shape[1], axis=1)
    out = render(photo, recipe, maps={"subject": subject, "depth": depth}, size=max(photo.shape[:2]))
    assert out.shape == photo.shape
    assert not np.array_equal(out, photo)
