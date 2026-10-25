import itertools
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

import numpy as np
import pytest
from PIL import Image

import bullterrier_image
from bullterrier_image import BullterrierImageError, imageio
from bullterrier_image.cli import main
from bullterrier_image.core import registry
from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.finding.subject import subject_matte
from bullterrier_image.recipe import Recipe, Step, load_recipe, render
from bullterrier_image.sheet import contact_sheet

ROOT = Path(__file__).resolve().parents[1]
CUT = '[[step]]\neffect="fill"\ncolor="#000000"\nmask="background"\n'  # black wherever the matte is 0
PROJECT = 'photos = ["me.png"]\nrecipes = ["cut.toml"]\noutput = "done"\n'


def cli(command: str) -> int:
    """Run the command line as a shell would split `command` (no argument holds a space), in the test's folder."""
    return main(command.split())


@pytest.fixture
def files(photo, tmp_path, monkeypatch):
    """The test's working folder: me.png (the 128x96 photo) and recipes/a.toml (grain) and recipes/b.toml (glitch)."""
    monkeypatch.chdir(tmp_path)  # commands name files as a user in that folder would
    imageio.save(photo, tmp_path / "me.png")
    (tmp_path / "recipes").mkdir()
    (tmp_path / "recipes" / "a.toml").write_text('name="a"\nsize=64\n[[step]]\neffect="grain"\n', encoding="utf-8")
    (tmp_path / "recipes" / "b.toml").write_text('name="b"\nsize=64\n[[step]]\neffect="glitch"\n', encoding="utf-8")
    return tmp_path


@pytest.fixture
def cut(files):
    """Adds cut.toml, a recipe that blacks out the background at the photo's own size, and matte.png, a matte for the
    photo: the left half background, the right half subject."""
    (files / "cut.toml").write_text(f'name="cut"\nsize=128\n{CUT}', encoding="utf-8")
    halves = np.zeros((96, 128), np.uint8)
    halves[:, 64:] = 255
    Image.fromarray(halves, "L").save(files / "matte.png")
    return files


def test_effects_lists_every_effect(capsys):
    assert cli("effects") == 0
    listed = {line for line in capsys.readouterr().out.splitlines() if line and not line.startswith(" ")}
    assert listed == set(registry.describe())  # one unindented heading per effect, its documentation indented below


class ProbeParams(EffectParams):
    """The probe effects' one setting."""

    length: float = param(2.0, unit="px", min=0, max=9, doc="Length of the candle wick, a rather distinctive phrase.")


def test_effects_prints_every_parameter_of_a_modelled_effect(capsys, throwaway_effects):
    @effect(category="light", params=ProbeParams)
    def wick_probe(rgb, params):
        """Lights a candle."""
        return rgb

    assert cli("effects") == 0
    out = capsys.readouterr().out
    entry = out[out.index("wick_probe\n") :]
    assert "    Lights a candle." in entry
    assert "    length : number, px, 0 to 9, default 2.0" in entry
    assert "Length of the candle wick, a rather distinctive phrase." in entry


class TunedParams(EffectParams):
    """A look control and two technique options."""

    amount: float = param(0.5, unit="fraction", min=0, max=1, doc="How much of the look; a rather distinctive phrase.")
    softness: float = param(2.0, unit="px", min=0, max=9, advanced=True, doc="Blur of the technique's mask.")
    edge: str = param("reflect", advanced=True, doc="Past the border, e.g. reflect.")


@pytest.fixture
def tuned_probe(throwaway_effects):
    @effect(category="light", params=TunedParams)
    def tuned_probe(rgb, params):
        """Tunes the light."""
        return rgb


