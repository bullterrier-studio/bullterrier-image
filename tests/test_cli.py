import sys

import numpy as np
import pytest
from PIL import GifImagePlugin, Image

from photo_fx import imageio
from photo_fx.cli import main
from photo_fx.core import registry
from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.matte import subject_matte
from photo_fx.recipe import load_recipe, render


@pytest.fixture
def files(photo, tmp_path):
    imageio.save(photo, tmp_path / "me.png")
    recipes = tmp_path / "recipes"
    recipes.mkdir()
    (recipes / "a.toml").write_text('name="a"\nsize=64\n[[step]]\neffect="grain"\n')
    (recipes / "b.toml").write_text('name="b"\nsize=64\n[[step]]\neffect="glitch"\n')
    return tmp_path


def test_effects_lists_every_effect(capsys):
    assert main(["effects"]) == 0
    listed = {line for line in capsys.readouterr().out.splitlines() if line and not line.startswith(" ")}
    assert listed == set(registry.describe())  # one unindented heading per effect, its documentation indented below


class WickParams(EffectParams):
    """The wick probe's settings."""

    length: float = param(2.0, unit="px", min=0, max=9, doc="Length of the candle wick, a rather distinctive phrase.")


def test_effects_prints_every_parameter_of_a_modelled_effect(capsys, throwaway_effects):
    @effect(category="light", params=WickParams)
    def wick_probe(rgb, params):
        """Lights a candle."""
        return rgb

    assert main(["effects"]) == 0
    out = capsys.readouterr().out
    entry = out[out.index("wick_probe\n") :]
    assert "    Lights a candle." in entry
    assert "    length : number, px, 0 to 9, default 2.0" in entry
    assert "Length of the candle wick, a rather distinctive phrase." in entry


def test_render_writes_one_file_per_recipe_in_a_folder(files):
    assert main(["render", str(files / "me.png"), str(files / "recipes"), "-o", str(files / "out")]) == 0
    assert sorted(p.name for p in (files / "out").iterdir()) == ["me__a.png", "me__b.png"]


def test_sheet_combines_recipes_into_one_image(files):
    assert main(["sheet", str(files / "me.png"), str(files / "recipes"), "-o", str(files / "s.png")]) == 0
    with Image.open(files / "s.png") as sheet:
        # two 480 px tiles side by side, in one row
        assert sheet.width > 2 * 480
        assert sheet.height < 2 * 480


def test_bad_recipe_is_reported_without_a_traceback(files, capsys):
    (files / "recipes" / "c.toml").write_text('[[step]]\neffect="nope"\n')
    assert main(["render", str(files / "me.png"), str(files / "recipes"), "-o", str(files / "o")]) == 1
    assert "c.toml" in capsys.readouterr().err
    assert not (files / "o").exists()  # failed before doing any work


@pytest.mark.skipif(sys.platform != "darwin", reason="the subject matte needs Apple Vision")
def test_photo_without_a_subject_is_reported_clearly(files, capsys):
    (files / "recipes" / "m.toml").write_text('[[step]]\neffect="fill"\nmask="background"\n')
    assert main(["render", str(files / "me.png"), str(files / "recipes" / "m.toml")]) == 1
    assert "no subject" in capsys.readouterr().err.lower()


def test_animate_writes_the_clip(files):
    recipe = files / "recipes" / "anim.toml"
    recipe.write_text(
        'name="anim"\nsize=48\n[animation]\nseconds=0.5\nfps=8\n[[step]]\neffect="grade"\nexposure={from=0,to=1}\n'
    )
    target = files / "clip.gif"
    assert main(["animate", str(files / "me.png"), str(recipe), "-o", str(target)]) == 0
    with Image.open(target) as clip:
        assert isinstance(clip, GifImagePlugin.GifImageFile)
        assert clip.n_frames == 4


def test_animate_explains_that_a_still_recipe_has_nothing_to_animate(files, capsys):
    assert main(["animate", str(files / "me.png"), str(files / "recipes" / "a.toml")]) == 1
    assert "no [animation]" in capsys.readouterr().err


# --- the subject: --matte (bring your own) and --segmenter (which model finds it) -----------------------------------

CUT = '[[step]]\neffect="fill"\ncolor="#000000"\nmask="background"\n'  # black wherever the matte is 0


