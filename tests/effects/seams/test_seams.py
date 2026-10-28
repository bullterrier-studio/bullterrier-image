"""Behaviour of the `seams` effect: a solid line or band along the borders between the pieces of a partition, and
nowhere else. The oracle for where it draws is worked out by hand from the borders of a grid, which are the lines
between pixels, in pixel units."""

import numpy as np
import pytest

from bullterrier_image.core import registry
from bullterrier_image.core.runner import render
from bullterrier_image.core.steps import Each, Recipe, Step
from bullterrier_image.recipe import Grid, Pick

NAME = "seams"
RED = (1.0, 0.0, 0.0)
WHITE = (1.0, 1.0, 1.0)


def columns(widths: list[int], rows: int = 1, height: int = 96) -> np.ndarray:
    """The pieces of a grid of columns `widths` px wide and `rows` equal rows, numbered row after row."""
    across = np.repeat(np.arange(len(widths)), widths)
    down = np.arange(height) * rows // height
    return (down[:, None] * len(widths) + across[None, :]).astype(np.int32)


def covered(widths: list[int], rows: int, line: float, height: int = 96) -> np.ndarray:
    """How much of each pixel a line `line` px wide, centred on every border of the grid, covers: a pixel is covered
    for the length of its side that falls inside the band of half-width line/2 round the nearest border."""
    across = np.cumsum(widths)[:-1]
    down = np.arange(1, rows) * height / rows
    x, y = np.arange(sum(widths)), np.arange(height)

    def share(position: np.ndarray, borders: np.ndarray) -> np.ndarray:
        """Per pixel along one axis: the share of its 1 px that lies within line/2 of any border."""
        low = position[:, None]  # the pixel spans low .. low + 1
        near = np.clip(np.minimum(low + 1, borders + line / 2) - np.maximum(low, borders - line / 2), 0, 1)
        return near.max(axis=1, initial=0.0)

    return np.maximum(share(x, across)[None, :], share(y, down)[:, None]) * min(line, 1.0)


@pytest.mark.parametrize(
    ("widths", "rows", "line"),
    [
        pytest.param([64, 64], 1, 2.0, id="a vertical border, 2 px wide: the two pixels beside it"),
        pytest.param([64, 64], 1, 4.0, id="4 px wide: two pixels each side"),
        pytest.param([40, 50, 38], 1, 6.0, id="two borders"),
        pytest.param([64, 64], 3, 2.0, id="crossing horizontal and vertical borders"),
        pytest.param([128], 4, 8.0, id="horizontal borders only"),
        pytest.param([64, 64], 1, 1.0, id="1 px wide: half of each of the two pixels beside the border"),
        pytest.param([64, 64], 1, 0.5, id="a hairline, fainter than a pixel"),
    ],
)
def test_the_line_is_drawn_as_wide_as_asked_along_every_border_and_nowhere_else(photo, widths, rows, line):
    out = registry.apply(NAME, photo, width=line, color=RED, partition=columns(widths, rows))
    cover = covered(widths, rows, line)[..., None]
    expected = photo * (1 - cover) + np.array(RED, np.float32) * cover
    np.testing.assert_allclose(out, expected, atol=1e-5)
    assert (cover > 0).any()
    np.testing.assert_array_equal(out[(cover == 0)[..., 0]], photo[(cover == 0)[..., 0]])  # not a pixel elsewhere


@pytest.mark.parametrize(("size", "painted"), [(128, 2), (256, 4), (512, 8)], ids=lambda v: str(v))
def test_the_width_is_in_px_of_480_so_the_same_look_fits_any_render_size(photo, size, painted):
    """A line of 7.5 px (of 480) is 2 px when rendered at 128, 4 px at 256 and 8 px at 512."""
    look = Recipe([Step(NAME, params={"width": 7.5}, partition="panes")], name="seams", size=128, regions=REGIONS)
    out = render(photo, look, size=size)
    row = np.abs(out[0].astype(np.float64) - np.array(WHITE)).max(axis=1) < 1e-6  # the line is solid white
    assert row.sum() == painted


@pytest.mark.parametrize(
    ("partition", "why"),
    [
        pytest.param(np.zeros((96, 128), np.int32), "a single piece has no border", id="one piece"),
        pytest.param(None, "no partition, no borders", id="no partition"),
    ],
)
def test_without_a_border_the_picture_is_returned_unchanged(photo, partition, why):
    out = registry.apply(NAME, photo, partition=partition)
    np.testing.assert_array_equal(out, photo)
    assert out is not photo, why  # a new picture, as every effect returns


