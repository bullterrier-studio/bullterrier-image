"""`_core.Lut`: the lookup table the Rust core holds, made from an array, the identity or a `.cube` file.

The expectations are worked out by hand; looking a colour up in a table is the core's, tested in Rust
(`crates/core/tests/it/lut.rs`). Here: what crosses the boundary (the array's index order, the domain, the errors).
"""

import numpy as np
import pytest

from bullterrier_image._native import _core  # noqa: PLC2701  architecture: the boundary the facade calls


def ramp(size: int) -> np.ndarray:
    """A table whose entry at [red, green, blue] is (red, green, blue) as whole numbers: every index is told apart."""
    index = np.arange(size, dtype=np.float32)
    red, green, blue = np.meshgrid(index, index, index, indexing="ij")
    return np.stack([red, green, blue], axis=-1)


def test_the_array_is_indexed_red_green_blue_and_comes_back_as_it_went_in():
    table = ramp(3)
    made = _core.Lut(table)
    assert made.size == 3
    np.testing.assert_array_equal(made.table(), table)  # the first axis is red, the last channel is the output


def test_the_identity_is_the_table_of_each_input_colour_at_its_grid_point():
    made = _core.Lut.identity(5)
    np.testing.assert_allclose(made.table(), ramp(5) / 4, atol=1e-7)
    assert (made.domain_min, made.domain_max) == ((0.0, 0.0, 0.0), (1.0, 1.0, 1.0))


def test_the_domain_is_kept_for_each_channel():
    made = _core.Lut(ramp(2), (-1.0, 0.0, 0.5), (1.0, 2.0, 1.5))
    assert (made.domain_min, made.domain_max) == ((-1.0, 0.0, 0.5), (1.0, 2.0, 1.5))


def test_a_cube_file_is_read_with_red_changing_fastest(tmp_path):
    rows = "\n".join(f"{red} {green} {blue}" for blue in range(2) for green in range(2) for red in range(2))
    (tmp_path / "ramp.cube").write_text(f"TITLE x\nLUT_3D_SIZE 2\n{rows}\n", encoding="utf-8")
    np.testing.assert_array_equal(_core.Lut.from_cube(tmp_path / "ramp.cube").table(), ramp(2))


@pytest.mark.parametrize(
    ("make", "said"),
    [
        pytest.param(lambda: _core.Lut(np.zeros((2, 2, 3), np.float32)), "shape", id="three dimensions"),
        pytest.param(lambda: _core.Lut(np.zeros((2, 3, 2, 3), np.float32)), "shape", id="not a cube"),
        pytest.param(lambda: _core.Lut(np.zeros((1, 1, 1, 3), np.float32)), "entries per axis", id="one entry"),
        pytest.param(lambda: _core.Lut(np.full((2, 2, 2, 3), np.nan, np.float32)), "finite", id="NaN entries"),
        pytest.param(lambda: _core.Lut(ramp(2), (1, 1, 1), (0, 0, 0)), "below", id="a domain upside down"),
        pytest.param(lambda: _core.Lut.identity(257), "257", id="an identity too large"),
        pytest.param(lambda: _core.Lut.from_cube("missing.cube"), "missing.cube", id="a missing file"),
    ],
)
def test_a_table_that_is_none_is_a_core_error_of_kind_lut_naming_the_problem(make, said):
    with pytest.raises(_core.CoreError, match=said) as caught:
        make()
    assert caught.value.kind == "lut"
