"""The `lut` effect through Python: a `.cube` file or a `Lut` handed to the registry, `fx` and a recipe file, and the
`LutError` raised for a bad file or table. Reading a `.cube` and the lookup itself are tested in Rust."""

from collections.abc import Callable
from itertools import product
from pathlib import Path

import numpy as np
import pytest

from bullterrier_image import Lut, LutError, Picture, fx, recipe
from bullterrier_image.core import registry
from bullterrier_image.imageio import load_lut

NAME = "lut"
Mapping = Callable[[float, float, float], tuple[float, float, float]]


def write_cube(path: Path, mapping: Mapping, size: int = 2) -> Path:
    """A `.cube` file whose entry at grid point (r, g, b) holds `mapping(r, g, b)`, rows with red changing fastest."""
    path.parent.mkdir(exist_ok=True)
    rows = [
        " ".join(f"{v:.6f}" for v in mapping(r / (size - 1), g / (size - 1), b / (size - 1)))
        for b, g, r in product(range(size), repeat=3)
    ]
    text = f'TITLE "a table"\nLUT_3D_SIZE {size}\n# the entries\n' + "\n".join(rows) + "\n"
    path.write_text(text, encoding="utf-8")
    return path


def negative(r: float, g: float, b: float) -> tuple[float, float, float]:
    return 1 - r, 1 - g, 1 - b


def test_a_negative_table_gives_the_negative(photo, tmp_path):
    cube = write_cube(tmp_path / "negative.cube", negative)
    np.testing.assert_allclose(registry.apply(NAME, photo, file=load_lut(cube)), 1 - photo, atol=1e-5)


# --- the file is read at the boundary, relative to the recipe ------------------------------------------------------


def write_recipe(folder: Path, cube: str) -> Path:
    path = folder / "look.toml"
    path.write_text(f'name = "look"\nsize = 128\n\n[[step]]\neffect = "lut"\nfile = "{cube}"\n', encoding="utf-8")
    return path


def test_a_recipe_reads_the_cube_relative_to_its_own_folder_whatever_the_working_folder(photo, tmp_path, monkeypatch):
    write_cube(tmp_path / "grades" / "negative.cube", negative)
    look = recipe.load_recipe(write_recipe(tmp_path, "grades/negative.cube"))
    monkeypatch.chdir(tmp_path.parent)
    np.testing.assert_allclose(recipe.render(photo, look), 1 - photo, atol=1e-5)


def test_fx_takes_the_path_of_a_cube_file(photo, tmp_path, monkeypatch):
    write_cube(tmp_path / "negative.cube", negative)
    monkeypatch.chdir(tmp_path)
    out = recipe.render(photo, recipe.Recipe([fx.lut(file="negative.cube", amount=0.5)], name="look", size=128))
    np.testing.assert_allclose(out, 0.5 * photo + 0.5 * (1 - photo), atol=1e-5)


def test_a_cube_that_cannot_be_used_raises_lut_error_naming_the_file_and_the_line(tmp_path):
    (tmp_path / "bad.cube").write_text("LUT_1D_SIZE 4\n0 0 0\n", encoding="utf-8")
    with pytest.raises(LutError, match=r"bad\.cube.*line 1.*1D"):
        load_lut(tmp_path / "bad.cube")


def test_a_recipe_with_a_bad_cube_fails_at_load_naming_step_parameter_and_file(tmp_path):
    (tmp_path / "bad.cube").write_text("LUT_1D_SIZE 4\n", encoding="utf-8")
    with pytest.raises(LutError, match=r"look\.toml: step 1: .*`file`.*bad\.cube"):
        recipe.load_recipe(write_recipe(tmp_path, "bad.cube"))


@pytest.mark.parametrize(
    "table",
    [np.zeros((2, 2, 3)), np.zeros((2, 2, 3, 3)), np.zeros((1, 1, 1, 3)), np.zeros((2, 2, 2, 3), np.int32)],
    ids=["three dimensions", "not a cube", "one entry", "not floats"],
)
def test_a_table_array_of_the_wrong_shape_is_refused(table):
    with pytest.raises(LutError, match="shape"):
        Lut(table)


def test_a_picture_is_not_a_table():
    with pytest.raises(ValueError, match="file"):
        registry.apply(NAME, np.zeros((2, 2, 3), np.float32), file=Picture.of_color("#000000"))
