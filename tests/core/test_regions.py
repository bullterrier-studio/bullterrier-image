"""Tests of `core/regions.py`: regions (a partition into numbered pieces, or a selection of 0..1) made from declared
kinds, the resolver that builds them by name, and the weights that give each piece its look.

The oracles are written here with plain numpy and loops (a grid's pieces, reading order, nearest-site classification);
what a region is for (covering every pixel once, the same pieces at any resolution, seeds inside `fit`) is checked as
a property of the result.
"""

import tracemalloc
from collections.abc import Callable

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image import RecipeError, SubjectNotFoundError
from bullterrier_image.core.regions import (
    Edge,
    Finder,
    Grid,
    Kind,
    Partition,
    Pick,
    Selection,
    Shards,
    Subject,
    as_partition,
    check,
    look_weights,
    needs,
    needs_finder,
    needs_subject,
    resolve,
    used,
)

Shape = tuple[int, int]
# Sizes no cell divides: a single pixel, a single row or column, and two odd rectangles (as the effect contract uses).
ODD_SIZES = [(1, 1), (1, 7), (7, 1), (13, 29), (61, 37)]


def disc(shape: Shape) -> np.ndarray:
    """A solid disc in the middle of the picture, 0.3 of the long side across the radius (at least the middle pixel)."""
    height, width = shape
    y, x = np.mgrid[0:height, 0:width] + 0.5
    return (np.hypot(x - width / 2, y - height / 2) <= 0.3 * max(shape)).astype(np.float32)


def finder(shape: Shape) -> Callable[[], np.ndarray]:
    """A subject finder for a picture of `shape`, standing in for the model (an external boundary)."""
    return lambda: disc(shape)


def true_boundary(shape: Shape) -> Finder:
    """A finder for a picture of `shape` that answers with the disc where it lies in the box asked about: the subject
    exactly as the picture has it, however the picture is cut (the model's stand-in at the external boundary)."""
    whole = disc(shape)

    def find(box: tuple[int, int, int, int]) -> np.ndarray:
        left, top, right, bottom = box
        return whole[top:bottom, left:right]

    return find


def declared() -> dict[str, Kind]:
    """One region of each kind, with the shared fields used: `fit`, `edge`, a preset, a pick of a grid, the subject."""
    return {
        "grid": Grid(columns=3, rows=2),
        "uneven": Grid(columns=[1, 3, 0.5], rows=[2, 1]),
        "grid_on_subject": Grid(columns=2, rows=2, fit="subject"),
        "torn_grid": Grid(columns=4, rows=3, edge=Edge(style="torn")),
        "glass": Shards(preset="glass"),
        "torn": Shards(preset="torn"),
        "shards_on_subject": Shards(count=7, fit="subject"),
        "pick": Pick(of="grid", pieces=[0, 3]),
        "pick_of_nothing": Pick(of="grid", pieces=[99]),
        "subject_whole": Subject(),
        "subject_per_grid": Subject(per="grid"),
        "subject_per_shards": Subject(per="glass", context=0.5),
    }


def first_pixels(labels: np.ndarray) -> list[int]:
    """Where in the raster scan (left to right along each row, rows top to bottom) each piece is first met, by piece."""
    flat = labels.ravel()
    return [int(np.flatnonzero(flat == piece)[0]) for piece in range(int(flat.max()) + 1)]


def assert_a_region(region: Partition | Selection, shape: Shape) -> None:
    """The promises of both kinds: a partition numbers the pieces that exist 0..n-1 in reading order and gives every
    pixel one; a selection holds floats from 0 to 1; neither can be written to."""
    match region:
        case Partition(labels=labels, count=count):
            assert labels.shape == shape
            assert labels.dtype == np.int32
            np.testing.assert_array_equal(np.unique(labels), np.arange(count))
            assert first_pixels(labels) == sorted(first_pixels(labels))
            assert not labels.flags.writeable
        case Selection(alpha=alpha):
            assert alpha.shape == shape
            assert alpha.dtype == np.float32
            assert alpha.min() >= 0
            assert alpha.max() <= 1
            assert not alpha.flags.writeable
        case _:
            pytest.fail(f"neither a partition nor a selection: {region!r}")


def labels_of(regions: dict, name: str, shape: Shape) -> np.ndarray:
    """The pieces of the partition `name` of `regions` over a picture of `shape`."""
    return resolve(regions, shape, subject=finder(shape), find=true_boundary(shape)).partition(name).labels


def same_pieces(labels: np.ndarray, other: np.ndarray) -> np.ndarray:
    """Where `other` gives a pixel the piece that `labels` does, matching pieces by their overlap since the two may
    number them differently: each piece of `labels` is taken to be the piece of `other` it covers most."""
    pairs = np.bincount(
        labels.ravel() * (other.max() + 1) + other.ravel(), minlength=(labels.max() + 1) * (other.max() + 1)
    )
    match = pairs.reshape(labels.max() + 1, other.max() + 1).argmax(axis=1)
    return match[labels] == other


# ---- every kind: the promises of a region -------------------------------------------------------------------------


@pytest.mark.parametrize("size", ODD_SIZES, ids=lambda s: f"{s[0]}x{s[1]}")
@pytest.mark.parametrize("name", list(declared()))
def test_every_kind_gives_a_partition_or_selection_at_any_size_and_the_same_one_every_time(name, size):
    first = resolve(declared(), size, subject=finder(size), find=true_boundary(size))[name]
    again = resolve(declared(), size, subject=finder(size), find=true_boundary(size))[name]
    assert_a_region(first, size)
    match first, again:
        case Partition(), Partition():
            np.testing.assert_array_equal(first.labels, again.labels)
        case Selection(), Selection():
            np.testing.assert_array_equal(first.alpha, again.alpha)
        case _:
            pytest.fail("the same declaration made two kinds of region")


def extremes(model: type, **fixed: object) -> list[tuple[str, dict]]:
    """Every numeric field of `model` at its documented minimum and at its maximum, as (id, values)."""
    return [
        (f"{doc.name}={value}", {**fixed, doc.name: value})
        for doc in model.fields_doc()  # type: ignore[attr-defined]
        if doc.minimum is not None
        for value in (doc.minimum, doc.maximum)
    ]


