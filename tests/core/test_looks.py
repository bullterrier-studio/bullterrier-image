"""Looks are recipe files with a `description`, listed by name. Read from a throwaway folder here; the shipped looks
are loaded and rendered in `tests/test_recipe.py`."""

import pytest

from bullterrier_image.core.looks import read
from bullterrier_image.errors import RecipeError


def test_looks_are_listed_by_file_name_sorted_with_their_description_and_file(tmp_path):
    (tmp_path / "zoom.toml").write_text('description = "Fast."\n', encoding="utf-8")
    (tmp_path / "calm.toml").write_text('description = "Slow."\nname = "other"\n', encoding="utf-8")
    (tmp_path / "notes.txt").write_text("not a look", encoding="utf-8")
    found = read(tmp_path)
    assert list(found) == ["calm", "zoom"]
    assert found["calm"].description == "Slow."
    assert found["zoom"].file == tmp_path / "zoom.toml"


def test_a_folder_without_looks_has_none(tmp_path):
    assert read(tmp_path) == {}


@pytest.mark.parametrize(
    "text",
    [
        pytest.param("[[step]]\neffect = 'grade'\n", id="no-description"),
        pytest.param('description = "  "\n', id="an-empty-description"),
        pytest.param("description = 3\n", id="a-description-that-is-no-text"),
    ],
)
def test_a_look_without_a_description_is_refused_naming_its_file(tmp_path, text):
    (tmp_path / "bare.toml").write_text(text, encoding="utf-8")
    with pytest.raises(RecipeError, match=r"bare\.toml.*description"):
        read(tmp_path)
