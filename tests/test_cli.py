import os
import re
import signal
import subprocess
import sys
import tomllib
from pathlib import Path

import numpy as np
import pytest
from PIL import GifImagePlugin, Image

import photo_fx
from photo_fx import PhotoFxError, imageio
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


def test_run_finds_the_subjects_again_when_the_segmenter_model_changes(cut, echo_model, monkeypatch):
    (cut / "photo-fx.toml").write_text('photos = ["me.png"]\nrecipes = ["cut.toml"]\noutput = "done"\n')
    command = ["run", str(cut / "photo-fx.toml"), "--segmenter", "onnx", "-q"]
    monkeypatch.setenv("PHOTO_FX_MODEL", str(echo_model(0)))  # the subject is where the photo is red
    assert main(command) == 0
    first = (cut / "done" / "me" / "cut.jpg").read_bytes()
    monkeypatch.setenv("PHOTO_FX_MODEL", str(echo_model(1)))  # and now where it is green: another cut-out
    assert main(command) == 0
    assert (cut / "done" / "me" / "cut.jpg").read_bytes() != first


# --- user mistakes are reported, not thrown at the user ------------------------------------------------------------

ROOT = Path(__file__).resolve().parents[1]


def user_mistakes(files):
    """Commands a user can get wrong, each with a word the error message must contain."""
    photo, recipe = str(files / "me.png"), str(files / "recipes" / "a.toml")
    (files / "notimage.png").write_text("this is text, not a picture", encoding="utf-8")
    (files / "blocker").write_text("a file where a folder is wanted", encoding="utf-8")
    (files / "broken.toml").write_text("[[step]\neffect=", encoding="utf-8")
    (files / "unknown.toml").write_text('[[step]]\neffect="nope"\n', encoding="utf-8")
    out = ["-o", str(files / "out")]
    (files / "empty").mkdir(exist_ok=True)
    (files / "cut.toml").write_text(f'name="cut"\nsize=64\n{CUT}', encoding="utf-8")  # needs the subject
    (files / "clip.toml").write_text('name="clip"\nsize=32\n[animation]\nseconds=0.2\nfps=5\n' + CUT, encoding="utf-8")
    return {
        "a file that is not an image": (["render", str(files / "notimage.png"), recipe, *out], "notimage.png"),
        "a folder instead of the photo": (["render", str(files), recipe, *out], str(files)),
        "a photo that does not exist": (["render", str(files / "gone.png"), recipe, *out], "gone.png"),
        "an output below a file": (["render", photo, recipe, "-o", str(files / "blocker" / "sub")], "blocker"),
        "a recipe with a syntax error": (["render", photo, str(files / "broken.toml"), *out], "broken.toml"),
        "an unknown effect": (["render", photo, str(files / "unknown.toml"), *out], "nope"),
        "a project file that does not exist": (["run", str(files / "gone.toml")], "gone.toml"),
        "a matte that is not an image": (["cutout", photo, *out, "--matte", str(files / "notimage.png")], "notimage"),
        "a folder with no recipe in it": (["render", photo, str(files / "empty"), *out], "empty"),
        "a picture type nothing can write": (["sheet", photo, recipe, "-o", str(files / "sheet.xyz")], ".xyz"),
        "a video type nothing can write": (
            ["animate", photo, str(files / "clip.toml"), "-o", str(files / "c.avi")],
            ".avi",
        ),
        # (the suite's PHOTO_FX_MODEL names no file, so the portable model cannot run)
        "a segmenter that cannot run": (
            ["render", photo, str(files / "cut.toml"), *out, "--segmenter", "onnx"],
            "no-such-model",
        ),
    }


MISTAKES = [
    "a file that is not an image", "a folder instead of the photo", "a photo that does not exist",
    "an output below a file", "a recipe with a syntax error", "an unknown effect",
    "a project file that does not exist", "a matte that is not an image", "a folder with no recipe in it",
    "a picture type nothing can write", "a video type nothing can write", "a segmenter that cannot run",
]  # fmt: skip


@pytest.mark.parametrize("mistake", MISTAKES)
def test_a_user_mistake_ends_in_one_error_line_on_stderr_and_exit_1(files, capsys, mistake):
    command, offender = user_mistakes(files)[mistake]
    assert main(command) == 1
    shown = capsys.readouterr()
    last_line = shown.err.strip().splitlines()[-1]  # progress lines may come first
    assert last_line.startswith("error: ")
    assert offender in last_line
    assert "Traceback" not in shown.err
    assert shown.out == ""  # results go to stdout, errors only to stderr