AT_THE_LIMITS: dict[str, Kind] = {
    **{f"shards {id_}": Shards(**values) for id_, values in extremes(Shards)},
    **{f"edge {id_}": Shards(edge=Edge(style="torn", **values)) for id_, values in extremes(Edge)},
    "shards impact at one corner": Shards(impact=(0, 0)),
    "shards impact at the other": Shards(impact=(1, 1)),
    "grid of one": Grid(columns=1, rows=1),
    "grid of most": Grid(columns=1000, rows=1000),
    "grid of one share": Grid(columns=[1], rows=[1]),
    "grid of tiny and huge shares": Grid(columns=[0.001, 1000], rows=[1000, 0.001]),
    "grid on the subject, torn": Grid(columns=3, rows=3, fit="subject", edge=Edge(style="torn", amount=0.1)),
    **{f"subject {id_}": Subject(per="panes", **values) for id_, values in extremes(Subject)},
}


@pytest.mark.parametrize("size", [(13, 29), (61, 37), (1, 1)], ids=lambda s: f"{s[0]}x{s[1]}")
@pytest.mark.parametrize("name", AT_THE_LIMITS)
def test_every_field_works_at_its_minimum_and_maximum(name, size):
    regions = {"panes": Grid(columns=3, rows=2), "it": AT_THE_LIMITS[name]}
    resolved = resolve(regions, size, subject=finder(size), find=true_boundary(size))["it"]
    assert_a_region(resolved, size)


def test_pieces_are_numbered_in_reading_order_so_a_looks_list_is_predictable():
    """Top to bottom, left to right, by where each piece is first met: for a grid, row after row."""
    labels = labels_of({"g": Grid(columns=3, rows=2)}, "g", (20, 30))
    assert labels[0, 0] == 0
    assert labels[0, 29] == 2
    assert labels[19, 0] == 3
    assert labels[19, 29] == 5


def test_a_selection_of_a_picture_with_fewer_pieces_than_picked_selects_what_exists():
    """The same recipe must work on a thumbnail, where a 3 x 2 grid of a 1 x 1 picture has only one piece."""
    regions = resolve({"g": Grid(columns=3, rows=2), "p": Pick(of="g", pieces=[0, 5])}, (1, 1), subject=None)
    assert regions.selection("p").alpha.tolist() == [[1.0]]


# ---- the same pieces at any resolution -----------------------------------------------------------------------------


@pytest.mark.parametrize(
    "kind",
    [
        Grid(columns=3, rows=4),
        Grid(columns=[1, 2], rows=[3, 1, 1]),
        Shards(preset="glass"),
        Shards(preset="mosaic"),
        Grid(columns=3, rows=4, edge=Edge(style="torn", amount=0.08, scale=0.3, seed=3)),
        Shards(count=12, edge=Edge(style="torn", amount=0.08, scale=0.3, seed=5)),
        Shards(preset="torn"),
        Shards(preset="stone"),
    ],
    ids=["grid", "uneven grid", "glass", "mosaic", "torn grid", "wide torn shards", "torn", "stone"],
)
def test_the_same_pieces_come_out_at_twice_the_resolution(kind):
    """Positions are shares of the frame, so the sites sit at the same share. A coarse pixel's centre is the centre of
    a 2 x 2 block of fine pixels, so its piece must be one of the four the block holds, unless a piece is thinner than
    a pixel there: at most 1 coarse pixel in 100 may fail that. A piece smaller than a coarse pixel may have no pixel
    there, so the coarse picture may have fewer pieces, never more. A torn edge bends the positions by noise defined
    in shares of the frame too, so it is the same tear: it differs only where the coarse picture leaves out a layer
    finer than its pixel, which moves a border by a fraction of a pixel. A different tear (the wide ones here, 8% of
    the long side, 7 px on the coarse picture) fails it."""
    coarse = labels_of({"k": kind}, "k", (60, 90))
    fine = labels_of({"k": kind}, "k", (120, 180))
    assert coarse.max() <= fine.max()
    blocks = np.stack([fine[a::2, b::2] for a in (0, 1) for b in (0, 1)])  # the four fine pixels of each coarse one
    # which fine piece each coarse piece is: the one most often found in its blocks
    votes = np.zeros((coarse.max() + 1, fine.max() + 1), int)
    for block in blocks:
        np.add.at(votes, (coarse.ravel(), block.ravel()), 1)
    match = votes.argmax(axis=1)
    assert len(set(match)) == len(match)
    found = (blocks == match[coarse]).any(axis=0)
    assert (~found).mean() < 0.01


# ---- grid ----------------------------------------------------------------------------------------------------------


def grid_oracle(
    shape: Shape, columns: list[float], rows: list[float], box: tuple[float, float, float, float]
) -> np.ndarray:
    """The pieces of a grid laid over `box` (x0, y0, x1, y1 in px), pixel by pixel: a pixel is in the column whose
    range holds its centre, a centre on a border going to the later column; pieces numbered by first appearance in the
    raster scan."""
    x0, y0, x1, y1 = box

    def edges(shares: list[float], lo: float, hi: float) -> list[float]:
        return [lo + (hi - lo) * sum(shares[: i + 1]) / sum(shares) for i in range(len(shares) - 1)]

    cuts_x, cuts_y = edges(columns, x0, x1), edges(rows, y0, y1)
    labels = np.zeros(shape, np.int32)
    names: dict[tuple[int, int], int] = {}
    for i in range(shape[0]):
        for j in range(shape[1]):
            cell = (sum(c <= i + 0.5 for c in cuts_y), sum(c <= j + 0.5 for c in cuts_x))
            labels[i, j] = names.setdefault(cell, len(names))
    return labels


GRIDS = {
    "3 by 2": ((20, 30), Grid(columns=3, rows=2), [1] * 3, [1] * 2),
    "uneven shares": ((17, 23), Grid(columns=[1, 2, 1], rows=[3, 1]), [1, 2, 1], [3, 1]),
    "a border through pixel centres": ((6, 3), Grid(columns=2, rows=4), [1, 1], [1] * 4),
    "one column of shares": ((9, 11), Grid(columns=[5], rows=3), [5], [1] * 3),
    "more pieces than pixels": ((2, 3), Grid(columns=5, rows=4), [1] * 5, [1] * 4),
}


