import sys

import pytest
from PIL import GifImagePlugin, Image

from photo_fx import effects, imageio
from photo_fx.cli import main
from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect


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
    assert listed == set(effects.describe())  # one unindented heading per effect, its documentation indented below


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