@pytest.fixture
def cut(files):
    """A recipe that blacks out the background, at the photo's own 128x96 size, and a matte file for that photo: the
    left half background, the right half subject."""
    (files / "cut.toml").write_text(f'name="cut"\nsize=128\n{CUT}')
    halves = np.zeros((96, 128), np.uint8)
    halves[:, 64:] = 255
    Image.fromarray(halves, "L").save(files / "matte.png")
    return files


def test_render_with_a_matte_file_keeps_exactly_the_subject_it_marks(cut, photo):
    assert main(["render", str(cut / "me.png"), str(cut / "cut.toml"), "-o", str(cut / "out"), "--matte",
                 str(cut / "matte.png")]) == 0  # fmt: skip
    out = imageio.load(cut / "out" / "me__cut.png")
    assert out[:, :64].max() == 0.0  # background: filled black
    np.testing.assert_allclose(out[:, 64:], imageio.load(cut / "me.png")[:, 64:])  # subject: untouched


def test_cutout_with_a_matte_file_uses_it_as_the_transparency(cut):
    assert main(["cutout", str(cut / "me.png"), "-o", str(cut / "c.png"), "--matte", str(cut / "matte.png")]) == 0
    with Image.open(cut / "c.png") as img:
        alpha = np.asarray(img)[..., 3]
    assert alpha[:, :64].max() == 0
    assert alpha[:, 64:].min() == 255


def test_a_matte_file_of_another_size_is_refused(cut, capsys):
    Image.new("L", (50, 40), 255).save(cut / "small.png")
    for command in (["render", str(cut / "me.png"), str(cut / "cut.toml")], ["cutout", str(cut / "me.png")]):
        assert main([*command, "-o", str(cut / "x"), "--matte", str(cut / "small.png")]) == 1
        assert "matte" in capsys.readouterr().err


def test_animate_and_steps_take_a_matte_file_too(cut):
    (cut / "anim.toml").write_text(f'name="anim"\nsize=48\n[animation]\nseconds=0.25\nfps=8\n{CUT}')
    target = cut / "clip.gif"
    assert main(["animate", str(cut / "me.png"), str(cut / "anim.toml"), "-o", str(target), "--matte",
                 str(cut / "matte.png")]) == 0  # fmt: skip
    with Image.open(target) as clip:
        frame = np.asarray(clip.convert("RGB"))
    assert frame.shape == (36, 48, 3)  # scaled with the photo to the recipe's 48 px
    assert frame[:, :22].max() <= 2  # the left half is still the background (black, within the GIF palette)
    assert frame[:, 26:].mean() > 50  # and the right half is still the photo
    assert main(["steps", str(cut / "me.png"), str(cut / "cut.toml"), "-o", str(cut / "st"), "--matte",
                 str(cut / "matte.png")]) == 0  # fmt: skip
    assert any((cut / "st").iterdir())


def test_the_segmenter_option_wins_over_the_environment(cut, echo_model, monkeypatch):
    monkeypatch.setenv("PHOTO_FX_SEGMENTER", "nonsense")  # refused if it were asked
    monkeypatch.setenv("PHOTO_FX_MODEL", str(echo_model(0)))  # the portable backend answers with the red channel
    assert main(["render", str(cut / "me.png"), str(cut / "cut.toml"), "-o", str(cut / "out"),
                 "--segmenter", "onnx"]) == 0  # fmt: skip
    photo = imageio.load(cut / "me.png")
    expected = render(photo, load_recipe(cut / "cut.toml"), subject_matte(photo, segmenter="onnx"))
    np.testing.assert_allclose(imageio.load(cut / "out" / "me__cut.png"), expected, atol=0.5 / 255)


def test_an_unknown_segmenter_is_a_usage_error(cut, capsys):
    with pytest.raises(SystemExit) as caught:
        main(["render", str(cut / "me.png"), str(cut / "cut.toml"), "--segmenter", "coreml"])
    assert caught.value.code == 2
    assert "coreml" in capsys.readouterr().err


def test_run_finds_subjects_with_the_chosen_segmenter(cut, echo_model, monkeypatch):
    monkeypatch.setenv("PHOTO_FX_SEGMENTER", "nonsense")
    monkeypatch.setenv("PHOTO_FX_MODEL", str(echo_model(0)))
    (cut / "photo-fx.toml").write_text('photos = ["me.png"]\nrecipes = ["cut.toml"]\noutput = "done"\n')
    assert main(["run", str(cut / "photo-fx.toml"), "--segmenter", "onnx"]) == 0
    assert (cut / "done" / "me" / "cut.jpg").exists()