@pytest.mark.parametrize(("shape", "grid", "columns", "rows"), GRIDS.values(), ids=GRIDS.keys())
def test_a_grids_pieces_are_those_of_an_independent_pixel_by_pixel_oracle(shape, grid, columns, rows):
    want = grid_oracle(shape, columns, rows, (0, 0, shape[1], shape[0]))
    np.testing.assert_array_equal(labels_of({"g": grid}, "g", shape), want)


def test_a_grid_with_fit_is_laid_over_the_selections_box_and_its_outer_pieces_run_to_the_picture_edge():
    shape = (20, 30)
    box = np.zeros(shape, np.float32)
    box[5:15, 8:22] = 1  # rows 5..14, columns 8..21: x from 8 to 22, y from 5 to 15
    regions = resolve({"g": Grid(columns=[1, 3], rows=2, fit="subject")}, shape, subject=lambda: box)
    want = grid_oracle(shape, [1, 3], [1, 1], (8, 5, 22, 15))
    np.testing.assert_array_equal(regions.partition("g").labels, want)
    assert want[0, 0] != want[19, 29]  # the oracle's own corners differ: the pieces beyond the box are not lost


# ---- shards --------------------------------------------------------------------------------------------------------


def test_shards_make_the_number_of_pieces_asked_for_and_another_seed_cuts_differently():
    shape = (96, 128)
    cut = {name: Shards(count=30, seed=seed) for name, seed in (("a", 1), ("again", 1), ("b", 2))}
    pieces = {name: labels_of(cut, name, shape) for name in cut}
    assert pieces["a"].max() + 1 == 30
    np.testing.assert_array_equal(pieces["a"], pieces["again"])
    assert not np.array_equal(pieces["a"], pieces["b"])


@pytest.mark.parametrize("count", [1, 2, 3, 7, 13, 29, 31, 100], ids=lambda n: f"{n} shards")
@pytest.mark.parametrize("fit", [None, "subject"], ids=["the whole picture", "the subject"])
def test_shards_make_exactly_the_number_of_pieces_asked_for_whatever_the_lattice_gives(count, fit):
    shape = (120, 160)
    assert labels_of({"s": Shards(count=count, jitter=1, fit=fit)}, "s", shape).max() + 1 == count


def test_zero_jitter_cuts_a_regular_honeycomb_and_full_jitter_an_uneven_scatter():
    def unevenness(jitter: float) -> float:
        areas = np.bincount(labels_of({"s": Shards(count=40, jitter=jitter)}, "s", (120, 120)).ravel())
        return float(areas.std() / areas.mean())

    assert unevenness(0) < 0.5 * unevenness(1)


def test_focus_crowds_small_shards_round_the_impact_point():
    shape = (120, 120)
    cut = Shards(count=60, focus=3, impact=(0.25, 0.75))
    labels = labels_of({"s": cut}, "s", shape)
    areas = np.bincount(labels.ravel())
    at_impact = labels[int(0.75 * 120), int(0.25 * 120)]
    assert areas[at_impact] < np.median(areas) / 2


def test_every_shard_laid_over_a_selection_has_a_pixel_inside_it_so_seeds_fall_inside_fit():
    shape = (96, 128)
    cut = {"s": Shards(count=12, jitter=1, fit="subject")}
    labels = labels_of(cut, "s", shape)
    inside = disc(shape) > 0.5
    assert labels.max() + 1 == 12
    assert set(np.unique(labels[inside])) == set(range(12))  # a piece with no pixel there had its seed outside


@pytest.mark.parametrize("count", [2, 3, 5, 8, 13, 21])
def test_every_seed_lies_inside_a_thin_selection_the_lattice_mostly_misses(count):
    """A one-pixel-wide diagonal: the box is the whole picture and few lattice points fall on the line, so seeds are
    dropped or added to make `count`, and each must lie on the line, so that every piece has a pixel of it."""
    shape = (80, 120)
    line = np.zeros(shape, np.float32)
    for row in range(80):
        line[row, int(row * 1.45)] = 1
    labels = resolve({"s": Shards(count=count, fit="subject")}, shape, subject=lambda: line).partition("s").labels
    assert set(np.unique(labels[line > 0])) == set(range(count))


def test_zero_jitter_cuts_a_selection_into_an_even_honeycomb_whatever_the_box_holds():
    """A disc and a speck in the far corner: the box is nearly the whole picture, the disc a third of it. Most pieces
    whose cell lies wholly inside the selection are one size, within 15%: the lattice is sized to the selection's
    area, not the box's. (Seeds dropped or added to make `count` spoil a few: not all of them.)"""
    shape = (150, 200)
    chosen = disc(shape)
    chosen[-5:, -5:] = 1
    labels = (
        resolve({"s": Shards(count=30, jitter=0, fit="subject")}, shape, subject=lambda: chosen).partition("s").labels
    )
    areas = np.bincount(labels.ravel())
    whole = [piece for piece in range(len(areas)) if (chosen[labels == piece] > 0.5).all()]
    assert len(whole) >= 4
    typical = np.median(areas[whole])
    assert (abs(areas[whole] / typical - 1) <= 0.15).mean() >= 0.6


def test_the_frame_of_a_soft_selection_is_where_it_chooses_more_than_half():
    shape = (20, 30)
    matte = np.zeros(shape, np.float32)
    matte[3:17, 4:26] = 0.3  # a faint halo: not chosen
    matte[5:15, 8:22] = 0.7  # the fringe of the subject: chosen
    matte[7:12, 12:18] = 1
    regions = resolve({"g": Grid(columns=[1, 3], rows=1, fit="subject")}, shape, subject=lambda: matte)
    want = grid_oracle(shape, [1, 3], [1], (8, 5, 22, 15))
    np.testing.assert_array_equal(regions.partition("g").labels, want)


