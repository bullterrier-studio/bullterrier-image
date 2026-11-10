"""`seams` in a recipe, through the facade: the step's `partition` names a region of the recipe (or a selection, two
pieces), and mask and each work as for any step. What the line looks like is tested in the core
(`crates/core/tests/it/seams.rs`)."""

import numpy as np

from bullterrier_image.core import registry
from bullterrier_image.core.steps import Each, Recipe, Step
from bullterrier_image.recipe import Grid, Pick, render

NAME = "seams"
RED = (1.0, 0.0, 0.0)

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
        "panes", looks=[Step(NAME, params={"width": 7.5, "color": "#ff0000"}, partition="panes"), None], feather=0
    )
    photo, out = run([step])
    assert (out[:, 63] == RED).all()  # the left piece has the seams look, so its side of the border is drawn
    np.testing.assert_array_equal(out[:, 64:], photo[:, 64:])  # the right piece is left as it was


def test_called_directly_the_effect_takes_the_piece_labels_as_its_partition():
    photo = np.full((4, 8, 3), 0.5, np.float32)
    halves = np.repeat([[0] * 4 + [1] * 4], 4, axis=0).astype(np.int32)
    out = registry.apply(NAME, photo, partition=halves, width=2.0, color="#ff0000")
    assert (out[:, 3:5] == RED).all()  # the pixels either side of the border
    np.testing.assert_array_equal(out[:, [0, 1, 2, 5, 6, 7]], photo[:, [0, 1, 2, 5, 6, 7]])
