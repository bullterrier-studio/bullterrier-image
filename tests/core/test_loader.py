"""Reading a recipe file: anything wrong with the file itself (its TOML, its encoding, its format version, its top-level
keys) is a `RecipeError` that starts with the file's name."""

import re

import pytest

from bullterrier_image.core.loader import load_recipe
from bullterrier_image.core.steps import Step
from bullterrier_image.errors import RecipeError

STEP = "[[step]]\neffect = 'grade'\n"


def _write(tmp_path, text, name="look.toml"):
    path = tmp_path / name
    path.write_text(text, encoding="utf-8")
    return path


def test_a_file_that_is_not_utf8_names_the_file(tmp_path):
    path = tmp_path / "latin.toml"
    path.write_bytes("name = 'café'\n".encode("latin-1") + STEP.encode())
    with pytest.raises(RecipeError, match=r"^latin\.toml: .*UTF-8"):
        load_recipe(path)


# Without `name`, a recipe is named after its file: every run of characters a name may not hold becomes one `-`, and the
# ends are trimmed so that the name starts with a letter or digit (expected values worked out by hand).
@pytest.mark.parametrize(
    ("file", "name"),
    [
        ("my look.toml", "my-look"),
        ("  --Holiday  (2024)!!.toml", "Holiday-2024"),
        ("v1.2 final.toml", "v1.2-final"),
        ("rock - roll.toml", "rock-roll"),  # the `-` already there joins the two replaced spaces: one `-`
        ("café au lait.toml", "caf-au-lait"),
        ("_draft_.toml", "draft_"),
        ("plain.toml", "plain"),
        ("a" * 63 + " b.toml", "a" * 63),  # cut to 64 characters, then the dangling `-` trimmed
        ("b" * 80 + ".toml", "b" * 64),
    ],
)
def test_a_recipe_without_a_name_is_named_after_its_file_made_safe(tmp_path, file, name):
    assert load_recipe(_write(tmp_path, STEP, file)).name == name


@pytest.mark.parametrize("file", ["日本.toml", "---.toml", "con.toml", "LPT1 .toml"])
def test_a_file_name_that_leaves_no_usable_name_asks_for_a_name_or_a_new_file_name(tmp_path, file):
    with pytest.raises(RecipeError, match=rf"^{re.escape(file)}: .*name = .*rename"):
        load_recipe(_write(tmp_path, STEP, file))


def test_a_recipe_without_a_format_or_with_format_1_loads(tmp_path):
    assert load_recipe(_write(tmp_path, "name = 'a'\n" + STEP, "a.toml")).steps[0].effect == "grade"
    assert load_recipe(_write(tmp_path, "format = 1\nname = 'b'\n" + STEP, "b.toml")).name == "b"


@pytest.mark.parametrize(
    ("written", "value"), [("2", "2"), ("0", "0"), ("'1'", "'1'"), ("1.0", "1.0"), ("true", "True")]
)
def test_any_other_format_is_refused_naming_the_value_and_the_formats_this_release_reads(tmp_path, written, value):
    path = _write(tmp_path, f"format = {written}\n" + STEP, "future.toml")
    with pytest.raises(RecipeError, match=r"^future\.toml: .*`format`") as raised:
        load_recipe(path)
    message = str(raised.value)
    assert value in message
    assert re.search(r"reads\D*\b1\b", message)  # the format this release reads


# A broken file: each is refused with a RecipeError that starts with the file's name and says what is wrong.
BROKEN = {
    "TOML syntax error, with its line and column": ("name = 'ok'\nsize = 64\ncolor = '#fff\n" + STEP, "line 3, column"),
    "misspelt size": ("sise = 2\n" + STEP, "`sise`"),
    "misspelt format": ("formt = 2\n" + STEP, "`formt`"),
    "steps for step": ("steps = 2\n" + STEP, "`steps`"),
    "an [animation] table, which no longer exists": ("[animation]\nseconds = 2\n" + STEP, "`animation`"),
    "a single [step] table instead of a list": ("[step]\neffect = 'grade'\n", r"\[\[step\]\]"),
    "an animated value, which no longer exists": (STEP + "exposure = { from = 0, to = 1 }\n", "step 1: .*exposure"),
}


@pytest.mark.parametrize(("text", "complaint"), BROKEN.values(), ids=BROKEN.keys())
def test_a_broken_file_is_a_recipe_error_naming_the_file_and_the_problem(tmp_path, text, complaint):
    with pytest.raises(RecipeError, match=rf"^broken\.toml: .*{complaint}"):
        load_recipe(_write(tmp_path, text, "broken.toml"))


# --- presets in a recipe file ------------------------------------------------------------------------------------


def test_a_step_may_name_a_preset_and_its_own_keys_win_over_it(tmp_path):
    path = _write(tmp_path, "[[step]]\neffect = 'gradient_map'\npreset = 'sepia'\nshadow_clip = 0.2\nopacity = 0.5\n")
    (step,) = load_recipe(path).steps
    assert step == Step("gradient_map", {"shadow_clip": 0.2}, preset="sepia").blend(0.5)
    assert "preset" not in step.params  # a step's key, not an effect's parameter


@pytest.mark.parametrize(
    ("effect", "preset", "complaint"),
    [
        pytest.param("gradient_map", "sepya", r"gradient_map.*'sepya'.*sepia", id="a preset the effect lacks"),
        pytest.param("grade", "sepia", r"grade.*'sepia'.*no presets", id="an effect without presets"),
    ],
)
def test_a_wrong_preset_in_a_file_names_the_file_the_step_and_the_preset(tmp_path, effect, preset, complaint):
    path = _write(tmp_path, f"[[step]]\neffect = 'grade'\n\n[[step]]\neffect = '{effect}'\npreset = '{preset}'\n")
    with pytest.raises(RecipeError, match=rf"^look\.toml: step 2: .*{complaint}"):
        load_recipe(path)
