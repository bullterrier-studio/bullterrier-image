import json
from pathlib import Path

import numpy as np
import pytest

from bullterrier_image import Recipe, Step, SubjectNotFoundError, imageio, load_recipe, render, trace
from bullterrier_image import recipe as facade
from bullterrier_image.core import runner
from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.matte import subject_matte
from bullterrier_image.ops.resample import resize

EXAMPLE_PHOTO = Path(__file__).parent.parent / "examples" / "photos" / "bullterrier.jpeg"


class AmountParams(EffectParams):
    """The one parameter of a probe effect."""

    amount: float = param(0.5, unit="fraction", min=0, max=1, doc="Recorded or ignored by the probe.")


def write(tmp_path, text, name="r.toml"):
    path = tmp_path / name
    path.write_text(text, encoding="utf-8")
    return path


def test_load_recipe_reads_steps_params_and_masks(tmp_path):
    r = load_recipe(write(tmp_path, 'name="x"\nsize=64\n[[step]]\neffect="fill"\ncolor="#fff"\nmask="background"\n'))
    assert r == Recipe("x", (Step("fill", {"color": "#fff"}, "background"),), 64)
    assert r.needs_matte


def test_recipe_without_masks_does_not_need_a_matte(tmp_path):
    assert not load_recipe(write(tmp_path, '[[step]]\neffect="grain"\n')).needs_matte


def test_recipe_name_defaults_to_the_file_name(tmp_path):
    assert load_recipe(write(tmp_path, '[[step]]\neffect="grain"\n', "my-look.toml")).name == "my-look"


@pytest.mark.parametrize(
    ("text", "expect"),
    [
        ('[[step]]\neffect="no_such_effect"\n', "step 1.*unknown effect"),
        ('[[step]]\neffect="grain"\nbogus=1\n', "step 1.*bogus"),
        ('[[step]]\neffect="grain"\n[[step]]\neffect="fill"\nmask="sideways"\n', "step 2.*mask"),
        ('[[step]]\ncolor="red"\n', "missing `effect`"),
        ("name='empty'\n", "at least one"),
    ],
)
def test_bad_recipes_fail_at_load_time_and_name_the_problem(tmp_path, text, expect):
    with pytest.raises(ValueError, match=expect):
        load_recipe(write(tmp_path, text))


@pytest.mark.parametrize(
    ("mask", "color", "filled", "region"), [("background", "#000000", 0.0, 0), ("subject", "#ffffff", 1.0, 1)]
)
def test_a_masked_step_changes_only_its_region(photo, matte, mask, color, filled, region):
    """`region` is the matte value of the pixels the mask selects."""
    out = render(photo, Recipe("m", (Step("fill", {"color": color}, mask),), size=None), matte)
    np.testing.assert_allclose(out[matte == 1 - region], photo[matte == 1 - region], atol=1e-6)
    assert np.allclose(out[matte == region], filled)


def test_steps_run_in_order(photo):
    r = Recipe("o", (Step("fill", {"color": "#ff0000"}), Step("grade", {"exposure": -1.0})), size=None)
    np.testing.assert_allclose(render(photo, r)[0, 0], [0.5, 0, 0], atol=1e-5)


def test_opacity_blends_the_step_in_partway(photo):
    half = Recipe("h", (Step("fill", {"color": "#000000"}, None, 0.5),), size=None)
    np.testing.assert_allclose(render(photo, half), photo * 0.5, atol=1e-6)
    off = Recipe("o", (Step("fill", {"color": "#000000"}, None, 0.0),), size=None)
    np.testing.assert_allclose(render(photo, off), photo, atol=1e-6)


def test_opacity_combines_with_a_mask(photo, matte):
    r = Recipe("b", (Step("fill", {"color": "#000000"}, "background", 0.5),), size=None)
    out = render(photo, r, matte)
    np.testing.assert_allclose(out[matte == 0], photo[matte == 0] * 0.5, atol=1e-6)
    np.testing.assert_allclose(out[matte == 1], photo[matte == 1], atol=1e-6)


@pytest.mark.parametrize(
    ("key", "good", "bad"),
    [pytest.param("opacity", 0.4, 1.5, id="opacity"), pytest.param("blend_mode", "screen", "lighten", id="blend_mode")],
)
def test_a_step_key_is_read_from_the_file_onto_the_step_and_validated(tmp_path, key, good, bad):
    (step,) = load_recipe(write(tmp_path, f'[[step]]\neffect="grain"\n{key}={json.dumps(good)}\n')).steps
    assert getattr(step, key) == good
    assert key not in step.runs[0].params
    with pytest.raises(ValueError, match=rf"step 1.*{key}.*{bad}"):
        load_recipe(write(tmp_path, f'[[step]]\neffect="grain"\n{key}={json.dumps(bad)}\n', "bad.toml"))


# --- blend_mode: the step's result is laid over its input as a layer is in an image editor ----------------------

MODES = ["normal", "multiply", "screen", "overlay", "soft_light", "difference", "color"]
ORANGE = np.array([1.0, 0.5, 0.0], np.float32)  # "#ff8000" to within 1/510


def fill(color: str, **keys) -> Recipe:
    return Recipe("b", (Step("fill", {"color": color}, **keys),), size=None)


@pytest.mark.parametrize(
    ("mode", "color"),
    [("multiply", "#ffffff"), ("screen", "#000000"), ("difference", "#000000"), ("overlay", "#808080")],
    ids=["multiply by white", "screen by black", "difference with black", "overlay with mid grey"],
)
def test_a_neutral_result_leaves_the_photo_as_it_was_under_its_mode(photo, mode, color):
    np.testing.assert_allclose(render(photo, fill(color, blend_mode=mode)), photo, atol=0.003)  # 0x80 is 0.502