def test_shards_laid_over_a_selection_are_smaller_inside_it_than_without_fit():
    shape = (96, 128)
    inside = disc(shape) > 0.5
    plain = labels_of({"s": Shards(count=12)}, "s", shape)
    fitted = labels_of({"s": Shards(count=12, fit="subject")}, "s", shape)
    assert len(np.unique(fitted[inside])) > len(np.unique(plain[inside]))


# ---- edges ---------------------------------------------------------------------------------------------------------


def test_a_straight_edge_ignores_the_other_edge_settings_and_a_torn_one_with_no_amount_is_straight():
    shape = (60, 80)
    straight = labels_of({"k": Shards(count=20)}, "k", shape)
    for edge in (Edge(amount=0.1, scale=0.5, seed=9), Edge(style="torn", amount=0)):
        np.testing.assert_array_equal(labels_of({"k": Shards(count=20, edge=edge)}, "k", shape), straight)


@pytest.mark.parametrize("amount", [0.01, 0.05], ids=["a small tear", "a wide tear"])
@pytest.mark.parametrize("kind", [Grid, Shards], ids=["grid", "shards"])
def test_a_torn_edge_moves_the_borders_by_about_its_amount_and_nothing_else(kind, amount):
    """The coordinates are shifted by noise of unit standard deviation times `amount` of the long side, so a pixel
    may change piece only if a straight border lies within a few standard deviations: 6, as the noise is smooth and a
    value that far out has a chance below 1e-8 of showing in a picture of this size."""
    shape = (150, 200)
    straight = kind(edge=Edge(style="straight"), **({"columns": 3, "rows": 3} if kind is Grid else {"count": 12}))
    torn = straight.model_copy(update={"edge": Edge(style="torn", amount=amount, scale=0.3, seed=4)})
    before = labels_of({"k": straight}, "k", shape)
    after = labels_of({"k": torn}, "k", shape)
    assert after.max() == before.max()
    on_border = np.zeros(shape, bool)
    on_border[1:] |= before[1:] != before[:-1]
    on_border[:, 1:] |= before[:, 1:] != before[:, :-1]
    from_border = ndimage.distance_transform_edt(~on_border)
    moved = ~same_pieces(before, after)
    assert moved.any()  # it tears at all
    assert from_border[moved].max() <= 6 * amount * 200 + 1


def border_length(labels: np.ndarray) -> int:
    """How many pixel sides lie between two pieces."""
    return int((labels[1:] != labels[:-1]).sum() + (labels[:, 1:] != labels[:, :-1]).sum())


@pytest.mark.parametrize(
    ("smoother", "rougher"),
    [
        pytest.param({"persistence": 0.0}, {"persistence": 1.0}, id="persistence"),
        pytest.param({"octaves": 1}, {"octaves": 8}, id="octaves"),
        pytest.param({"scale": 0.5}, {"scale": 0.02}, id="scale"),
    ],
)
def test_a_rougher_tear_has_longer_borders(smoother, rougher):
    """Raising `persistence` or `octaves` adds fine layers to the wander, lowering `scale` makes it finer."""
    shape = (240, 320)

    def tear(**fields: float) -> Grid:
        return Grid(columns=2, rows=2, edge=Edge(style="torn", amount=0.02, **{"scale": 0.1, **fields}))

    assert border_length(labels_of({"k": tear(**smoother)}, "k", shape)) < border_length(
        labels_of({"k": tear(**rougher)}, "k", shape)
    )


def test_a_different_edge_seed_tears_differently():
    shape = (80, 120)
    a, b = (labels_of({"k": Grid(columns=3, rows=3, edge=Edge(style="torn", seed=s))}, "k", shape) for s in (1, 2))
    assert not np.array_equal(a, b)


# ---- pick, subject, background -------------------------------------------------------------------------------------


def test_pick_selects_exactly_the_pixels_of_the_pieces_listed():
    shape = (20, 30)
    regions = resolve({"g": Grid(columns=3, rows=2), "p": Pick(of="g", pieces=[1, 4])}, shape, subject=None)
    labels = regions.partition("g").labels
    np.testing.assert_array_equal(regions.selection("p").alpha, np.isin(labels, [1, 4]).astype(np.float32))
    assert regions.selection("p").alpha.sum() == 2 * 10 * 10


def test_the_subject_is_the_finders_matte_and_the_background_is_what_is_left():
    shape = (96, 128)
    regions = resolve({}, shape, subject=finder(shape))
    np.testing.assert_array_equal(regions.selection("subject").alpha, disc(shape))
    np.testing.assert_allclose(regions.selection("background").alpha, 1 - disc(shape), atol=1e-7)
    assert set(regions) == {"subject", "background"}


def test_the_finder_runs_only_if_a_region_needs_the_subject_and_then_once():
    calls = []

    def counting() -> np.ndarray:
        calls.append(1)
        return disc((20, 30))

    regions = resolve(
        {"g": Grid(columns=2), "s": Shards(count=3, fit="subject"), "t": Shards(count=4, fit="background")},
        (20, 30),
        subject=counting,
    )
    assert "s" in regions
    assert "subject" in regions
    assert "nope" not in regions
    regions["g"]
    assert calls == []  # neither asking whether a region exists nor making a grid needed the subject
    regions["s"], regions["t"], regions["subject"], regions["background"]
    assert calls == [1]


def test_regions_are_made_once_and_handed_out_read_only():
    regions = resolve(declared(), (20, 30), subject=finder((20, 30)))
    assert regions["glass"] is regions["glass"]
    with pytest.raises(ValueError, match="read-only"):
        regions.partition("glass").labels[0, 0] = 5


def test_a_finder_that_returns_a_matte_of_another_size_is_refused():
    regions = resolve({"s": Shards(fit="subject")}, (20, 30), subject=lambda: np.ones((10, 10), np.float32))
    with pytest.raises(SubjectNotFoundError, match="10"):
        regions["s"]


def test_a_region_that_needs_the_subject_says_so_when_there_is_no_finder():
    regions = resolve({"s": Shards(fit="subject"), "g": Grid()}, (20, 30), subject=None)
    regions["g"]
    with pytest.raises(SubjectNotFoundError, match="subject"):
        regions["s"]


