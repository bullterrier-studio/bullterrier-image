"""Behaviour of the `pattern` effect: stripes, checks, dots or a grid drawn in two colours in place of the picture."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "pattern"
FOREGROUND, BACKGROUND = (1.0, 0.0, 0.0), (0.0, 0.0, 1.0)


def render(shape: tuple[int, int] = (40, 40), **settings: object) -> np.ndarray:
    """The pattern over a picture of `shape` (h, w), red on blue, hard edged unless `feather` says otherwise."""
    settings = {"foreground": "#ff0000", "background": "#0000ff", "feather": 0, **settings}
    return registry.apply(NAME, np.zeros((*shape, 3), np.float32), **settings)


def foreground(out: np.ndarray) -> np.ndarray:
    """(h, w) bool: the pixels that are the foreground colour (the red channel is 1 only there)."""
    return out[..., 0] > 0.5


COLUMNS = np.arange(40)[None, :]
ROWS = np.arange(40)[:, None]


@pytest.mark.parametrize(
    ("settings", "expected"),
    [
        pytest.param(
            {"kind": "stripes", "thickness": 0.4},
            (COLUMNS % 10 >= 3) & (COLUMNS % 10 <= 6) & (ROWS >= 0),
            id="stripes: a band 4 px wide in the middle of every 10 px cell (pixel centres 3.5 to 6.5)",
        ),
        pytest.param(
            {"kind": "grid", "thickness": 0.2},
            np.isin(COLUMNS % 10, [0, 9]) | np.isin(ROWS % 10, [0, 9]),
            id="grid: lines 2 px wide centred on the cell borders, so the last and the first pixel of a cell",
        ),
        pytest.param(
            {"kind": "checks"},
            ((COLUMNS // 10 + ROWS // 10) % 2 == 0),
            id="checks: 10 px squares alternate, the top-left one in the foreground colour",
        ),
    ],
)
def test_each_kind_draws_the_pattern_worked_out_by_hand_on_a_10_px_cell(settings: dict, expected: np.ndarray) -> None:
    out = render(cell=10, **settings)
    np.testing.assert_array_equal(foreground(out), expected)
    np.testing.assert_array_equal(out[foreground(out)], np.tile(FOREGROUND, (expected.sum(), 1)))
    np.testing.assert_array_equal(out[~foreground(out)], np.tile(BACKGROUND, ((~expected).sum(), 1)))


def test_dots_are_discs_of_the_given_radius_at_the_cell_centres() -> None:
    """Radius 0.3 of a 10 px cell is 3 px round the cell centre (5, 5): a pixel is in when its centre is within 3."""
    out = render(kind="dots", cell=10, radius=0.3)
    expected = np.zeros((40, 40), bool)
    for y in range(40):
        for x in range(40):
            expected[y, x] = np.hypot((x + 0.5) % 10 - 5, (y + 0.5) % 10 - 5) < 3
    np.testing.assert_array_equal(foreground(out), expected)


@pytest.mark.parametrize("kind", ["stripes", "checks", "dots", "grid"])
@pytest.mark.parametrize("cell", [8, 12.5])
def test_the_pattern_repeats_every_cell_in_both_directions(kind: str, cell: float) -> None:
    out = render((100, 100), kind=kind, cell=cell, feather=2, thickness=0.3, radius=0.35)
    period = int(2 * cell)  # two cells is a whole number of px for both values
    np.testing.assert_allclose(out[:, :-period], out[:, period:], atol=1e-4)
    np.testing.assert_allclose(out[:-period], out[period:], atol=1e-4)


def test_the_angle_turns_the_stripes() -> None:
    horizontal = render(kind="stripes", cell=10, angle=90)
    np.testing.assert_array_equal(horizontal, horizontal[:, :1].repeat(40, axis=1))  # constant along each row
    assert (horizontal[:, 0] != horizontal[0, 0]).any()  # but the rows differ
    diagonal = render((60, 60), kind="stripes", cell=10, angle=45, feather=3)
    np.testing.assert_allclose(diagonal[1:, :-1], diagonal[:-1, 1:], atol=1e-3)  # constant along the other diagonal


def test_a_quarter_turn_swaps_the_two_colours_of_the_checks() -> None:
    """Turning clockwise about the top-left corner moves the squares one place sideways: the picture's top-left
    square, once the foreground's, now shows the neighbour that was swung into view, which has the other colour."""
    np.testing.assert_array_equal(
        foreground(render(kind="checks", cell=10, angle=90)), ~foreground(render(kind="checks", cell=10))
    )