@pytest.mark.parametrize(
    ("mode", "by_hand"),
    [
        ("multiply", lambda photo: photo * ORANGE),
        ("screen", lambda photo: photo + ORANGE - photo * ORANGE),
        ("difference", lambda photo: np.abs(photo - ORANGE)),
    ],
)
def test_the_mode_combines_the_steps_input_with_its_result(photo, mode, by_hand):
    np.testing.assert_allclose(render(photo, fill("#ff8000", blend_mode=mode)), by_hand(photo), atol=0.002)


@pytest.mark.parametrize("mode", MODES)
def test_opacity_0_leaves_the_photo_as_it_was_under_every_mode(photo, mode):
    np.testing.assert_array_equal(render(photo, fill("#ff8000", blend_mode=mode, opacity=0.0)), photo)


def test_the_mode_combines_first_and_the_opacity_mixes_that_in(photo):
    grey = np.float32(0x80 / 255)
    out = render(photo, fill("#808080", blend_mode="multiply", opacity=0.5))
    np.testing.assert_allclose(out, photo + 0.5 * (photo * grey - photo), atol=1e-6)


def test_a_mode_on_the_background_leaves_the_subject_untouched(photo, matte):
    out = render(photo, fill("#ff8000", blend_mode="multiply", mask="background"), matte)
    np.testing.assert_array_equal(out[matte == 1], photo[matte == 1])
    np.testing.assert_allclose(out[matte == 0], photo[matte == 0] * ORANGE, atol=0.002)


def test_a_step_says_its_mode_and_by_default_it_is_normal():
    assert Step("grain").blend_mode == "normal"
    assert Step("grain", blend_mode="screen") != Step("grain")
    assert Step("glow", blend_mode="screen", opacity=0.8).describe() == "glow() screen x0.8"


def test_effects_that_declare_a_matte_parameter_get_the_person_silhouette(photo, matte, throwaway_effects):
    seen = []

    @effect(category="utilities", params=AmountParams)
    def silhouette_probe(rgb, params, *, matte=None):
        """Records the silhouette it is given."""
        seen.append(matte)
        return rgb

    r = Recipe("s", (Step("silhouette_probe"),), size=None)
    assert r.needs_matte  # an effect that wants the silhouette makes the recipe look for the person
    render(photo, r, matte)
    assert len(seen) == 1
    np.testing.assert_array_equal(seen[0], matte)


def test_an_effect_that_changes_the_image_shape_is_an_error_not_a_silent_mismatch(photo, throwaway_effects):
    @effect(category="utilities", params=AmountParams)
    def halve_for_shape_test(rgb, params):
        return rgb[::2, ::2]

    recipe = Recipe("shapeshift", (Step("halve_for_shape_test"),), size=None)
    for run in (render, trace):
        with pytest.raises(RuntimeError) as raised:
            run(photo, recipe)
        message = str(raised.value)  # names the effect and both shapes
        assert "halve_for_shape_test" in message
        assert "(96, 128, 3)" in message
        assert "(48, 64, 3)" in message


def test_size_scales_photo_and_matte_together(photo, matte):
    out = render(photo, Recipe("s", (Step("fill", {"color": "#000000"}, "background"),), size=64), matte)
    assert out.shape == (48, 64, 3)


def test_matte_of_the_wrong_size_is_rejected(photo):
    with pytest.raises(SubjectNotFoundError, match="matte"):
        render(photo, Recipe("r", (Step("grain"),), size=None), np.ones((5, 5), np.float32))


def test_shipped_recipes_all_load():
    examples = Path(__file__).parent.parent / "examples"
    files = sorted([*(examples / "recipes").glob("*.toml"), *(examples / "experiments").glob("*.toml")])
    assert files
    assert all(load_recipe(f).steps for f in files)


def test_the_facade_finds_the_subject_with_matte_subject_matte_by_default(echo_model, monkeypatch):
    """Without a matte or `find_subject`, `bullterrier_image.recipe`'s functions find the subject with `subject_matte`:
    the results must equal the core's given `subject_matte`'s answer. The portable backend with a stand-in model (the
    external boundary, see `echo_model`) is chosen through the environment, so the test is the same on every OS."""
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "onnx")
    monkeypatch.setenv("BT_IMAGE_MODEL", str(echo_model(0)))
    photo = resize(imageio.load(EXAMPLE_PHOTO), 160)
    masked = Recipe("m", (Step("fill", {"color": "#000000"}, "background"),), size=None)
    alpha = subject_matte(photo)
    expected = runner.render(photo, masked, alpha)
    assert not np.array_equal(expected, runner.render(photo, masked, np.ones_like(alpha)))  # the subject matters
    np.testing.assert_array_equal(facade.render(photo, masked), expected)
    np.testing.assert_array_equal(facade.trace(photo, masked)[-1].image, expected)
    np.testing.assert_array_equal(facade.render_all(photo, [masked])[0], expected)


def test_the_facade_uses_a_given_finder_instead_of_the_default(photo, matte):
    masked = Recipe("m", (Step("fill", {"color": "#000000"}, "background"),), size=None)
    found = facade.render(photo, masked, find_subject=lambda rgb: 1.0 - matte)
    np.testing.assert_array_equal(found, runner.render(photo, masked, 1.0 - matte))
    assert not np.array_equal(found, runner.render(photo, masked, matte))