def test_laying_a_kind_over_a_selection_that_chooses_nothing_is_refused_naming_the_region():
    regions = resolve({"s": Shards(fit="subject")}, (20, 30), subject=lambda: np.zeros((20, 30), np.float32))
    with pytest.raises(RecipeError, match=r"'s'.*subject.*nothing"):
        regions["s"]


def test_asking_for_a_region_as_the_wrong_kind_is_refused_naming_it():
    regions = resolve({"g": Grid(columns=2), "p": Pick(of="g", pieces=[0])}, (20, 30), subject=None)
    with pytest.raises(RecipeError, match=r"'g'.*partition.*selection"):
        regions.selection("g")
    with pytest.raises(RecipeError, match=r"'p'.*selection.*partition"):
        regions.partition("p")
    with pytest.raises(KeyError, match="nope"):
        regions["nope"]


def test_a_hard_edged_selection_is_a_two_piece_partition_of_what_it_chooses_and_what_it_does_not():
    soft = Selection(np.array([[0, 0.2, 0.5, 0.5001, 0.8, 1]] * 2, np.float32))  # exactly half is not more than half
    pieces = as_partition(soft)
    assert_a_region(pieces, (2, 6))
    chosen = soft.alpha > 0.5
    assert chosen.tolist() == [[False, False, False, True, True, True]] * 2
    assert pieces.count == 2
    assert (pieces.labels == pieces.labels[0, 0]).tolist() == (~chosen).tolist()
    assert as_partition(Selection(np.ones((3, 3), np.float32))).count == 1


# ---- declared regions are checked ----------------------------------------------------------------------------------

BAD_DECLARATIONS = {
    "fit names no region": ({"glass": Shards(fit="nope")}, "'glass'.*'nope'.*subject, background"),
    "fit names a partition": ({"g": Grid(), "s": Shards(fit="g")}, "'s'.*fit.*'g'.*partition.*selection"),
    "pick of a selection": ({"p": Pick(of="subject")}, "'p'.*'subject'.*selection.*partition"),
    "pick of nothing": ({"p": Pick(of="gone", pieces=[0])}, "'p'.*'gone'.*valid"),
    "pick left unset": ({"g": Grid(), "p": Pick()}, "'p'.*valid.*g"),
    "a region that picks itself": ({"p": Pick(of="p")}, "'p'.*`of`.*selection.*partition"),
    "a cycle through two": ({"s": Shards(fit="p"), "p": Pick(of="s")}, "cycle.*'s'.*'p'"),
    "a name taken by a built-in": ({"subject": Grid()}, "'subject'.*built-in"),
    "per names a selection": ({"s": Subject(per="subject")}, "'s'.*per.*'subject'.*selection.*partition"),
    "per names no region": ({"g": Grid(), "s": Subject(per="gone")}, "'s'.*'gone'.*valid.*g"),
    "a cycle through per": ({"s": Subject(per="p"), "p": Shards(fit="s")}, "cycle.*'s'.*'p'"),
}


@pytest.mark.parametrize(("regions", "complaint"), BAD_DECLARATIONS.values(), ids=BAD_DECLARATIONS.keys())
def test_a_declaration_that_refers_wrongly_is_refused_naming_the_region_and_what_is_valid(regions, complaint):
    with pytest.raises(RecipeError, match=complaint):
        resolve(regions, (20, 30), subject=None)


def test_the_references_are_checked_even_for_regions_no_step_will_ask_for():
    with pytest.raises(RecipeError, match="unused"):
        resolve({"g": Grid(), "unused": Pick(of="missing")}, (20, 30), subject=None)


NEEDS = {
    "a selection where one is wanted": ("step 2", "mask", "head", "selection", True),
    "a partition where one is wanted": ("step 2", "each", "panes", "partition", True),
    "a built-in selection": ("step 2", "mask", "background", "selection", True),
    "any region, a partition": ("step 2", "partition", "panes", None, True),
    "any region, a selection": ("step 2", "partition", "head", None, True),
    "a partition where a selection is wanted": ("step 2", "mask", "panes", "selection", False),
    "a selection where a partition is wanted": ("step 2", "each", "head", "partition", False),
    "no such region": ("step 2", "mask", "nope", "selection", False),
}


@pytest.mark.parametrize(("owner", "field", "name", "wanted", "fine"), NEEDS.values(), ids=NEEDS.keys())
def test_needs_accepts_a_region_of_the_wanted_kind_and_otherwise_names_the_owner_field_and_valid_names(
    owner, field, name, wanted, fine
):
    declared: dict[str, Kind] = {"panes": Grid(columns=2), "head": Pick(of="panes", pieces=[1])}
    if fine:
        needs(owner, field, name, wanted, declared)
        return
    with pytest.raises(RecipeError, match=rf"^{owner}: `{field}` names '{name}'.*valid .*s: ") as raised:
        needs(owner, field, name, wanted, declared)
    listed = str(raised.value).split("valid ", 1)[1]
    assert ("panes" in listed) == (wanted != "selection")  # only the regions of the wanted kind are listed
    assert ("head" in listed) == (wanted != "partition")


def test_check_refuses_what_resolve_refuses_without_needing_a_picture():
    check({"g": Grid(), "p": Pick(of="g")})
    with pytest.raises(RecipeError, match=r"'p'.*'gone'"):
        check({"p": Pick(of="gone")})


NEEDS_SUBJECT = {
    "nothing asked": ([], False),
    "a grid": (["grid"], False),
    "a pick of a grid": (["pick"], False),
    "the subject itself": (["subject"], True),
    "the background": (["background"], True),
    "shards laid over the subject": (["shards_on_subject"], True),
    "a pick of a grid laid over the subject": (["grid_on_subject", "grid"], True),
    "a region and, among others, one that does": (["grid", "shards_on_subject"], True),
    "a region laid over a pick of a grid": (["over_pick"], False),
    "a region two steps away from the subject": (["far"], True),
    "the subject found piece by piece needs no subject of the whole picture": (["subject_per_grid"], False),
    "the subject without `per` is the subject": (["subject_whole"], True),
    "a region laid over the subject found piece by piece": (["over_found"], False),
}