@pytest.mark.parametrize(
    ("command", "shows_advanced"),
    [
        pytest.param("effects", False, id="listing-hides-advanced"),
        pytest.param("effects --category light", False, id="category-listing-hides-advanced"),
        pytest.param("effects --advanced", True, id="advanced-switch-shows-all"),
        pytest.param("effects tuned_probe", True, id="naming-an-effect-shows-all"),
    ],
)
def test_effects_shows_advanced_parameters_only_when_asked_or_when_one_effect_is_named(
    command, shows_advanced, tuned_probe, capsys
):
    assert cli(command) == 0
    out = capsys.readouterr().out
    lines = out[out.index("tuned_probe\n") :].splitlines()[1:]
    entry = "\n".join(itertools.takewhile(lambda line: not line or line.startswith(" "), lines))
    assert "amount : number, fraction, 0 to 1, default 0.5" in entry
    assert ("softness : number, px, 0 to 9, default 2.0" in entry) is shows_advanced
    assert ("edge : text" in entry) is shows_advanced
    assert ("(+2 advanced)" in entry) is not shows_advanced


def test_effects_naming_one_effect_prints_only_that_effect(tuned_probe, capsys):
    assert cli("effects tuned_probe") == 0
    assert _headings(capsys.readouterr().out) == ["tuned_probe"]


def test_effects_naming_an_unknown_effect_is_an_error_listing_the_available_ones(tuned_probe, capsys):
    assert cli("effects no_such_effect") == 1
    captured = capsys.readouterr()
    assert captured.out == ""
    assert "no_such_effect" in captured.err
    assert "tuned_probe" in captured.err


def test_effects_naming_an_effect_and_a_category_exclude_each_other(capsys):
    with pytest.raises(SystemExit) as caught:
        cli("effects tuned_probe --category light")
    assert caught.value.code == 2
    assert "not allowed with" in capsys.readouterr().err


def _headings(out: str) -> list[str]:
    """The effect names `bt-image effects` printed: the unindented lines."""
    return [line for line in out.splitlines() if line and not line.startswith(" ")]


def test_effects_category_lists_only_the_effects_of_that_category(fresh_registry, register_probe, capsys):
    for name, category in [("lamp", "light"), ("tint", "colour"), ("beam", "light")]:
        register_probe(name, category)
    assert cli("effects --category light") == 0
    out = capsys.readouterr().out
    assert _headings(out) == ["beam", "lamp"]  # sorted, and not the colour effect
    assert "    The beam probe." in out  # with the same documentation the plain call prints


@pytest.mark.parametrize(
    "option",
    [
        pytest.param("--category distortion", id="known-category-without-effects"),
        pytest.param("--category nonsense", id="unknown-category"),
    ],
)
def test_effects_category_without_effects_is_an_error_naming_it_and_the_choices(
    option, fresh_registry, register_probe, capsys
):
    register_probe("lamp", "light")
    assert cli(f"effects {option}") == 1
    captured = capsys.readouterr()
    assert captured.out == ""
    assert option.split()[1] in captured.err
    assert "light" in captured.err  # the categories that do have effects


def test_effects_list_categories_gives_each_category_with_effects_its_count_and_description(
    fresh_registry, register_probe, capsys
):
    for name, category in [("drift", "utilities"), ("tint", "colour"), ("hue", "colour"), ("lamp", "light")]:
        register_probe(name, category)
    assert cli("effects --list-categories") == 0
    rows = [line.split(maxsplit=2) for line in capsys.readouterr().out.splitlines()]
    assert [(category, count) for category, count, _ in rows] == [("colour", "2"), ("light", "1"), ("utilities", "1")]
    assert all(description for _, _, description in rows)


def test_effects_help_documents_the_category_options(capsys):
    with pytest.raises(SystemExit):
        cli("effects --help")
    help_text = capsys.readouterr().out
    assert "--category" in help_text
    assert "--list-categories" in help_text


def test_effects_category_and_list_categories_exclude_each_other(capsys):
    with pytest.raises(SystemExit) as caught:
        cli("effects --category light --list-categories")
    assert caught.value.code == 2


def test_render_writes_one_file_per_recipe_in_a_folder_and_prints_each_to_stdout(files, capsys):
    assert cli("render me.png recipes -o out") == 0
    assert sorted(p.name for p in (files / "out").iterdir()) == ["me__a.png", "me__b.png"]
    assert capsys.readouterr().out.splitlines() == [str(Path("out") / f"me__{n}.png") for n in "ab"]


