"""Reading a recipe file: anything wrong with the file itself (its TOML, its encoding, its format version, its top-level
keys) is a `RecipeError` that starts with the file's name."""

import re

import pytest

from photo_fx.core.errors import RecipeError
from photo_fx.core.loader import load_recipe

STEP = "[[step]]\neffect = 'grade'\n"


def _write(tmp_path, text, name="look.toml"):
    path = tmp_path / name
    path.write_text(text, encoding="utf-8")
    return path


def test_a_toml_syntax_error_names_the_file_and_keeps_the_line_and_column(tmp_path):
    path = _write(tmp_path, "name = 'ok'\nsize = 64\ncolor = '#fff\n" + STEP, "broken.toml")
    with pytest.raises(RecipeError, match=r"^broken\.toml: ") as raised:
        load_recipe(path)
    assert "line 3" in str(raised.value)
    assert "column" in str(raised.value)


def test_a_file_that_is_not_utf8_names_the_file(tmp_path):
    path = tmp_path / "latin.toml"
    path.write_bytes("name = 'café'\n".encode("latin-1") + STEP.encode())
    with pytest.raises(RecipeError, match=r"^latin\.toml: .*UTF-8"):
        load_recipe(path)


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


@pytest.mark.parametrize("key", ["sise", "formt", "steps", "animations"])
def test_a_misspelt_top_level_key_is_refused_naming_it(tmp_path, key):
    path = _write(tmp_path, f"{key} = 2\n" + STEP, "typo.toml")
    with pytest.raises(RecipeError, match=rf"^typo\.toml: .*`{key}`"):
        load_recipe(path)


def test_a_single_step_table_instead_of_a_list_of_steps_is_refused_naming_the_file(tmp_path):
    path = _write(tmp_path, "[step]\neffect = 'grade'\n", "single.toml")
    with pytest.raises(RecipeError, match=r"^single\.toml: .*\[\[step\]\]"):
        load_recipe(path)


def test_an_animation_error_is_a_recipe_error_naming_the_file(tmp_path):
    path = _write(tmp_path, "[animation]\nseconds = -1\n" + STEP, "clip.toml")
    with pytest.raises(RecipeError, match=r"^clip\.toml: .*seconds"):
        load_recipe(path)
    path = _write(tmp_path, STEP + "exposure = { from = 0, to = 1, ease = 'wobbly' }\n", "ease.toml")
    with pytest.raises(RecipeError, match=r"^ease\.toml: step 1: .*ease"):
        load_recipe(path)