@pytest.mark.parametrize(("names", "expected"), NEEDS_SUBJECT.values(), ids=NEEDS_SUBJECT.keys())
def test_regions_need_the_subject_when_they_are_made_from_it_however_indirectly(names, expected):
    regions = {
        **declared(),
        "over_pick": Shards(fit="pick"),
        "inner": Grid(fit="subject"),
        "pick_inner": Pick(of="inner"),
        "far": Shards(fit="pick_inner"),
        "over_found": Shards(fit="subject_per_grid"),
    }
    check(regions)
    assert needs_subject(regions, names) is expected


USED = {
    "nothing": ([], set()),
    "a grid": (["grid"], {"grid"}),
    "a pick brings the partition it picks from": (["pick"], {"pick", "grid"}),
    "the built-in ones are named too": (["subject", "grid"], {"subject", "grid"}),
    "a region laid over a pick laid over a grid laid over the subject": (
        ["far"],
        {"far", "pick_inner", "inner", "subject"},
    ),
    "two asking for the same": (["pick", "grid", "pick"], {"pick", "grid"}),
}


@pytest.mark.parametrize(("names", "expected"), USED.values(), ids=USED.keys())
def test_the_regions_used_are_those_named_and_those_they_are_made_from(names, expected):
    regions = {
        **declared(),
        "inner": Grid(fit="subject"),
        "pick_inner": Pick(of="inner"),
        "far": Shards(fit="pick_inner"),
    }
    assert used(regions, names) == expected


def test_a_selection_asked_for_as_pieces_is_split_and_a_partition_is_kept():
    regions = resolve({"g": Grid(columns=2)}, (4, 6), subject=finder((4, 6)))
    assert regions.pieces("g") is regions.partition("g")
    split = regions.pieces("subject")
    assert split.count == 2
    np.testing.assert_array_equal(split.labels, as_partition(regions.selection("subject")).labels)


# ---- the models ----------------------------------------------------------------------------------------------------

NEEDS_FINDER = {
    "nothing asked": ([], False),
    "a grid": (["grid"], False),
    "the built-in subject": (["subject"], False),
    "the subject without `per`": (["subject_whole"], False),
    "the subject found piece by piece": (["subject_per_grid"], True),
    "a pick of shards laid over that subject": (["pick_found"], True),
}


@pytest.mark.parametrize(("names", "expected"), NEEDS_FINDER.values(), ids=NEEDS_FINDER.keys())
def test_regions_need_a_finder_to_ask_about_pieces_only_when_one_found_piece_by_piece_is_used(names, expected):
    regions = {**declared(), "over_found": Shards(fit="subject_per_grid"), "pick_found": Pick(of="over_found")}
    check(regions)
    assert needs_finder(regions, names) is expected


BAD_VALUES = {
    "no columns": (Grid, {"columns": 0}, "columns"),
    "too many rows": (Grid, {"rows": 1001}, "rows"),
    "no shares": (Grid, {"columns": []}, "columns"),
    "a share that is not positive": (Grid, {"rows": [1, 0]}, "rows"),
    "a share that is no number": (Grid, {"rows": [1, "a"]}, "rows"),
    "a misspelt field": (Grid, {"column": 3}, "column"),
    "no shards": (Shards, {"count": 0}, "count"),
    "too many shards": (Shards, {"count": 501}, "count"),
    "jitter past 1": (Shards, {"jitter": 1.5}, "jitter"),
    "negative focus": (Shards, {"focus": -1}, "focus"),
    "an impact outside the frame": (Shards, {"impact": (1.5, 0.5)}, "impact"),
    "an impact of one number": (Shards, {"impact": (0.5,)}, "impact"),
    "a negative seed": (Shards, {"seed": -1}, "seed"),
    "a preset that does not exist": (Shards, {"preset": "nope"}, "nope.*glass"),
    "an edge style that is not offered": (Shards, {"edge": {"style": "wavy"}}, "style"),
    "an edge amount that is too large": (Shards, {"edge": {"amount": 0.5}}, "amount"),
    "an edge with a misspelt field": (Grid, {"edge": {"stlye": "torn"}}, "stlye"),
    "no pieces to pick": (Pick, {"pieces": []}, "pieces"),
    "a negative piece": (Pick, {"pieces": [0, -1]}, "pieces"),
    "context past 1": (Subject, {"context": 1.5}, "context"),
    "negative context": (Subject, {"context": -0.1}, "context"),
    "threshold past 1": (Subject, {"threshold": 1.1}, "threshold"),
    "negative threshold": (Subject, {"threshold": -0.1}, "threshold"),
    "a per that is no text": (Subject, {"per": 3}, "per"),
}


@pytest.mark.parametrize(("model", "values", "complaint"), BAD_VALUES.values(), ids=BAD_VALUES.keys())
def test_a_bad_value_is_refused_naming_the_field(model, values, complaint):
    with pytest.raises(RecipeError, match=complaint):
        model(**values)


def test_values_written_as_a_recipe_file_writes_them_are_accepted():
    """TOML gives lists and tables, numpy arrays come from scripts: both make the same model."""
    from_file = Grid(columns=[1, 2], rows=[1, 1, 2], edge={"style": "torn", "amount": 0.02})
    from_script = Grid(columns=np.array([1, 2]), rows=(1, 1, 2), edge=Edge(style="torn", amount=0.02))
    assert from_file == from_script
    assert Shards(impact=[0.2, 0.8]).impact == (0.2, 0.8)
    assert Pick(pieces=np.array([2, 3])).pieces == (2, 3)


def test_a_preset_sets_the_fields_and_a_field_given_beside_it_wins():
    glass = Shards(preset="glass")
    assert glass.focus > 0  # a preset's own value
    assert Shards(preset="glass", focus=0).focus == 0
    assert Shards(preset="glass", count=5).count == 5
    assert Shards(preset="glass", count=5).focus == glass.focus