def test_sheet_combines_recipes_into_one_image(files):
    assert cli("sheet me.png recipes -o s.png") == 0
    with Image.open(files / "s.png") as sheet:
        # two 480 px tiles side by side, in one row
        assert sheet.width > 2 * 480
        assert sheet.height < 2 * 480


def test_sweep_puts_the_recipe_at_each_value_of_a_parameter_side_by_side(files, capsys):
    assert cli("sweep me.png recipes/a.toml grain.amount 0..0.2 -o out/s.png --count 5 --columns 2") == 0
    assert capsys.readouterr().out.splitlines() == [str(Path("out") / "s.png")]
    photo = imageio.load(files / "me.png")  # as the command reads it
    values = (0, 0.05, 0.1, 0.15, 0.2)
    expected = contact_sheet(
        [(f"amount={v}", render(photo, Recipe("a", [Step("grain", {"amount": v})], size=64))) for v in values],
        columns=2,
    )
    imageio.save(expected, files / "expected.png")
    np.testing.assert_array_equal(imageio.load(files / "out" / "s.png"), imageio.load(files / "expected.png"))


def test_sweep_takes_a_negative_range_after_dashes_and_a_matte_for_a_recipe_that_needs_the_subject(cut):
    (cut / "masked.toml").write_text(
        'name="masked"\nsize=128\n[[step]]\neffect="glitch"\nmask="subject"\nbands=3\nseed=5\n', encoding="utf-8"
    )
    assert cli("sweep me.png masked.toml glitch.split --count 3 --matte matte.png -o s.png -- -6..6") == 0
    photo = imageio.load(cut / "me.png")
    tiles = []
    for split in (-6, 0, 6):
        look = Recipe("masked", [Step("glitch", {"split": split, "bands": 3, "seed": 5}, mask="subject")], size=128)
        tiles.append((f"split={split}", render(photo, look, imageio.load_matte(cut / "matte.png"))))
    imageio.save(contact_sheet(tiles), cut / "expected.png")
    np.testing.assert_array_equal(imageio.load(cut / "s.png"), imageio.load(cut / "expected.png"))


@pytest.mark.parametrize("span", ["0-20", "a..b", "5", "1..", "..2", "1..2..3", "nan..1"])
def test_sweep_range_must_be_two_numbers_joined_by_two_dots(files, capsys, span):
    with pytest.raises(SystemExit) as caught:
        cli(f"sweep me.png recipes/a.toml grain.amount {span} -o s.png")
    assert caught.value.code == 2
    assert span in capsys.readouterr().err
    assert cli("sweep me.png recipes/a.toml grain.amount 0..0.2 -o s.png") == 0  # the known-good side


def test_bad_recipe_is_reported_before_any_work(files, capsys):
    (files / "recipes" / "c.toml").write_text('[[step]]\neffect="nope"\n', encoding="utf-8")
    assert cli("render me.png recipes -o o") == 1
    assert "c.toml" in capsys.readouterr().err
    assert not (files / "o").exists()


def test_photo_without_a_subject_is_reported_clearly(files, echo_model, monkeypatch, capsys):
    monkeypatch.setenv("BT_IMAGE_SUBJECT_MODEL", str(echo_model(0)))  # answers with the red channel; the photo is black
    imageio.save(np.zeros((96, 128, 3), np.float32), files / "me.png")
    (files / "m.toml").write_text('[[step]]\neffect="fill"\nmask="background"\n', encoding="utf-8")
    assert cli("render me.png m.toml") == 1
    assert "no subject" in capsys.readouterr().err.lower()


# --- a recipe with transparency: PNG and WebP, never JPEG ------------------------------------------------------------


@pytest.fixture
def avatar(files):
    """Adds avatar.toml: a square crop of the photo, as a circle with a clear background."""
    (files / "avatar.toml").write_text(
        'name="avatar"\nsize=64\n[frame]\naspect=1\ntransparent="circle"\n[[step]]\neffect="grain"\n', encoding="utf-8"
    )
    return files


