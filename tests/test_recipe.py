import numpy as np
import pytest

from photo_fx import Recipe, Step, load_recipe, render, render_all
from photo_fx.matte import _refine_edge
from photo_fx.recipe import Animated, Animation, render_frames


def write(tmp_path, text, name="r.toml"):
    path = tmp_path / name
    path.write_text(text)
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
    "text, expect",
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


def test_masked_step_changes_only_the_masked_region(photo, matte):
    r = Recipe("bg", (Step("fill", {"color": "#000000"}, "background"),), size=None)
    out = render(photo, r, matte)
    inside, outside = matte == 1, matte == 0
    np.testing.assert_allclose(out[inside], photo[inside], atol=1e-6)
    assert np.allclose(out[outside], 0.0)


def test_subject_mask_is_the_mirror_image(photo, matte):
    r = Recipe("s", (Step("fill", {"color": "#ffffff"}, "subject"),), size=None)
    out = render(photo, r, matte)
    np.testing.assert_allclose(out[matte == 0], photo[matte == 0], atol=1e-6)
    assert np.allclose(out[matte == 1], 1.0)


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
    assert ok.steps[0].opacity == 0.4 and "opacity" not in ok.steps[0].params
    with pytest.raises(ValueError, match="step 1.*opacity"):
        load_recipe(write(tmp_path, '[[step]]\neffect="grain"\nopacity=1.5\n', "bad.toml"))


def test_effects_that_declare_a_matte_parameter_get_the_person_silhouette(photo, matte, monkeypatch):
    from photo_fx.effects import _registry

    seen = []
    monkeypatch.setitem(_registry._EFFECTS, "spy", lambda rgb, matte=None: seen.append(matte) or rgb)
    r = Recipe("s", (Step("spy"),), size=None)
    assert r.needs_matte  # an effect that wants the silhouette makes the recipe look for the person
    render(photo, r, matte)
    assert len(seen) == 1 and seen[0] is matte


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
    assert r.steps[0].params == {"exposure": Animated(0.0, 1.0, "in-out")} and r.steps[0].animated


def test_animation_positions_hold_at_both_ends_and_run_in_between():
    positions = Animation(seconds=2, fps=10, hold_start=0.5, hold_end=0.5).positions()
    assert len(positions) == 20
    assert positions[:6] == [0.0] * 6 and positions[-5:] == [1.0] * 5  # held, then still at the end
    assert positions == sorted(positions)


def test_pingpong_plays_forward_then_back_without_repeating_the_turning_frames():
    once = Animation(seconds=1, fps=5).positions()
    both = Animation(seconds=1, fps=5, loop="pingpong").positions()
    assert both == once + once[-2:0:-1] and len(both) == 2 * len(once) - 2


def test_animated_values_ease_between_their_end_points():
    for ease in ("linear", "in", "out", "in-out"):
        value = Animated(2.0, 6.0, ease)
        assert value.at(0.0) == 2.0 and value.at(1.0) == 6.0
        assert value.at(0.25) <= value.at(0.5) <= value.at(0.75)
    assert Animated(0.0, 1.0, "in-out").at(0.5) == 0.5 and Animated(0.0, 1.0, "in").at(0.5) < 0.5


@pytest.mark.parametrize(
    "text, expect",
    [
        ('[[step]]\neffect="grade"\nexposure={from=0, to=1}\n', "no \\[animation\\] table"),
        (ANIMATED.replace('{ from = 0.0, to = 1.0, ease = "in-out" }', "{ from = 0.0 }"), "step 1.*exposure"),
        (ANIMATED.replace('"in-out"', '"bouncy"'), "step 1.*exposure"),
        (ANIMATED.replace("hold_end = 0.5", "hold_end = 1.5"), "no time"),
        (ANIMATED.replace("seconds = 2", "speed = 2"), "needs `seconds`"),
        (ANIMATED.replace("hold_end = 0.5", 'loop = "spiral"'), "loop"),
    ],
)
def test_bad_animations_fail_at_load_time(tmp_path, text, expect):
    with pytest.raises(ValueError, match=expect):
        load_recipe(write(tmp_path, text))


def test_cubic_bezier_easing_slow_in_slow_out_without_overshoot():
    gentle = Animated(0.0, 1.0, "gentle")
    values = [gentle.at(i / 100) for i in range(101)]
    assert values[0] == 0.0 and abs(values[-1] - 1.0) < 1e-9
    assert values == sorted(values) and max(values) <= 1.0 + 1e-9  # smooth, never past the target
    assert gentle.at(0.1) < 0.1 and gentle.at(0.9) > 0.9  # slow start, slow finish
    assert Animated(0, 1, (0.4, 0.0, 0.2, 1.0)).at(0.3) == gentle.at(0.3)  # the preset is that curve


@pytest.mark.parametrize("name", ["gentle", "standard", "accelerate", "decelerate", "emphasized", "sine"])
def test_every_named_curve_runs_from_zero_to_one(name):
    curve = Animated(0.0, 1.0, name)
    assert abs(curve.at(0.0)) < 1e-9 and abs(curve.at(1.0) - 1.0) < 1e-9


def test_exit_and_entrance_curves_accelerate_and_decelerate():
    assert Animated(0, 1, "accelerate").at(0.3) < 0.3 < Animated(0, 1, "decelerate").at(0.3)