def test_feather_blends_the_edge_over_that_many_px_and_zero_gives_hard_edges() -> None:
    """Stripes of 5 px (thickness 0.5) in a 10 px cell have their edges at x = 2.5 and 7.5. Column 2 has its centre
    on the edge (half and half), column 3 is 1 px inside it: coverage 0.5 + 1 / feather, here feather 4 = 0.75."""
    soft = render(kind="stripes", cell=10, thickness=0.5, feather=4)
    np.testing.assert_allclose(soft[0, 2], 0.5 * np.array(FOREGROUND) + 0.5 * np.array(BACKGROUND), atol=1e-5)
    np.testing.assert_allclose(soft[0, 3], 0.75 * np.array(FOREGROUND) + 0.25 * np.array(BACKGROUND), atol=1e-5)
    np.testing.assert_allclose(soft[0, 5], FOREGROUND, atol=1e-6)  # the middle of the stripe is untouched
    hard = render(kind="stripes", cell=10, thickness=0.5, feather=0)
    assert {tuple(colour) for colour in hard.reshape(-1, 3)} == {FOREGROUND, BACKGROUND}


def test_thickness_and_radius_only_matter_to_the_kinds_that_use_them() -> None:
    np.testing.assert_array_equal(render(kind="checks", thickness=0.1, radius=0.1), render(kind="checks"))
    np.testing.assert_array_equal(render(kind="dots", thickness=0.1), render(kind="dots"))
    np.testing.assert_array_equal(render(kind="grid", radius=0.7), render(kind="grid"))


DEFAULTS = {  # settings, row, column -> the coverage of the foreground there, worked out by hand from the documentation
    "grid: a line 0.2 * 32 = 6.4 px wide, so 3.2 px each side of a border, fully covers the pixel centred 2.5 px in": (
        {},
        16,
        2,
        1.0,
    ),
    "grid: the pixel centred 3.5 px in is 0.3 px beyond the line, feathered over 1 px: 0.5 - 0.3": ({}, 16, 3, 0.2),
    "grid: a pixel 1.3 px beyond the line is clear of it": ({}, 16, 4, 0.0),
    "grid: the middle of the cell is far from every line": ({}, 16, 16, 0.0),
    "stripes run down the picture at angle 0: the same in every row": ({"kind": "stripes"}, 5, 18, 1.0),
    "stripes: centred on the middle of the cell, 3.2 px each side, so the pixel at 2.5 px is covered": (
        {"kind": "stripes"},
        0,
        18,
        1.0,
    ),
    "stripes: the pixel at 3.5 px from the middle is 0.3 px beyond the stripe: 0.5 - 0.3": (
        {"kind": "stripes"},
        9,
        19,
        0.2,
    ),
    "stripes: the border of a cell is 16 px from the stripe": ({"kind": "stripes"}, 0, 0, 0.0),
    "dots: the dot is 0.3 * 32 = 9.6 px in radius, so the pixel at (9.5, 0.5) from its centre is feathered": (
        {"kind": "dots"},
        16,
        25,
        0.5 + (9.6 - (9.5**2 + 0.5**2) ** 0.5),
    ),
    "dots: the pixel at (10.5, 0.5) from the centre is clear of the dot": ({"kind": "dots"}, 16, 26, 0.0),
    "checks: the top-left square is the foreground, 16.5 px from the nearest border at its middle": (
        {"kind": "checks"},
        15,
        15,
        1.0,
    ),
}


@pytest.mark.parametrize(("settings", "row", "column", "coverage"), DEFAULTS.values(), ids=DEFAULTS.keys())
def test_with_no_settings_it_draws_white_on_near_black_with_a_cell_of_32_px_and_a_feather_of_1_px(
    settings: dict, row: int, column: int, coverage: float
) -> None:
    """The defaults: a grid of 32 px cells, white on #101018, lines 0.2 of a cell wide, dots 0.3 of a cell in radius, a
    1 px feather, at angle 0. The colour at a pixel is the ground plus the white less the ground times the coverage."""
    out = registry.apply(NAME, np.zeros((40, 40, 3), np.float32), **settings)
    ground = np.array([16, 16, 24]) / 255
    np.testing.assert_allclose(out[row, column], ground + (1 - ground) * coverage, atol=1e-6)