@pytest.mark.parametrize(
    "command", ["render me.png avatar.toml -o out", "render me.png avatar.toml -o out --format webp"]
)
def test_render_writes_a_recipe_with_transparency_as_a_picture_with_alpha(avatar, command):
    assert cli(command) == 0
    (written,) = (avatar / "out").iterdir()
    with Image.open(written) as img:
        assert img.mode == "RGBA"
        assert img.size == (64, 64)
        alpha = np.asarray(img)[..., 3]
    assert (alpha[0, 0], alpha[32, 32]) == (0, 255)


def test_render_to_jpg_is_refused_for_a_recipe_with_transparency_in_plain_words(avatar, capsys):
    assert cli("render me.png avatar.toml -o out --format jpg") == 1
    error = capsys.readouterr().err
    assert "transparency" in error
    assert "Traceback" not in error
    assert cli("render me.png recipes/a.toml -o out2 --format jpg") == 0  # known good: the same format, no transparency


def test_sheet_shows_a_recipe_with_transparency_over_a_checkerboard(avatar):
    assert cli("sheet me.png avatar.toml -o s.png") == 0
    with Image.open(avatar / "s.png") as sheet:
        assert sheet.mode == "RGB"
        corner = np.asarray(sheet)[14:20, 14:20]  # the tile's top-left corner, which the circle leaves clear
    assert len({tuple(pixel) for pixel in corner.reshape(-1, 3)}) == 1
    assert corner[0, 0, 0] == corner[0, 0, 1] == corner[0, 0, 2]  # a grey of the board, not a colour of the photo


def test_steps_saves_the_cut_as_a_png_and_the_steps_before_it_as_jpg(avatar):
    assert cli("steps me.png avatar.toml -o st") == 0
    assert sorted(path.name for path in (avatar / "st").iterdir()) == [
        "00-input.jpg",
        "01-grain.jpg",
        "02-transparent.png",
        "_sheet.jpg",
    ]


# --- the subject: --matte (bring your own) and --segmenter (which model finds it) -----------------------------------


def test_render_with_a_matte_file_keeps_exactly_the_subject_it_marks(cut):
    assert cli("render me.png cut.toml -o out --matte matte.png") == 0
    out = imageio.load(cut / "out" / "me__cut.png")
    assert out[:, :64].max() == 0.0  # background: filled black
    np.testing.assert_allclose(out[:, 64:], imageio.load(cut / "me.png")[:, 64:])  # subject: untouched


def test_cutout_with_a_matte_file_uses_it_as_the_transparency(cut):
    assert cli("cutout me.png -o c.png --matte matte.png") == 0
    with Image.open(cut / "c.png") as img:
        alpha = np.asarray(img)[..., 3]
    assert alpha[:, :64].max() == 0
    assert alpha[:, 64:].min() == 255


@pytest.mark.parametrize("command", ["render me.png cut.toml", "cutout me.png"])
def test_a_matte_file_of_another_size_is_refused(cut, capsys, command):
    Image.new("L", (50, 40), 255).save(cut / "small.png")
    assert cli(f"{command} -o x --matte small.png") == 1
    assert "matte" in capsys.readouterr().err


def test_steps_takes_a_matte_file_too(cut):
    assert cli("steps me.png cut.toml -o st --matte matte.png") == 0
    assert any((cut / "st").iterdir())


def test_the_segmenter_option_wins_over_the_environment(cut, echo_model, monkeypatch):
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "nonsense")  # refused if it were asked
    monkeypatch.setenv(
        "BT_IMAGE_SUBJECT_MODEL", str(echo_model(0))
    )  # the portable backend answers with the red channel
    assert cli("render me.png cut.toml -o out --segmenter onnx") == 0
    photo = imageio.load(cut / "me.png")
    expected = render(photo, load_recipe(cut / "cut.toml"), subject_matte(photo, segmenter="onnx"))
    np.testing.assert_allclose(imageio.load(cut / "out" / "me__cut.png"), expected, atol=0.5 / 255)
    (cut / "bt-image.toml").write_text(PROJECT, encoding="utf-8")
    assert cli("run bt-image.toml --segmenter onnx") == 0  # and in a project run
    assert (cut / "done" / "me" / "cut.jpg").exists()