@pytest.mark.parametrize("preset", ["glass", "torn", "stone", "mosaic"])
def test_every_shipped_preset_cuts_a_picture_differently_from_the_default(preset):
    shape = (96, 128)
    default = labels_of({"s": Shards()}, "s", shape)
    named = labels_of({"s": Shards(preset=preset)}, "s", shape)
    assert named.shape == default.shape
    assert not np.array_equal(named, default)


@pytest.mark.parametrize("model", [Grid, Shards, Pick, Subject, Edge])
def test_every_field_is_documented_with_its_unit_and_range(model):
    text = model.documentation()
    for doc in model.fields_doc():
        assert doc.name in text
        assert doc.doc
        if doc.minimum is not None:
            assert doc.unit


# ---- the weights of the looks --------------------------------------------------------------------------------------


@pytest.mark.parametrize("feather", [0, 0.5, 3, 12], ids=lambda f: f"feather {f}")
@pytest.mark.parametrize(
    "assignment",
    [[0, 1], [1, 0], [2, 2], [0, 0]],
    ids=["one look each", "swapped", "both pieces the third look", "both the first"],
)
def test_the_weights_of_all_looks_sum_to_one_everywhere_and_stay_between_0_and_1(assignment, feather):
    shape = (37, 61)
    partition = resolve({"g": Grid(columns=2, rows=2)}, shape, subject=None).partition("g")
    weights = look_weights(partition, assignment * 2, 3, feather=feather)
    assert weights.shape == (3, *shape)
    assert weights.dtype == np.float32
    np.testing.assert_allclose(weights.sum(axis=0), 1, atol=1e-6)
    assert weights.min() >= 0
    assert weights.max() <= 1 + 1e-6


def test_without_feather_each_look_has_the_pixels_of_its_pieces_and_nothing_else():
    shape = (20, 30)
    partition = resolve({"g": Grid(columns=3, rows=2)}, shape, subject=None).partition("g")
    assignment = [2, 0, 2, 1, 1, 0]
    weights = look_weights(partition, assignment, 3, feather=0)
    for look in range(3):
        mine = [piece for piece, chosen in enumerate(assignment) if chosen == look]
        np.testing.assert_array_equal(weights[look], np.isin(partition.labels, mine).astype(np.float32))


def test_feather_softens_a_border_over_a_few_pixels_and_leaves_the_middle_of_a_piece_alone():
    shape = (40, 128)
    partition = resolve({"g": Grid(columns=2)}, shape, subject=None).partition("g")
    weights = look_weights(partition, [0, 1], 2, feather=2)
    row = weights[0, 20]
    # the border is at x = 64: the two pixels beside it share the look about equally, the first slightly more
    assert row[63] == pytest.approx(0.5, abs=0.15)
    assert row[63] > row[64]
    assert row[63] + row[64] == pytest.approx(1, abs=1e-6)
    assert row[0] == pytest.approx(1, abs=1e-6)
    assert row[40] == pytest.approx(1, abs=1e-6)
    assert row[100] == pytest.approx(0, abs=1e-6)
    assert 0 < row[60] < 1  # it is a ramp, not a step


def test_a_look_no_piece_has_gets_no_weight():
    partition = resolve({"g": Grid(columns=2)}, (10, 20), subject=None).partition("g")
    weights = look_weights(partition, [0, 2], 4, feather=1)
    assert weights[1].max() == 0
    assert weights[3].max() == 0


def test_weights_for_a_4k_picture_need_little_more_memory_than_the_weights_themselves():
    """The result is `count` picture-sized float32 maps; blurring each in turn must not multiply that (no
    supersampling). Budget: the maps plus 3 picture-sized buffers of working space (measured: the pieces' look map and
    one blur's output, 2)."""
    shape = (2160, 3840)
    partition = resolve({"g": Grid(columns=3, rows=2)}, shape, subject=None).partition("g")
    buffer = shape[0] * shape[1] * 4
    tracemalloc.start()
    try:
        held = tracemalloc.get_traced_memory()[0]
        weights = look_weights(partition, [0, 1, 2, 1, 2, 0], 3, feather=3)
        peak = tracemalloc.get_traced_memory()[1] - held
    finally:
        tracemalloc.stop()
    assert weights.shape == (3, *shape)
    assert peak <= (3 + 3) * buffer


@pytest.mark.parametrize(
    "kind",
    [Grid(columns=3, rows=2, edge=Edge(style="torn")), Shards(preset="glass"), Shards(preset="stone")],
    ids=["a torn grid", "glass", "stone"],
)
def test_making_the_pieces_of_a_1_mp_picture_needs_a_bounded_amount_of_memory(kind):
    """Measured on a 4K picture: 8.5 float32 picture-sized buffers for a grid, 14.5 for a torn one (the most: the
    noise is float64), 10 for glass. The budget leaves room for another platform, never for a blow-up."""
    shape = (866, 1155)
    buffer = shape[0] * shape[1] * 4
    regions = resolve({"k": kind}, shape, subject=None)
    tracemalloc.start()
    try:
        regions["k"]
        peak = tracemalloc.get_traced_memory()[1]
    finally:
        tracemalloc.stop()
    assert peak <= 20 * buffer


# ---- the subject found piece by piece (grid first) -----------------------------------------------------------------


class Recorder:
    """A finder that records the boxes it is asked about and answers by `answer(box)`: the model's stand-in."""

    def __init__(self, answer: Finder) -> None:
        self.answer = answer
        self.boxes: list[tuple[int, int, int, int]] = []

    def __call__(self, box: tuple[int, int, int, int]) -> np.ndarray:
        self.boxes.append(box)
        return self.answer(box)


def size_of(box: tuple[int, int, int, int]) -> tuple[int, int]:
    """The (height, width) of a view `(left, top, right, bottom)`."""
    return box[3] - box[1], box[2] - box[0]


def found(find: Finder, shape: Shape = (20, 30), per: Kind | None = None, **fields: object) -> np.ndarray:
    """The alpha of `Subject(per="panes", **fields)` over `shape`: a 2 x 2 grid (or `per`) found in by `find`."""
    regions = resolve(
        {"panes": per or Grid(columns=2, rows=2), "it": Subject(per="panes", **fields)}, shape, subject=None, find=find
    )
    return regions.selection("it").alpha