def test_an_unknown_segmenter_in_the_environment_ends_in_one_error_line(cut, capsys, monkeypatch):
    monkeypatch.setenv("PHOTO_FX_SEGMENTER", "coreml")
    assert main(["render", str(cut / "me.png"), str(cut / "cut.toml"), "-o", str(cut / "out")]) == 1
    last_line = capsys.readouterr().err.strip().splitlines()[-1]
    assert last_line.startswith("error: ")
    assert "PHOTO_FX_SEGMENTER" in last_line
    (cut / "photo-fx.toml").write_text('photos = ["me.png"]\nrecipes = ["cut.toml"]\noutput = "done"\n')
    assert main(["run", str(cut / "photo-fx.toml")]) == 1  # a project that needs the subject says so at once
    assert "PHOTO_FX_SEGMENTER" in capsys.readouterr().err.strip().splitlines()[-1]


class WrongParams(EffectParams):
    """A probe effect without settings."""

    amount: float = param(0.5, unit="fraction", min=0, max=1, doc="Unused.")


def test_a_bug_in_an_effect_is_not_dressed_up_as_a_mistake_of_the_user(files, throwaway_effects):
    """The command line catches `PhotoFxError` and `OSError`, nothing wider: a plain ValueError out of an effect is a
    bug, and the developer needs its traceback."""

    @effect(category="utilities", params=WrongParams)
    def buggy_probe(rgb, params):
        """Fails with a plain ValueError, as a bug would."""
        raise ValueError("a bug, not a user mistake")

    (files / "buggy.toml").write_text('name="buggy"\nsize=32\n[[step]]\neffect="buggy_probe"\n', encoding="utf-8")
    with pytest.raises(ValueError, match="a bug"):
        main(["render", str(files / "me.png"), str(files / "buggy.toml"), "-o", str(files / "out")])


@pytest.mark.parametrize("columns", ["0", "-2", "many"])
def test_sheet_columns_must_be_a_positive_whole_number(files, capsys, columns):
    command = ["sheet", str(files / "me.png"), str(files / "recipes"), "-o", str(files / "s.png")]
    with pytest.raises(SystemExit) as caught:
        main([*command, "--columns", columns])
    assert caught.value.code == 2
    assert "--columns" in capsys.readouterr().err
    assert main([*command, "--columns", "2"]) == 0  # the known-good side


def test_the_same_commands_with_valid_input_succeed(files):
    """The known-good side of the table above: only the mistake makes the difference."""
    assert main(["render", str(files / "me.png"), str(files / "recipes" / "a.toml"), "-o", str(files / "out")]) == 0


def test_render_prints_each_file_it_wrote_to_stdout(files, capsys):
    main(["render", str(files / "me.png"), str(files / "recipes"), "-o", str(files / "out")])
    assert capsys.readouterr().out.split("\n")[:-1] == [str(files / "out" / f"me__{n}.png") for n in "ab"]


def test_debug_lets_the_original_exception_through(files):
    command, _ = user_mistakes(files)["a photo that does not exist"]
    with pytest.raises(OSError, match=r"gone\.png"):
        main([*command, "--debug"])
    command, _ = user_mistakes(files)["an unknown effect"]
    with pytest.raises(PhotoFxError, match="nope"):
        main([*command, "--debug"])


def test_photo_fx_debug_in_the_environment_does_the_same(files, monkeypatch):
    command, offender = user_mistakes(files)["an unknown effect"]
    monkeypatch.setenv("PHOTO_FX_DEBUG", "1")
    with pytest.raises(PhotoFxError, match=offender):
        main(command)
    monkeypatch.setenv("PHOTO_FX_DEBUG", "0")  # anything but 1 leaves the friendly error on
    assert main(command) == 1


def photo_fx_process(*args: str, **options) -> subprocess.Popen:
    """`python -m photo_fx ARGS` in a fresh interpreter (the way a user starts it), stderr and stdout as text."""
    environment = {key: value for key, value in os.environ.items() if key != "PHOTO_FX_DEBUG"}
    return subprocess.Popen(
        [sys.executable, "-m", "photo_fx", *args], cwd=ROOT, env=environment, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, **options,
    )  # fmt: skip