def test_an_unknown_segmenter_is_a_usage_error(cut, capsys):
    with pytest.raises(SystemExit) as caught:
        cli("render me.png cut.toml --segmenter coreml")
    assert caught.value.code == 2
    assert "coreml" in capsys.readouterr().err


def test_run_finds_the_subjects_again_when_the_segmenter_model_changes(cut, echo_model, monkeypatch):
    (cut / "bt-image.toml").write_text(PROJECT, encoding="utf-8")
    monkeypatch.setenv("BT_IMAGE_SUBJECT_MODEL", str(echo_model(0)))  # the subject is where the photo is red
    assert cli("run bt-image.toml --segmenter onnx -q") == 0
    first = (cut / "done" / "me" / "cut.jpg").read_bytes()
    monkeypatch.setenv("BT_IMAGE_SUBJECT_MODEL", str(echo_model(1)))  # and now where it is green: another cut-out
    assert cli("run bt-image.toml --segmenter onnx -q") == 0
    assert (cut / "done" / "me" / "cut.jpg").read_bytes() != first


# --- user mistakes are reported, not thrown at the user ------------------------------------------------------------

# Commands a user can get wrong, each with a word the one error line must name. Every command differs from a working
# one (`render me.png recipes/a.toml -o out`, tested below) only by its mistake.
MISTAKES = {
    "a file that is not an image": ("render notimage.png recipes/a.toml -o out", "notimage.png"),
    "a folder instead of the photo": ("render empty recipes/a.toml -o out", "empty"),
    "a photo that does not exist": ("render gone.png recipes/a.toml -o out", "gone.png"),
    "an output below a file": ("render me.png recipes/a.toml -o blocker/sub", "blocker"),
    "a recipe with a syntax error": ("render me.png broken.toml -o out", "broken.toml"),
    "an unknown effect": ("render me.png unknown.toml -o out", "nope"),
    "a project file that does not exist": ("run gone.toml", "gone.toml"),
    "a matte that is not an image": ("cutout me.png -o out --matte notimage.png", "notimage"),
    "a folder with no recipe in it": ("render me.png empty -o out", "empty"),
    "a sweep of an effect the recipe does not have": ("sweep me.png recipes/a.toml blur.radius 0..1 -o s.png", "blur"),
    "a sweep of a parameter the effect does not have": (
        "sweep me.png recipes/a.toml grain.amoun 0..1 -o s.png",
        "amoun",
    ),
    "a sweep beyond the parameter's range": ("sweep me.png recipes/a.toml grain.amount 0..5 -o s.png", "amount"),
    "a sweep of one tile": ("sweep me.png recipes/a.toml grain.amount 0..1 -o s.png --count 1", "count"),
    "a picture type nothing can write": ("sheet me.png recipes/a.toml -o sheet.xyz", ".xyz"),
    # (the suite's BT_IMAGE_SUBJECT_MODEL names no file, so the portable model cannot run)
    "a segmenter that cannot run": ("render me.png cut.toml -o out --segmenter onnx", "no-such-model"),
}


@pytest.fixture
def mistakes(files):
    """The files the mistakes above need."""
    (files / "notimage.png").write_text("this is text, not a picture", encoding="utf-8")
    (files / "blocker").write_text("a file where a folder is wanted", encoding="utf-8")
    (files / "broken.toml").write_text("[[step]\neffect=", encoding="utf-8")
    (files / "unknown.toml").write_text('[[step]]\neffect="nope"\n', encoding="utf-8")
    (files / "empty").mkdir()
    (files / "cut.toml").write_text(f'name="cut"\nsize=64\n{CUT}', encoding="utf-8")  # needs the subject
    return files


@pytest.mark.parametrize(("command", "offender"), MISTAKES.values(), ids=MISTAKES.keys())
def test_a_user_mistake_ends_in_one_error_line_on_stderr_and_exit_1(mistakes, capsys, command, offender):
    assert cli(command) == 1
    shown = capsys.readouterr()
    last_line = shown.err.strip().splitlines()[-1]  # progress lines may come first
    assert last_line.startswith("error: ")
    assert offender in last_line
    assert "Traceback" not in shown.err
    assert shown.out == ""  # results go to stdout, errors only to stderr