@pytest.mark.parametrize("context", [0, 0.25, 1], ids=lambda c: f"context {c}")
@pytest.mark.parametrize("per", [Grid(columns=3, rows=2), Shards(count=7, seed=3)], ids=["a grid", "shards"])
def test_a_finder_that_knows_the_true_boundary_is_united_into_that_boundary_whatever_the_pieces_and_context(
    per, context
):
    shape = (40, 60)
    assert_equal = np.testing.assert_array_equal
    assert_equal(found(true_boundary(shape), shape, per, context=context), disc(shape))


def test_without_per_the_subject_is_the_built_in_subject_and_the_finder_of_pieces_is_not_asked():
    shape = (20, 30)
    asked = Recorder(true_boundary(shape))
    regions = resolve({"it": Subject(threshold=1, context=1)}, shape, subject=finder(shape), find=asked)
    np.testing.assert_array_equal(regions.selection("it").alpha, regions.selection("subject").alpha)
    assert asked.boxes == []


def test_the_finder_is_asked_once_about_the_box_of_each_piece_however_often_the_region_is_asked_for():
    asked = Recorder(true_boundary((20, 30)))
    regions = resolve(
        {"panes": Grid(columns=2, rows=2), "it": Subject(per="panes", context=0)}, (20, 30), subject=None, find=asked
    )
    regions["it"], regions["it"]
    # (left, top, right, bottom): each piece is 15 px wide and 10 high
    assert sorted(asked.boxes) == [(0, 0, 15, 10), (0, 10, 15, 20), (15, 0, 30, 10), (15, 10, 30, 20)]


def test_context_widens_the_view_by_its_share_of_the_pieces_size_on_each_side_up_to_the_pictures_edge():
    asked = Recorder(true_boundary((20, 30)))
    found(asked, context=0.2)
    # a piece is 15 wide and 10 high: 0.2 of that is 3 px sideways and 2 px up and down, and the picture cuts the rest
    assert sorted(asked.boxes) == [(0, 0, 18, 12), (0, 8, 18, 20), (12, 0, 30, 12), (12, 8, 30, 20)]


def test_a_pixel_takes_its_value_from_the_view_of_its_own_piece_only():
    """Every view overlaps its neighbours' pieces; each answers with a value of its own over the whole view."""
    values = iter(
        [0.91, 0.93, 0.95, 0.97]
    )  # in the order of the pieces: top left, top right, bottom left, bottom right
    alpha = found(lambda box: np.full(size_of(box), next(values), np.float32), context=0.5)
    assert alpha[:10, :15] == pytest.approx(0.91)
    assert alpha[:10, 15:] == pytest.approx(0.93)
    assert alpha[10:, :15] == pytest.approx(0.95)
    assert alpha[10:, 15:] == pytest.approx(0.97)


def test_pixels_the_finder_is_not_surer_of_than_the_threshold_are_dropped_and_the_rest_kept_as_they_are():
    def blobs(box: tuple[int, int, int, int]) -> np.ndarray:
        left, top, _, _ = box
        matte = np.zeros(size_of(box), np.float32)
        if (left, top) == (0, 0):
            matte[2:5, 2:5] = 0.5  # exactly the threshold: dropped
            matte[6:8, 2:5] = 0.75
        if (left, top) == (15, 10):
            matte[2:5, 2:5] = 0.95
        return matte

    alpha = found(blobs, context=0, threshold=0.5)
    expected = np.zeros((20, 30), np.float32)
    expected[6:8, 2:5] = 0.75
    expected[12:15, 17:20] = 0.95
    np.testing.assert_array_equal(alpha, expected)
    strict = found(blobs, context=0, threshold=0.8)
    np.testing.assert_array_equal(strict, np.where(expected == 0.95, expected, 0))  # only the confident blob is left


def test_a_faint_subject_invented_over_a_whole_piece_is_dropped_and_a_confident_small_one_and_the_true_ones_are_kept():
    shape = (20, 30)

    def invents_in_the_last_piece(box: tuple[int, int, int, int]) -> np.ndarray:
        if box[:2] != (15, 10):
            return true_boundary(shape)(box)
        matte = np.full(size_of(box), 0.6, np.float32)  # the whole piece, but the finder is not sure
        matte[1:3, 1:3] = 0.95  # and a small thing it is sure of
        return matte

    expected = disc(shape)
    expected[10:, 15:] = 0
    expected[11:13, 16:18] = 0.95
    np.testing.assert_array_equal(found(invents_in_the_last_piece, context=0), expected)


def test_a_piece_the_finder_finds_nothing_in_adds_nothing_and_the_others_are_not_affected():
    shape = (20, 30)

    def nothing_on_the_right(box: tuple[int, int, int, int]) -> np.ndarray:
        if box[0] == 15:
            raise SubjectNotFoundError("nothing stands out")
        return true_boundary(shape)(box)

    expected = disc(shape)
    expected[:, 15:] = 0
    np.testing.assert_array_equal(found(nothing_on_the_right, context=0), expected)


def test_a_finder_that_answers_a_matte_that_does_not_fit_its_view_is_refused_naming_both_sizes():
    with pytest.raises(SubjectNotFoundError, match=r"\(7, 7\).*\(10, 15\)"):
        found(lambda box: np.ones((7, 7), np.float32), context=0)


def test_a_subject_found_piece_by_piece_says_so_when_there_is_no_finder_for_pieces():
    regions = resolve({"panes": Grid(), "it": Subject(per="panes")}, (20, 30), subject=finder((20, 30)))
    with pytest.raises(SubjectNotFoundError, match=r"'it'.*panes"):
        regions["it"]


def test_shards_can_be_laid_over_the_subject_found_piece_by_piece():
    shape = (40, 60)
    regions = resolve(
        {"panes": Grid(columns=2, rows=2), "it": Subject(per="panes", context=0), "cut": Shards(count=5, fit="it")},
        shape,
        subject=None,
        find=true_boundary(shape),
    )
    cut = regions.partition("cut")
    assert_a_region(cut, shape)
    assert cut.count == 5
