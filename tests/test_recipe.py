import numpy as np
import pytest

from portrait_fx import Recipe, Step, load_recipe, render, render_all
from portrait_fx.matte import _refine_edge


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
    monkeypatch.setattr("portrait_fx.recipe.subject_matte", lambda rgb: calls.append(1) or matte)
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