def test_a_width_of_nothing_draws_nothing(photo):
    np.testing.assert_array_equal(registry.apply(NAME, photo, width=0, partition=columns([64, 64])), photo)


def test_the_line_is_the_colour_asked_for_and_solid_where_it_covers_a_whole_pixel(photo):
    out = registry.apply(NAME, photo, width=4, color="#336699", partition=columns([64, 64]))
    inside = out[:, 62:66]
    np.testing.assert_allclose(inside, np.broadcast_to(np.array([0x33, 0x66, 0x99]) / 255, inside.shape), atol=1e-6)


@pytest.mark.parametrize("size", [(1, 1), (1, 7), (7, 1), (13, 29), (61, 37)], ids=lambda s: f"{s[0]}x{s[1]}")
def test_any_size_and_any_pieces_give_a_picture_in_range_and_a_hairline_touches_only_pixels_beside_a_border(size):
    rng = np.random.default_rng(3)
    rgb = rng.random((*size, 3), dtype=np.float32)
    labels = rng.integers(0, 4, size).astype(np.int32)
    out = registry.apply(NAME, rgb, width=0.5, color=RED, partition=labels)  # half a pixel wide
    assert out.shape == rgb.shape
    assert out.min() >= 0
    assert out.max() <= 1
    height, width = size
    for y in range(height):
        for x in range(width):
            neighbours = [(y + dy, x + dx) for dy, dx in ((0, 1), (0, -1), (1, 0), (-1, 0))]
            beside = any(
                0 <= ny < height and 0 <= nx < width and labels[ny, nx] != labels[y, x] for ny, nx in neighbours
            )
            if not beside:
                np.testing.assert_array_equal(out[y, x], rgb[y, x])


# --- with the rest of a recipe: a region of the recipe is the partition, and mask and each work as for any step -------

# Widths below are in design px (of 480): 7.5 of them are 2 px in the 128 px wide pictures rendered here.
REGIONS: dict[str, Grid | Pick] = {"panes": Grid(columns=2, rows=1), "head": Pick(of="panes", pieces=[0])}


def run(steps, **keys):
    photo = np.random.default_rng(1).random((96, 128, 3), dtype=np.float32)
    return photo, render(photo, Recipe(steps, name="seams", size=128, regions=REGIONS), **keys)


def test_a_step_draws_the_borders_of_the_region_it_names():
    photo, out = run([Step(NAME, params={"width": 7.5, "color": "#ff0000"}, partition="panes")])
    assert (out[:, 63:65] == RED).all()
    np.testing.assert_array_equal(out[:, :63], photo[:, :63])
    np.testing.assert_array_equal(out[:, 65:], photo[:, 65:])


def test_a_selection_counts_as_a_partition_of_two_pieces_so_a_hard_edge_gets_a_line(subject):
    photo = np.random.default_rng(1).random((96, 128, 3), dtype=np.float32)
    step = Step(NAME, params={"width": 7.5, "color": "#ff0000"}, partition="subject")
    out = render(photo, Recipe([step], name="outline", size=128), maps={"subject": subject})
    painted = (out != photo).any(axis=2)
    inside = subject > 0.5
    edge = inside ^ np.roll(inside, 1, axis=0) | inside ^ np.roll(inside, 1, axis=1)
    assert painted[edge].all()  # every pixel beside the disc's edge
    assert not painted[48, 64]  # the middle of the disc
    assert not painted[0, 0]  # a corner of the picture


def test_a_mask_limits_the_seams_like_any_step():
    photo, out = run([Step(NAME, params={"width": 7.5, "color": "#ff0000"}, partition="panes", mask="head")])
    assert (out[:, 63] == RED).all()  # the left piece, where the mask is
    np.testing.assert_array_equal(out[:, 64:], photo[:, 64:])


def test_as_the_look_of_an_each_step_only_the_pieces_with_that_look_show_the_line():
    step = Each(
        "panes", looks=[Step(NAME, params={"width": 7.5, "color": "#ff0000"}, partition="panes"), "none"], feather=0
    )
    photo, out = run([step])
    assert (out[:, 63] == RED).all()  # the left piece has the seams look, so its side of the border is drawn
    np.testing.assert_array_equal(out[:, 64:], photo[:, 64:])  # the right piece is left as it was