def test_the_same_commands_with_valid_input_succeed(files):
    """The known-good side of the table above: only the mistake makes the difference."""
    assert cli("render me.png recipes/a.toml -o out") == 0


def test_an_unknown_segmenter_in_the_environment_ends_in_one_error_line(cut, capsys, monkeypatch):
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "coreml")
    (cut / "bt-image.toml").write_text(PROJECT, encoding="utf-8")
    # a render, and a project that needs the subject, say so at once
    for command in ("render me.png cut.toml -o out", "run bt-image.toml"):
        assert cli(command) == 1
        last_line = capsys.readouterr().err.strip().splitlines()[-1]
        assert last_line.startswith("error: ")
        assert "BT_IMAGE_SEGMENTER" in last_line


def test_a_bug_in_an_effect_is_not_dressed_up_as_a_mistake_of_the_user(files, throwaway_effects):
    """The command line catches `BullterrierImageError` and `OSError`, nothing wider: a plain ValueError out of an
    effect is a
    bug, and the developer needs its traceback."""

    @effect(category="utilities", params=ProbeParams)
    def buggy_probe(rgb, params):
        """Fails with a plain ValueError, as a bug would."""
        raise ValueError("a bug, not a user mistake")

    (files / "buggy.toml").write_text('name="buggy"\nsize=32\n[[step]]\neffect="buggy_probe"\n', encoding="utf-8")
    with pytest.raises(ValueError, match="a bug"):
        cli("render me.png buggy.toml -o out")


@pytest.mark.parametrize("columns", ["0", "-2", "many"])
def test_sheet_columns_must_be_a_positive_whole_number(files, capsys, columns):
    with pytest.raises(SystemExit) as caught:
        cli(f"sheet me.png recipes -o s.png --columns {columns}")
    assert caught.value.code == 2
    assert "--columns" in capsys.readouterr().err
    assert cli("sheet me.png recipes -o s.png --columns 2") == 0  # the known-good side


@pytest.mark.parametrize(
    ("mistake", "error"), [("a photo that does not exist", OSError), ("an unknown effect", BullterrierImageError)]
)
def test_debug_lets_the_original_exception_through(mistakes, mistake, error):
    command, offender = MISTAKES[mistake]
    with pytest.raises(error, match=re.escape(offender)):
        cli(f"{command} --debug")


def test_bullterrier_image_debug_in_the_environment_does_the_same(mistakes, monkeypatch):
    command, offender = MISTAKES["an unknown effect"]
    monkeypatch.setenv("BT_IMAGE_DEBUG", "1")
    with pytest.raises(BullterrierImageError, match=offender):
        cli(command)
    monkeypatch.setenv("BT_IMAGE_DEBUG", "0")  # anything but 1 leaves the friendly error on
    assert cli(command) == 1


def test_ctrl_c_stops_with_one_line_and_exit_130(files, capsys, throwaway_effects):
    """Ctrl-C reaches a running command as KeyboardInterrupt (Python's own translation of SIGINT); a step that raises
    it stands in for the person, at a moment the test chooses, so there is no race with a real signal."""

    @effect(category="utilities", params=ProbeParams)
    def ctrl_c_probe(rgb, params):
        """Stands in for the person pressing Ctrl-C while a picture renders."""
        raise KeyboardInterrupt

    (files / "stop.toml").write_text('name="stop"\nsize=32\n[[step]]\neffect="ctrl_c_probe"\n', encoding="utf-8")
    assert cli("render me.png stop.toml -o out -q") == 130
    assert capsys.readouterr().err == "interrupted\n"
    assert not (files / "out" / "me__stop.png").exists()
    with pytest.raises(KeyboardInterrupt):  # --debug lets it through, for developers
        cli("render me.png stop.toml -o out -q --debug")


def test_quiet_hides_the_progress_lines_but_not_the_results(files, capsys):
    assert cli("render me.png recipes -o out") == 0
    loud = capsys.readouterr()
    assert loud.err != ""  # progress is shown by default
    assert cli("render me.png recipes -o out -q") == 0
    quiet = capsys.readouterr()
    assert quiet.err == ""
    assert quiet.out == loud.out


