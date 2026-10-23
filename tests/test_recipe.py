from pathlib import Path

import numpy as np
import pytest

from photo_fx import Recipe, RecipeError, Step, SubjectNotFoundError, imageio, load_recipe, render, trace
from photo_fx import recipe as facade
from photo_fx.core import runner
from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.core.timing import Animated, Animation
from photo_fx.matte import subject_matte
from photo_fx.ops.resample import resize
from photo_fx.recipe import render_frames

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
        ('[[step]]\neffect="sparkle"\n', "step 1.*unknown effect"),
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


def test_opacity_is_read_from_the_file_and_validated(tmp_path):
    ok = load_recipe(write(tmp_path, '[[step]]\neffect="grain"\nopacity=0.4\n'))
    assert ok.steps[0].opacity == 0.4
    assert "opacity" not in ok.steps[0].params
    with pytest.raises(ValueError, match=r"step 1.*opacity"):
        load_recipe(write(tmp_path, '[[step]]\neffect="grain"\nopacity=1.5\n', "bad.toml"))


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


def test_matte_cannot_be_set_in_a_recipe_file(tmp_path):
    with pytest.raises(ValueError, match="matte"):
        load_recipe(write(tmp_path, '[[step]]\neffect="evaporate"\nmatte=1\n'))


ANIMATED = """
[animation]
seconds = 2
fps = 10
hold_start = 0.5
hold_end = 0.5

[[step]]
effect = "grade"
exposure = { from = 0.0, to = 1.0, ease = "in-out" }
"""


def test_animation_is_read_from_the_file(tmp_path):
    r = load_recipe(write(tmp_path, ANIMATED))
    assert r.animation == Animation(seconds=2, fps=10, hold_start=0.5, hold_end=0.5)
    assert r.steps[0].params == {"exposure": Animated(0.0, 1.0, "in-out")}
    assert r.steps[0].animated


def test_animation_positions_hold_at_both_ends_and_run_in_between():
    positions = Animation(seconds=2, fps=10, hold_start=0.5, hold_end=0.5).positions()
    assert len(positions) == 20
    # held, then still at the end
    assert positions[:6] == [0.0] * 6
    assert positions[-5:] == [1.0] * 5
    assert positions == sorted(positions)


def test_pingpong_plays_forward_then_back_without_repeating_the_turning_frames():
    once = Animation(seconds=1, fps=5).positions()
    both = Animation(seconds=1, fps=5, loop="pingpong").positions()
    assert both == once + once[-2:0:-1]
    assert len(both) == 2 * len(once) - 2


@pytest.mark.parametrize(
    ("text", "expect"),
    [
        ('[[step]]\neffect="grade"\nexposure={from=0, to=1}\n', "no animation"),
        (ANIMATED.replace('{ from = 0.0, to = 1.0, ease = "in-out" }', "{ from = 0.0 }"), "step 1.*exposure"),
        (ANIMATED.replace('"in-out"', '"bouncy"'), "step 1.*exposure"),
        (ANIMATED.replace("hold_end = 0.5", "hold_end = 1.5"), "no time"),
        (ANIMATED.replace("seconds = 2", "speed = 2"), "needs `seconds`"),
        (ANIMATED.replace("hold_end = 0.5", 'loop = "spiral"'), "loop"),
        (ANIMATED.replace('ease = "in-out"', 'over = "forever"'), "step 1.*exposure"),
        (ANIMATED.replace('ease = "in-out"', "ease = [0.4, 0.0, 0.2]"), "step 1.*exposure"),
    ],
)
def test_bad_animations_fail_at_load_time(tmp_path, text, expect):
    """Each is refused naming the step and the field; every rule of an easing curve is checked where `Animated` is
    made in code (tests/core/test_steps.py), the one model both paths go through."""
    with pytest.raises(ValueError, match=expect):
        load_recipe(write(tmp_path, text))


def test_a_custom_cubic_bezier_can_be_written_as_four_numbers(tmp_path):
    r = load_recipe(write(tmp_path, ANIMATED.replace('ease = "in-out"', "ease = [0.4, 0.0, 0.2, 1.0]")))
    assert r.steps[0].params["exposure"].ease == (0.4, 0.0, 0.2, 1.0)


