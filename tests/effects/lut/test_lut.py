"""Behaviour of the `lut` effect: a `.cube` file read at the boundary, then looked up with trilinear interpolation.

The expectations are worked out by hand: a table sampled from a function whose lookup is known, `.cube` files written
here row by row in the format's own order (red changes fastest), and a table of three entries per axis whose midpoint
is a mean of two entries.
"""

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


def write_cube(path: Path, mapping: Mapping, size: int = 2, header: str = "") -> Path:
    """A `.cube` file whose entry at grid point (r, g, b) holds `mapping(r, g, b)`, rows with red changing fastest."""
    path.parent.mkdir(exist_ok=True)
    rows = [
        " ".join(f"{v:.6f}" for v in mapping(r / (size - 1), g / (size - 1), b / (size - 1)))
        for b, g, r in product(range(size), repeat=3)
    ]
    text = f'TITLE "a table"\n{header}LUT_3D_SIZE {size}\n# the entries\n' + "\n".join(rows) + "\n"
    path.write_text(text, encoding="utf-8")
    return path


def identity(r: float, g: float, b: float) -> tuple[float, float, float]:
    return r, g, b


def negative(r: float, g: float, b: float) -> tuple[float, float, float]:
    return 1 - r, 1 - g, 1 - b


@pytest.mark.parametrize("size", [2, 5, 33], ids=lambda size: f"{size} entries per axis")
def test_an_identity_table_leaves_the_picture_as_it_was(photo, tmp_path, size):
    cube = write_cube(tmp_path / "identity.cube", identity, size)
    np.testing.assert_allclose(registry.apply(NAME, photo, file=load_lut(cube)), photo, atol=1e-5)


def test_a_negative_table_gives_the_negative(photo, tmp_path):
    cube = write_cube(tmp_path / "negative.cube", negative)
    np.testing.assert_allclose(registry.apply(NAME, photo, file=load_lut(cube)), 1 - photo, atol=1e-5)


@pytest.mark.parametrize("amount", [0.0, 0.25, 0.5, 1.0], ids=lambda amount: f"amount {amount}")
def test_amount_is_the_share_of_the_way_from_the_picture_to_the_grade(photo, tmp_path, amount):
    cube = write_cube(tmp_path / "negative.cube", negative)
    out = registry.apply(NAME, photo, file=load_lut(cube), amount=amount)
    np.testing.assert_allclose(out, (1 - amount) * photo + amount * (1 - photo), atol=1e-5)


def test_a_colour_between_the_entries_is_interpolated_trilinearly(tmp_path):
    # A table sampled from a function that is linear in each channel on its own is reproduced exactly between entries.
    def multilinear(r: float, g: float, b: float) -> tuple[float, float, float]:
        return r * g, b * (1 - r), (r + g + b) / 3

    cube = write_cube(tmp_path / "multilinear.cube", multilinear)
    colour = np.full((1, 1, 3), (0.25, 0.5, 0.75), np.float32)
    # by hand: red 0.25 * 0.5 = 0.125; green 0.75 * (1 - 0.25) = 0.5625; blue (0.25 + 0.5 + 0.75) / 3 = 0.5
    np.testing.assert_allclose(registry.apply(NAME, colour, file=load_lut(cube)), [[[0.125, 0.5625, 0.5]]], atol=1e-6)


def test_between_two_entries_of_a_curve_the_result_is_their_mean(tmp_path):
    # Three entries per axis sample the square at 0, 0.5, 1 (0, 0.25, 1); a value halfway between the first two lies
    # at the mean of 0 and 0.25.
    cube = write_cube(tmp_path / "square.cube", lambda r, g, b: (r**2, g**2, b**2), size=3)
    colour = np.full((1, 1, 3), 0.25, np.float32)
    np.testing.assert_allclose(registry.apply(NAME, colour, file=load_lut(cube)), 0.125, atol=1e-6)


def test_the_file_lists_blue_slowest_and_red_fastest(photo, tmp_path):
    cube = write_cube(tmp_path / "swap.cube", lambda r, g, b: (b, g, r))
    np.testing.assert_allclose(registry.apply(NAME, photo, file=load_lut(cube)), photo[..., ::-1], atol=1e-5)


