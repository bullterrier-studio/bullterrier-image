"""Behaviour of the `outline` effect."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "outline"
GREY = 0.5
WHITE = "#ffffff"


def grey(size: int = 41) -> np.ndarray:
    return np.full((size, size, 3), GREY, np.float32)


def lone_pixel(size: int = 41) -> np.ndarray:
    """A subject of one pixel in the middle."""
    subject = np.zeros((size, size), np.float32)
    subject[size // 2, size // 2] = 1.0
    return subject


def block() -> np.ndarray:
    """A subject of a 20 x 20 square, rows and columns 10..29 of a 40 x 40 picture."""
    subject = np.zeros((40, 40), np.float32)
    subject[10:30, 10:30] = 1.0
    return subject


@pytest.mark.parametrize(
    "settings",
    [
        {"width": 0, "glow": 0},
        {"width": 0, "glow": 0, "position": "inside"},
        {"width": 0, "glow": 0, "position": "center"},
    ],
    ids=["no width, no glow", "no width, inside", "no width, centre"],
)
def test_nothing_is_drawn(settings, photo, subject):
    np.testing.assert_array_equal(registry.apply(NAME, photo, subject=subject, **settings), photo)


def test_without_a_subject_there_is_nothing_to_outline(photo):
    np.testing.assert_array_equal(registry.apply(NAME, photo, width=6, glow=8), photo)


def test_a_lone_pixel_grows_to_a_disc_of_radius_width():
    """Hand count: within 2 px of a pixel are 4 neighbours at 1, 4 diagonals at 1.41 and 4 at 2, i.e. 12 pixels. Two
    px beyond the stroke (width + 1 away) nothing is touched; the pixel itself is the subject and stays."""
    size, centre = 41, 20
    out = registry.apply(NAME, grey(size), subject=lone_pixel(size), width=2, color=WHITE)
    y, x = np.mgrid[0:size, 0:size]
    reach = np.hypot(y - centre, x - centre)
    painted = (out != GREY).any(axis=-1)
    assert ((reach <= 2) & (reach > 0)).sum() == 12
    np.testing.assert_array_equal(out[(reach <= 2) & (reach > 0)], 1.0)
    np.testing.assert_array_equal(out[reach >= 3], GREY)
    assert not painted[centre, centre]


@pytest.mark.parametrize("position", ["outside", "inside", "center"])
def test_the_stroke_is_drawn_to_the_matte_edge_side_it_is_asked_for(position):
    """Along the middle column of a square covering rows 10..29: width 4 outside paints rows 6..9, inside rows 10..13,
    centre (2 px each side) rows 8..11; its first unpainted neighbours stay as they were."""
    painted_rows = {"outside": range(6, 10), "inside": range(10, 14), "center": range(8, 12)}[position]
    out = registry.apply(NAME, grey(40), subject=block(), width=4, color=WHITE, position=position)
    top_half = out[:20, 20, 0]  # the bottom edge is the mirror image
    expected = np.where(np.isin(np.arange(20), list(painted_rows)), 1.0, GREY)
    np.testing.assert_array_equal(top_half, expected)


def test_outside_leaves_the_subject_untouched(photo, subject):
    out = registry.apply(NAME, photo, subject=subject, width=6, glow=10, color="#ff0000")
    inside = subject >= 0.5
    np.testing.assert_array_equal(out[inside], photo[inside])
    assert (out != photo).any()


def test_a_stroke_behind_a_soft_edge_shows_through_only_where_the_subject_is_absent():
    """A pixel the subject covers 25 % (below the 0.5 threshold, so outside) is painted over 75 %."""
    subject = block()
    subject[20, 9] = 0.25
    out = registry.apply(NAME, grey(40), subject=subject, width=3, color=WHITE)
    assert out[20, 9, 0] == pytest.approx(GREY + 0.75 * (1.0 - GREY))
    assert out[20, 8, 0] == 1.0


@pytest.mark.parametrize(
    ("threshold", "painted"),
    [(0.3, False), (0.5, True)],
    ids=["0.4 counts as subject at 0.3", "0.4 counts as background at 0.5"],
)
def test_subject_threshold_is_the_subject_value_from_which_a_pixel_belongs_to_the_subject(threshold, painted):
    subject = block()
    subject[20, 9] = 0.4
    out = registry.apply(NAME, grey(40), subject=subject, width=3, color=WHITE, subject_threshold=threshold)
    assert (out[20, 9, 0] != GREY) == painted
    assert (out[20, 6, 0] == 1.0) != painted  # a 3 px stroke reaches column 6 only from a subject that starts at 9


def test_the_stroke_is_solid_colour():
    out = registry.apply(NAME, grey(40), subject=block(), width=3, color=[1.0, 0.0, 0.2])
    np.testing.assert_allclose(out[9, 20], [1.0, 0.0, 0.2], atol=1e-6)  # the row just above the square


def test_the_glow_fades_quadratically_from_the_stroke_to_nothing_at_glow_px():
    """Width 2, glow 4: a pixel `d` beyond the stroke's outer edge (2 px out) has (1 - d / 4)^2 of the colour."""
    out = registry.apply(NAME, grey(40), subject=block(), width=2, glow=4, color=WHITE)
    column = out[:, 20, 0]
    for row, share in {9: 1.0, 8: 1.0, 7: 0.5625, 6: 0.25, 5: 0.0625, 4: 0.0}.items():
        assert column[row] == pytest.approx(GREY + share * (1.0 - GREY)), row
    assert column[3] == GREY
    # without glow, the pixel just past the stroke is untouched
    no_glow = registry.apply(NAME, grey(40), subject=block(), width=2, glow=0, color=WHITE)[:, 20, 0]
    assert no_glow[7] == GREY


@pytest.mark.parametrize("glow", [0.0, 0.5], ids=["no glow", "a glow shorter than the antialiasing"])
def test_a_fractional_width_blends_its_last_pixel_by_the_part_of_it_the_stroke_covers(glow):
    """Width 2.5 along the middle column: 2 px are full, the third is covered by half of it."""
    column = registry.apply(NAME, grey(40), subject=block(), width=2.5, glow=glow, color=WHITE)[:, 20, 0]
    assert column[7] == pytest.approx(GREY + 0.5 * (1.0 - GREY))  # 3 px above the square
    assert column[6] == GREY