@pytest.mark.skipif(sys.platform == "win32", reason="the test interrupts the process with SIGINT")
def test_ctrl_c_stops_with_one_line_and_exit_130(files):
    (files / "long.toml").write_text(
        'name="long"\nsize=32\n[animation]\nseconds=600\nfps=30\n[[step]]\neffect="grade"\nexposure={from=0,to=1}\n',
        encoding="utf-8",
    )
    process = photo_fx_process("animate", str(files / "me.png"), str(files / "long.toml"), "-o", str(files / "c.gif"))
    assert process.stderr is not None
    for line in process.stderr:  # the first progress line says the render is running, so the signal arrives inside main
        if line.startswith("frame"):
            break
    else:
        pytest.fail(f"the process ended before it reported progress (exit {process.wait()})")
    process.send_signal(signal.SIGINT)
    _, rest = process.communicate(timeout=60)
    assert process.returncode == 130
    assert "interrupted" in rest
    assert "Traceback" not in rest


# --- the program itself: --version, python -m photo_fx, the exit codes in --help --------------------------------------


def test_version_is_the_one_in_pyproject(capsys):
    declared = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))["project"]["version"]
    with pytest.raises(SystemExit) as caught:
        main(["--version"])
    assert caught.value.code == 0
    assert capsys.readouterr().out.strip() == f"photo-fx {declared}"
    assert photo_fx.__version__ == declared


@pytest.mark.parametrize("option", ["--help", "--version"])
def test_help_and_version_load_no_image_library(option):
    """`photo-fx --help` and `--version` answer at once: the libraries that render pictures (numpy, scipy, Pillow,
    pydantic, the effects, the subject finder, the pipeline) load only when a command needs them."""
    heavy = ["numpy", "scipy", "PIL", "pydantic", "photo_fx.core", "photo_fx.effects", "photo_fx.matte",
             "photo_fx.pipeline"]  # fmt: skip
    code = (
        "import json, sys\n"
        "from photo_fx.cli import main\n"
        f"try:\n    main([{option!r}])\nexcept SystemExit:\n    pass\n"
        f"json.dump([m for m in {heavy!r} if m in sys.modules], sys.stderr)\n"
    )
    done = subprocess.run([sys.executable, "-P", "-c", code], capture_output=True, text=True, check=False, cwd=ROOT)
    assert done.returncode == 0, done.stderr
    assert done.stdout.startswith(("usage: photo-fx", "photo-fx "))  # it really answered
    assert done.stderr == "[]"


def test_python_dash_m_photo_fx_runs_the_command_line():
    process = photo_fx_process("--version")
    out, err = process.communicate(timeout=60)
    assert process.returncode == 0, err
    assert out.startswith("photo-fx ")


def test_python_dash_m_photo_fx_reports_an_error_with_exit_1_and_no_traceback(tmp_path):
    process = photo_fx_process("render", str(tmp_path / "gone.png"), str(tmp_path / "r.toml"))
    _, err = process.communicate(timeout=60)
    assert process.returncode == 1
    assert err.startswith("error: ")
    assert "Traceback" not in err


def test_the_top_level_help_documents_the_exit_codes(capsys):
    with pytest.raises(SystemExit):
        main(["--help"])
    text = capsys.readouterr().out
    for code in ("0", "1", "2", "130"):
        assert re.search(rf"^\s+{code}\s+\S", text, re.MULTILINE), f"exit code {code} is not explained:\n{text}"


def commands_in_help(capsys) -> list[str]:
    with pytest.raises(SystemExit):
        main(["--help"])
    listed = re.search(r"\{([a-z,-]+)\}", capsys.readouterr().out)
    assert listed, "the help shows no list of commands"
    return listed.group(1).split(",")


def test_every_command_has_a_help_with_an_example(capsys):
    commands = commands_in_help(capsys)
    assert {"render", "run", "effects"} <= set(commands)
    for command in commands:
        with pytest.raises(SystemExit) as caught:
            main([command, "--help"])
        assert caught.value.code == 0
        text = capsys.readouterr().out
        assert re.search(rf"^example: photo-fx {command}\b", text, re.MULTILINE), f"{command}: no example in\n{text}"


@pytest.mark.parametrize("command", ["gallery", "regenerate", "new-effect"])
def test_developer_commands_are_not_part_of_the_installed_cli(command, capsys):
    """They need a source checkout, so `poe` runs them from `tools/dev.py`."""
    assert command not in commands_in_help(capsys)
    with pytest.raises(SystemExit) as caught:
        main([command])
    assert caught.value.code == 2
    assert "invalid choice" in capsys.readouterr().err


def test_quiet_hides_the_progress_lines_but_not_the_results(files, capsys):
    command = ["render", str(files / "me.png"), str(files / "recipes"), "-o", str(files / "out")]
    main(command)
    loud = capsys.readouterr()
    assert loud.err != ""  # progress is shown by default
    assert main([*command, "-q"]) == 0
    quiet = capsys.readouterr()
    assert quiet.err == ""
    assert quiet.out == loud.out