# --- the program itself: --version, python -m bullterrier_image, the exit codes in --help
# --------------------------------------


def test_version_is_the_one_in_pyproject(capsys):
    declared = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))["project"]["version"]
    with pytest.raises(SystemExit) as caught:
        cli("--version")
    assert caught.value.code == 0
    assert capsys.readouterr().out.strip() == f"bt-image {declared}"
    assert bullterrier_image.__version__ == declared


@pytest.mark.parametrize("option", ["--help", "--version"])
def test_help_and_version_load_no_image_library(option):
    """`bt-image --help` and `--version` answer at once: the libraries that render pictures (numpy, scipy, Pillow,
    pydantic, the effects, the subject finder, the pipeline) load only when a command needs them."""
    heavy = [
        *("numpy", "scipy", "PIL", "pydantic"),
        *(
            "bullterrier_image.core",
            "bullterrier_image.effects",
            "bullterrier_image.finding.subject",
            "bullterrier_image.pipeline",
        ),
    ]
    code = (
        "import json, sys\n"
        "from bullterrier_image.cli import main\n"
        f"try:\n    main([{option!r}])\nexcept SystemExit:\n    pass\n"
        f"json.dump([m for m in {heavy!r} if m in sys.modules], sys.stderr)\n"
    )
    done = subprocess.run(
        [sys.executable, "-P", "-c", code], capture_output=True, encoding="utf-8", check=False, cwd=ROOT
    )
    assert done.returncode == 0, done.stderr
    assert done.stdout.startswith(("usage: bt-image", "bt-image "))  # it really answered
    assert done.stderr == "[]"


@pytest.mark.parametrize(
    ("command", "code", "output"),
    [("--version", 0, "bt-image "), ("render gone.png r.toml", 1, "error: ")],
    ids=["answers", "reports an error with no traceback"],
)
def test_python_dash_m_bullterrier_image_runs_the_command_line(tmp_path, command, code, output):
    """`python -m bullterrier_image ARGS` in a fresh interpreter, the way a user starts it."""
    environment = {key: value for key, value in os.environ.items() if key != "BT_IMAGE_DEBUG"}
    done = subprocess.run(
        [sys.executable, "-m", "bullterrier_image", *command.split()],
        cwd=tmp_path, env=environment, capture_output=True, encoding="utf-8", check=False, timeout=60,
    )  # fmt: skip
    assert done.returncode == code, done.stderr
    assert (done.stdout + done.stderr).startswith(output)
    assert "Traceback" not in done.stderr


def test_the_top_level_help_documents_the_exit_codes(capsys):
    with pytest.raises(SystemExit):
        cli("--help")
    text = capsys.readouterr().out
    for code in ("0", "1", "2", "130"):
        assert re.search(rf"^\s+{code}\s+\S", text, re.MULTILINE), f"exit code {code} is not explained:\n{text}"


def commands_in_help(capsys) -> list[str]:
    with pytest.raises(SystemExit):
        cli("--help")
    listed = re.search(r"\{([a-z,-]+)\}", capsys.readouterr().out)
    assert listed, "the help shows no list of commands"
    return listed.group(1).split(",")


def test_every_command_has_a_help_with_an_example(capsys):
    commands = commands_in_help(capsys)
    assert {"render", "run", "effects"} <= set(commands)
    for command in commands:
        with pytest.raises(SystemExit) as caught:
            cli(f"{command} --help")
        assert caught.value.code == 0
        text = capsys.readouterr().out
        assert re.search(rf"^example: bt-image {command}\b", text, re.MULTILINE), f"{command}: no example in\n{text}"


@pytest.mark.parametrize("command", ["gallery", "regenerate", "new-effect"])
def test_developer_commands_are_not_part_of_the_installed_cli(command, capsys):
    """They need a source checkout, so `poe` runs them from `tools/dev.py`."""
    assert command not in commands_in_help(capsys)
    with pytest.raises(SystemExit) as caught:
        cli(command)
    assert caught.value.code == 2
    assert "invalid choice" in capsys.readouterr().err