@pytest.mark.parametrize("ease", ["[0.4, 0.0, 0.2]", "[1.5, 0.0, 0.2, 1.0]", '"wobbly"', '[0.4, 0.0, "x", 1.0]'])
def test_bad_easing_is_rejected_when_loading(tmp_path, ease):
    text = ANIMATED.replace('ease = "in-out"', f"ease = {ease}")
    with pytest.raises(ValueError, match="step 1.*exposure"):
        load_recipe(write(tmp_path, text))


def test_a_custom_cubic_bezier_can_be_written_as_four_numbers(tmp_path):
    r = load_recipe(write(tmp_path, ANIMATED.replace('ease = "in-out"', "ease = [0.4, 0.0, 0.2, 1.0]")))
    assert r.steps[0].params["exposure"].ease == (0.4, 0.0, 0.2, 1.0)


def test_over_clip_keeps_running_through_the_holds():
    still = Animated(0.0, 10.0, over="motion")
    clock = Animated(0.0, 10.0, over="clip")
    assert still.at(0.0, clip=0.5) == 0.0 and clock.at(0.0, clip=0.5) == 5.0  # held at its start / still running
    frames = Animation(seconds=2, fps=10, hold_start=0.5, hold_end=0.5).frames()
    assert frames[0] == (0.0, 0.0) and frames[3] == (0.0, 3 / 19) and frames[-1] == (1.0, 1.0)
    assert [clip for _, clip in frames] == sorted(clip for _, clip in frames)


def test_over_must_be_motion_or_clip(tmp_path):
    with pytest.raises(ValueError, match="step 1.*exposure"):
        load_recipe(write(tmp_path, ANIMATED.replace('ease = "in-out"', 'over = "forever"')))


def test_render_at_picks_a_moment_of_the_clip(photo, tmp_path):
    r = load_recipe(write(tmp_path, ANIMATED.replace("size", "size")))
    r = Recipe(r.name, r.steps, size=None, animation=r.animation)
    np.testing.assert_allclose(render(photo, r, at=0.0), photo, atol=1e-6)
    np.testing.assert_allclose(render(photo, r, at=1.0), np.clip(photo * 2.0, 0, 1), atol=1e-5)


def test_render_frames_yields_every_frame_and_computes_the_still_steps_once(photo, tmp_path, monkeypatch):
    r = load_recipe(write(tmp_path, ANIMATED.replace("[[step]]", '[[step]]\neffect="grain"\n[[step]]', 1)))
    r = Recipe(r.name, r.steps, size=None, animation=r.animation)
    calls = []
    real = effects_apply = __import__("photo_fx").recipe.effects.apply
    monkeypatch.setattr(
        "photo_fx.recipe.effects.apply", lambda name, *a, **k: calls.append(name) or real(name, *a, **k)
    )
    frames = list(render_frames(photo, r))
    assert len(frames) == 20 and calls.count("grain") == 1 and calls.count("grade") == 20
    np.testing.assert_allclose(frames[0], render(photo, r, at=0.0), atol=1e-6)
    assert effects_apply is real


def test_render_frames_needs_an_animated_recipe(photo):
    with pytest.raises(ValueError, match="no \\[animation\\]"):
        list(render_frames(photo, Recipe("still", (Step("grain"),), size=None)))


def test_size_scales_photo_and_matte_together(photo, matte):
    out = render(photo, Recipe("s", (Step("fill", {"color": "#000000"}, "background"),), size=64), matte)
    assert out.shape == (48, 64, 3)


def test_matte_of_the_wrong_size_is_rejected(photo):
    with pytest.raises(ValueError, match="matte"):
        render(photo, Recipe("r", (Step("grain"),), size=None), np.ones((5, 5), np.float32))


def test_shipped_recipes_all_load():
    from pathlib import Path

    files = sorted((Path(__file__).parent.parent / "recipes").glob("*.toml"))
    assert files and all(load_recipe(f).steps for f in files)


def test_render_all_finds_the_person_once_and_only_if_needed(photo, matte, monkeypatch):
    calls = []
    monkeypatch.setattr("photo_fx.recipe.subject_matte", lambda rgb: calls.append(1) or matte)
    masked = Recipe("m", (Step("fill", {"color": "#000000"}, "background"),), size=None)
    plain = Recipe("p", (Step("grade", {"exposure": 0.5}),), size=None)

    assert len(render_all(photo, [masked, masked, plain])) == 3
    assert calls == [1]
    render_all(photo, [plain])
    assert calls == [1]  # no masked recipe, so no second search


# --- matte edge refinement (the part of matte.py that runs anywhere) -----------------------------------------------


def test_refine_edge_shrink_pulls_the_edge_in():
    alpha = np.zeros((40, 40), np.float32)
    alpha[10:30, 10:30] = 1
    assert _refine_edge(alpha, shrink=3, feather=0).sum() < alpha.sum()
    np.testing.assert_array_equal(_refine_edge(alpha, shrink=0, feather=0), alpha)


def test_refine_edge_feather_softens_without_leaving_range():
    alpha = np.zeros((40, 40), np.float32)
    alpha[10:30, 10:30] = 1
    soft = _refine_edge(alpha, shrink=0, feather=2)
    assert 0 < soft[10, 20] < 1 and soft.min() >= 0 and soft.max() <= 1