def test_over_clip_keeps_running_through_the_holds():
    still = Animated(0.0, 10.0, over="motion")
    clock = Animated(0.0, 10.0, over="clip")
    # held at its start / still running
    assert still.at(0.0, clip=0.5) == 0.0
    assert clock.at(0.0, clip=0.5) == 5.0
    frames = Animation(seconds=2, fps=10, hold_start=0.5, hold_end=0.5).frames()
    assert frames[0] == (0.0, 0.0)
    assert frames[3] == (0.0, 3 / 19)
    assert frames[-1] == (1.0, 1.0)
    assert [clip for _, clip in frames] == sorted(clip for _, clip in frames)


def test_render_at_picks_a_moment_of_the_clip(photo, tmp_path):
    r = load_recipe(write(tmp_path, ANIMATED))
    r = Recipe(r.name, r.steps, size=None, animation=r.animation)
    np.testing.assert_allclose(render(photo, r, at=0.0), photo, atol=1e-6)
    np.testing.assert_allclose(render(photo, r, at=1.0), np.clip(photo * 2.0, 0, 1), atol=1e-5)


def test_render_frames_yields_every_frame_and_computes_the_still_steps_once(photo, tmp_path, throwaway_effects):
    runs = []

    @effect(category="utilities", params=AmountParams)
    def still_probe(rgb, params):
        """A step that does not change over the clip; counts how often it runs."""
        runs.append(params.amount)
        return rgb

    animated = load_recipe(write(tmp_path, ANIMATED))
    r = Recipe("r", (Step("still_probe"), *animated.steps), size=None, animation=animated.animation)
    frames = list(render_frames(photo, r))
    # the still step ran once for the whole clip
    assert len(frames) == 20
    assert len(runs) == 1
    np.testing.assert_allclose(frames[0], render(photo, r, at=0.0), atol=1e-6)
    np.testing.assert_allclose(frames[-1], render(photo, r, at=1.0), atol=1e-6)


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


def test_render_frames_needs_an_animated_recipe(photo):
    with pytest.raises(RecipeError, match="no animation"):
        list(render_frames(photo, Recipe("still", (Step("grain"),), size=None)))


def test_size_scales_photo_and_matte_together(photo, matte):
    out = render(photo, Recipe("s", (Step("fill", {"color": "#000000"}, "background"),), size=64), matte)
    assert out.shape == (48, 64, 3)


def test_matte_of_the_wrong_size_is_rejected(photo):
    with pytest.raises(SubjectNotFoundError, match="matte"):
        render(photo, Recipe("r", (Step("grain"),), size=None), np.ones((5, 5), np.float32))


def test_shipped_recipes_all_load():
    files = sorted((Path(__file__).parent.parent / "recipes").glob("*.toml"))
    assert files
    assert all(load_recipe(f).steps for f in files)


def test_the_facade_finds_the_subject_with_matte_subject_matte_by_default(echo_model, monkeypatch):
    """Without a matte or `find_subject`, `photo_fx.recipe`'s functions find the subject with `subject_matte`: the
    results must equal the core's given `subject_matte`'s answer. The portable backend with a stand-in model (the
    external boundary, see `echo_model`) is chosen through the environment, so the test is the same on every OS."""
    monkeypatch.setenv("PHOTO_FX_SEGMENTER", "onnx")
    monkeypatch.setenv("PHOTO_FX_MODEL", str(echo_model(0)))
    photo = resize(imageio.load(EXAMPLE_PHOTO), 160)
    masked = Recipe("m", (Step("fill", {"color": "#000000"}, "background"),), size=None)
    animated = Recipe("a", (*masked.steps, Step("grade", {"exposure": Animated(0.0, 1.0)})), None, Animation(2, 1))
    alpha = subject_matte(photo)
    expected = runner.render(photo, masked, alpha)
    assert not np.array_equal(expected, runner.render(photo, masked, np.ones_like(alpha)))  # the subject matters
    np.testing.assert_array_equal(facade.render(photo, masked), expected)
    np.testing.assert_array_equal(facade.trace(photo, masked)[-1].image, expected)
    np.testing.assert_array_equal(facade.render_all(photo, [masked])[0], expected)
    clip = list(facade.render_frames(photo, animated))
    np.testing.assert_array_equal(clip[0], next(runner.render_frames(photo, animated, alpha)))


def test_the_facade_uses_a_given_finder_instead_of_the_default(photo, matte):
    masked = Recipe("m", (Step("fill", {"color": "#000000"}, "background"),), size=None)
    found = facade.render(photo, masked, find_subject=lambda rgb: 1.0 - matte)
    np.testing.assert_array_equal(found, runner.render(photo, masked, 1.0 - matte))
    assert not np.array_equal(found, runner.render(photo, masked, matte))