def test_colours_beyond_the_domain_take_the_edge_of_the_table(tmp_path):
    # Over the domain 0.25..0.75 the red entries run 0.5..1 and the green ones 0..0.5: 0 lies below the red edge and
    # 1 above the green edge, so each gives its table's edge value; blue, an identity, has a quarter of the
    # way along the domain at 0.375.
    cube = write_cube(
        tmp_path / "narrow.cube",
        lambda r, g, b: (0.5 + r / 2, g / 2, b),
        header="DOMAIN_MIN 0.25 0.25 0.25\nDOMAIN_MAX 0.75 0.75 0.75\n",
    )
    colour = np.array([[[0.0, 1.0, 0.375]]], np.float32)
    np.testing.assert_allclose(registry.apply(NAME, colour, file=load_lut(cube)), [[[0.5, 0.5, 0.25]]], atol=1e-6)


def test_a_table_that_leaves_the_range_gives_a_picture_inside_it(photo):
    table = np.full((2, 2, 2, 3), (1.4, -0.2, 0.5), np.float32)
    out = registry.apply(NAME, photo, file=Lut(table))
    np.testing.assert_allclose(out, np.broadcast_to([1.0, 0.0, 0.5], photo.shape), atol=1e-6)


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
    out = recipe.render(photo, recipe.Recipe("look", [fx.lut(file="negative.cube", amount=0.5)], size=128))
    np.testing.assert_allclose(out, 0.5 * photo + 0.5 * (1 - photo), atol=1e-5)


ROWS = "0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n"
BAD_FILES = {
    "1D table": ("LUT_1D_SIZE 4\n0 0 0\n", r"line 1.*1D"),
    "no size": ('TITLE "x"\n' + ROWS, r"line 2.*before LUT_3D_SIZE"),
    "size 1": ("LUT_3D_SIZE 1\n0 0 0\n", r"line 1.*LUT_3D_SIZE"),
    "size not whole": ("LUT_3D_SIZE 2.5\n", r"line 1.*LUT_3D_SIZE"),
    "too few rows": ("LUT_3D_SIZE 2\n" + ROWS[:-6], r"needs 8 rows.*has 7"),
    "too many rows": ("LUT_3D_SIZE 2\n" + ROWS + "0 0 0\n", r"needs 8 rows.*has 9"),
    "a row of two numbers": ("LUT_3D_SIZE 2\n0 0\n", r"line 2.*needs 3 numbers"),
    "a word in a row": ("LUT_3D_SIZE 2\n0 0 x\n", r"line 2.*cannot read"),
    "unknown keyword": ("LUT_3D_SIZE 2\nGAMMA 2.2\n", r"line 2.*unknown keyword 'GAMMA'"),
    "domain upside down": ("DOMAIN_MIN 1 1 1\nDOMAIN_MAX 0 0 0\nLUT_3D_SIZE 2\n" + ROWS, r"domain"),
    "infinite value": ("LUT_3D_SIZE 2\n" + ROWS.replace("0 0 0", "1e999 0 0", 1), r"finite"),
    "empty file": ("", r"no LUT_3D_SIZE"),
}


@pytest.mark.parametrize(("text", "message"), BAD_FILES.values(), ids=BAD_FILES.keys())
def test_a_cube_that_cannot_be_used_fails_naming_the_file_and_the_line(tmp_path, text, message):
    (tmp_path / "bad.cube").write_text(text, encoding="utf-8")
    with pytest.raises(LutError, match=r"bad\.cube.*" + message):
        load_lut(tmp_path / "bad.cube")


@pytest.mark.parametrize("name", ["binary.cube", "missing.cube"])
def test_a_cube_that_is_not_text_or_not_there_fails_naming_the_file(tmp_path, name):
    (tmp_path / "binary.cube").write_bytes(b"\xff\xfe\x00\x01")
    with pytest.raises(LutError, match=name):
        load_lut(tmp_path / name)


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
