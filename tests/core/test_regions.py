"""Tests of regions through a recipe: a partition into numbered pieces, or a selection of 0..1, made from declared
kinds, and the models that declare them.

A region is looked at as the steps of a recipe are given it: a probe step paints the piece number of each pixel (or
the selection itself), and the picture rendered is read back. The oracles are written here with plain numpy; what a
region is for (covering every pixel once, the same pieces at any resolution) is checked as a property of the result.
The rules of each kind (where a grid's borders fall, what a band chooses, how an svg is placed) are held by the Rust
tests of the core.
"""

from itertools import pairwise
from pathlib import Path as File
from typing import Any

import numpy as np
import pytest
from scipy import ndimage

from bullterrier_image import RecipeError
from bullterrier_image.core.params import EffectParams, FieldName, param
from bullterrier_image.core.registry import effect
from bullterrier_image.recipe import (
    Band,
    Bands,
    Combine,
    Curve,
    Cuts,
    Edge,
    Finder,
    Grid,
    Line,
    LinearGradient,
    Path,
    Pick,
    RadialGradient,
    Recipe,
    Shards,
    Step,
    Subject,
    Svg,
    render,
)

Shape = tuple[int, int]
MOSAIC = str(File(__file__).resolve().parents[2] / "examples" / "assets" / "mosaic.svg")
BLOB = (
    "M 0.08 0.42 C 0.1 0.25, 0.32 0.22, 0.38 0.38 C 0.44 0.52, 0.36 0.74, 0.22 0.72 C 0.08 0.7, 0.06 0.56, 0.08 0.42 Z"
)
WAVE = "M 0.38 0 C 0.15 0.45, 0.85 0.55, 0.62 1"
# Sizes no cell divides: a single pixel, a single row or column, and two odd rectangles (as the effect contract uses).
ODD_SIZES = [(1, 1), (1, 7), (7, 1), (13, 29), (61, 37)]
PIECE_KINDS = (Grid, Shards, Cuts, Svg, Bands)  # the kinds that make a partition; every other kind a selection


def disc(shape: Shape) -> np.ndarray:
    """A solid disc in the middle of the picture, 0.3 of the long side across the radius (at least the middle pixel)."""
    height, width = shape
    y, x = np.mgrid[0:height, 0:width] + 0.5
    return (np.hypot(x - width / 2, y - height / 2) <= 0.3 * max(shape)).astype(np.float32)


def ramp(shape: Shape) -> np.ndarray:
    """A depth map for a picture of `shape` (the depth model's stand-in): nearest at the top left, farthest at the
    bottom right, falling evenly along both sides, so every kind of picture holds near and far pixels."""
    height, width = shape
    y, x = np.mgrid[0:height, 0:width] + 0.5
    return (1 - (y / height + x / width) / 2).astype(np.float32)


def declared() -> dict[str, Any]:
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
        "cut": Cuts(lines=[Line(x=0.5, y=0.5, angle=298)]),
        "crossing_cuts": Cuts(lines=[Line(angle=330), Line(x=0.3, y=0.6, angle=240)]),
        "curve": Cuts(lines=[Curve(path=WAVE)]),
        "torn_cut": Cuts(lines=[Line(angle=276)], edge=Edge(style="torn")),
        "cuts_on_subject": Cuts(lines=[Line(angle=350), Curve(path=WAVE)], fit="subject"),
        "blob": Path(d=BLOB),
        "torn_blob": Path(d=BLOB, edge=Edge(style="torn")),
        "blob_on_subject": Path(d=BLOB, fit="subject"),
        "mosaic": Svg(file=MOSAIC),
        "torn_mosaic": Svg(file=MOSAIC, edge=Edge(style="torn")),
        "mosaic_on_subject": Svg(file=MOSAIC, fit="subject"),
        "pick_of_mosaic": Pick(of="mosaic", pieces=["ring", "sun", 0]),
        "near": Band(of="depth"),
        "hard_band": Band(of="depth", low=0.4, high=0.6, feather=0),
        "grid_on_near": Grid(columns=2, rows=2, fit="near"),
        "pick_of_grid_on_near": Pick(of="grid_on_near", pieces=[1]),
        "bands": Bands(of="depth"),
        "many_bands": Bands(of="depth", count=100),
        "pick_of_bands": Pick(of="bands", pieces=[0]),
        "tilt": LinearGradient(),
        "spot": RadialGradient(),
        "near_and_tilted": Combine(of=("near", "tilt")),
        "near_or_spot": Combine(op="max", of=("near", "spot")),
        "bands_of_the_tilt": Bands(of="tilt", count=4),
        "band_of_a_band": Band(of="near", low=0.3, high=0.9),
    }


class ProbeParams(EffectParams):
    """The one parameter of the probes that show a region."""

    amount: float = param(0.5, unit="fraction", min=0, max=1, doc="Unused: a probe ignores it.")


class FieldProbeParams(ProbeParams):
    """The parameters of the probe that shows a selection, and the selection it shows."""

    field: FieldName = param("depth", doc="The selection painted.")


@pytest.fixture
def shown(throwaway_effects):
    """`shown(regions, name, shape)`: the region `name` of `regions` as the steps of a recipe are given it, over a
    picture of `shape` that the subject (a disc) and the depth (a ramp) are given for: the labels of a partition (int32)
    or the values of a selection (float32). Pieces found by pieces are found by a finder that sees all of each piece."""

    @effect(category="utilities", params=ProbeParams)
    def piece_probe(rgb, params, *, partition=None):
        """Paints the piece number of each pixel in base 256 over the three channels."""
        assert partition is not None
        digits = [(partition >> shift) & 255 for shift in (0, 8, 16)]
        return (np.stack(digits, axis=-1) / 255).astype(np.float32)

    @effect(category="utilities", params=FieldProbeParams)
    def selection_probe(rgb, params, *, field):
        """Paints the selection it is given."""
        return np.repeat(field[..., None], 3, axis=2)

    def show(regions: dict, name: str, shape: Shape) -> np.ndarray:
        by_piece = isinstance(regions[name], PIECE_KINDS)
        step = Step("piece_probe", partition=name) if by_piece else Step("selection_probe", params={"field": name})
        recipe = Recipe([step], name="shown", size=None, regions=regions)
        maps = {"subject": disc(shape), "depth": ramp(shape)}
        everything = Finder("sees all", lambda crop: np.ones(crop.shape[:2], np.float32))
        picture = render(np.zeros((*shape, 3), np.float32), recipe, maps=maps, finders={"subject": everything})
        if by_piece:
            digits = [np.rint(picture[..., channel] * 255).astype(np.int32) for channel in range(3)]
            return digits[0] + (digits[1] << 8) + (digits[2] << 16)
        return picture[..., 0]

    return show


def first_pixels(labels: np.ndarray) -> list[int]:
    """Where in the raster scan (left to right along each row, rows top to bottom) each piece is first met, by piece."""
    flat = labels.ravel()
    return [int(np.flatnonzero(flat == piece)[0]) for piece in range(int(flat.max()) + 1)]


def assert_a_region(region: np.ndarray, shape: Shape, nearest_first: np.ndarray | None = None) -> None:
    """The promises of both kinds: a partition (int32) numbers the pieces that exist 0..n-1 in reading order (or, given
    the depth as `nearest_first`, by depth: every pixel of a piece at least as near as every pixel of the pieces after
    it, as `Bands` numbers them) and gives every pixel one; a selection (float32) holds floats from 0 to 1."""
    assert region.shape == shape
    if region.dtype == np.int32:
        pieces = int(region.max()) + 1
        np.testing.assert_array_equal(np.unique(region), np.arange(pieces))
        if nearest_first is None:
            assert first_pixels(region) == sorted(first_pixels(region))
        else:
            depths = [nearest_first[region == piece] for piece in range(pieces)]
            assert all(nearer.min() >= farther.max() for nearer, farther in pairwise(depths))
    else:
        assert region.min() >= 0
        assert region.max() <= 1


def numbering(kind: object, shape: Shape) -> np.ndarray | None:
    """What `assert_a_region` checks the pieces of `kind` over a picture of `shape` are numbered by: the depth for
    `Bands` of depth (nearest first), None (reading order) for every other kind."""
    return ramp(shape) if isinstance(kind, Bands) and kind.of == "depth" else None


def same_pieces(labels: np.ndarray, other: np.ndarray) -> np.ndarray:
    """Where `other` gives a pixel the piece that `labels` does, matching pieces by their overlap since the two may
    number them differently: each piece of `labels` is taken to be the piece of `other` it covers most."""
    pairs = np.bincount(
        labels.ravel() * (other.max() + 1) + other.ravel(), minlength=(labels.max() + 1) * (other.max() + 1)
    )
    match = pairs.reshape(labels.max() + 1, other.max() + 1).argmax(axis=1)
    return match[labels] == other


def border_length(labels: np.ndarray) -> int:
    """How many pixel sides lie between two pieces."""
    return int((labels[1:] != labels[:-1]).sum() + (labels[:, 1:] != labels[:, :-1]).sum())


# ---- every kind: the promises of a region -------------------------------------------------------------------------


@pytest.mark.parametrize("size", ODD_SIZES, ids=lambda s: f"{s[0]}x{s[1]}")
@pytest.mark.parametrize("name", list(declared()))
def test_every_kind_gives_a_partition_or_selection_at_any_size_and_the_same_one_every_time(name, size, shown):
    first = shown(declared(), name, size)
    again = shown(declared(), name, size)
    assert_a_region(first, size, numbering(declared()[name], size))
    np.testing.assert_array_equal(first, again)


def extremes(model: type, **fixed: object) -> list[tuple[str, dict]]:
    """Every numeric field of `model` at its documented minimum and at its maximum, as (id, values)."""
    return [
        (f"{doc.name}={value}", {**fixed, doc.name: value})
        for doc in model.fields_doc()  # type: ignore[attr-defined]
        if doc.minimum is not None
        for value in (doc.minimum, doc.maximum)
    ]


AT_THE_LIMITS: dict[str, Any] = {
    **{f"shards {id_}": Shards(**values) for id_, values in extremes(Shards)},
    **{f"edge {id_}": Shards(edge=Edge(style="torn", **values)) for id_, values in extremes(Edge)},
    **{f"line {id_}": Cuts(lines=[Line(**values)]) for id_, values in extremes(Line)},
    **{f"path {id_}": Path(d=BLOB, **values) for id_, values in extremes(Path)},
    "cuts of most": Cuts(lines=[Line(x=i / 31, y=1 - i / 31, angle=11 * i) for i in range(32)]),
    "a cut torn at the limit": Cuts(lines=[Curve(path=WAVE)], edge=Edge(style="torn", amount=48, scale=480)),
    "a path torn at the limit": Path(d=BLOB, edge=Edge(style="torn", amount=48, scale=4.8)),
    "shards impact at one corner": Shards(impact=(0, 0)),
    "shards impact at the other": Shards(impact=(1, 1)),
    "grid of one": Grid(columns=1, rows=1),
    "grid of most": Grid(columns=100, rows=100),
    "grid of most columns": Grid(columns=1000, rows=10),
    "grid of most rows": Grid(columns=10, rows=1000),
    "grid of one share": Grid(columns=[1], rows=[1]),
    "grid of tiny and huge shares": Grid(columns=[0.001, 1000], rows=[1000, 0.001]),
    "grid on the subject, torn": Grid(columns=3, rows=3, fit="subject", edge=Edge(style="torn", amount=48)),
    **{f"subject {id_}": Subject(per="panes", **values) for id_, values in extremes(Subject)},
    **{f"band {id_}": Band(**values) for id_, values in extremes(Band, of="depth", low=0)},
    **{f"bands {id_}": Bands(**values) for id_, values in extremes(Bands, of="depth")},
    **{f"linear gradient {id_}": LinearGradient(**values) for id_, values in extremes(LinearGradient)},
    **{f"radial gradient {id_}": RadialGradient(**values) for id_, values in extremes(RadialGradient)},
    "a band of nothing between": Band(of="depth", low=0.5, high=0.5, feather=0),
}


@pytest.mark.parametrize("size", [(13, 29), (61, 37), (1, 1)], ids=lambda s: f"{s[0]}x{s[1]}")
@pytest.mark.parametrize("name", AT_THE_LIMITS)
def test_every_field_works_at_its_minimum_and_maximum(name, size, shown):
    regions = {"panes": Grid(columns=3, rows=2), "it": AT_THE_LIMITS[name]}
    assert_a_region(shown(regions, "it", size), size, numbering(AT_THE_LIMITS[name], size))


@pytest.mark.parametrize(
    "kind",
    [
        Grid(columns=3, rows=4),
        Grid(columns=[1, 2], rows=[3, 1, 1]),
        Shards(preset="glass"),
        Shards(preset="mosaic"),
        Grid(columns=3, rows=4, edge=Edge(style="torn", amount=38.4, scale=144, seed=3)),
        Shards(count=12, edge=Edge(style="torn", amount=38.4, scale=144, seed=5)),
        Shards(preset="torn"),
        Shards(preset="stone"),
        Cuts(lines=[Line(angle=330), Line(x=0.3, y=0.6, angle=240)]),
        Cuts(lines=[Curve(path=WAVE)]),
        Svg(file=MOSAIC),
    ],
    ids=[
        "grid",
        "uneven grid",
        "glass",
        "mosaic",
        "torn grid",
        "wide torn shards",
        "torn",
        "stone",
        "cuts",
        "curve",
        "svg",
    ],
)
def test_the_same_pieces_come_out_at_twice_the_resolution(kind, shown):
    """Positions are shares of the frame, so the sites sit at the same share. A coarse pixel's centre is the centre of
    a 2 x 2 block of fine pixels, so its piece must be one of the four the block holds, unless a piece is thinner than
    a pixel there: at most 1 coarse pixel in 100 may fail that. A piece smaller than a coarse pixel may have no pixel
    there, so the coarse picture may have fewer pieces, never more. A torn edge bends the positions by noise defined
    in design px, scaled to the picture, so it is the same tear: it differs only where the coarse picture leaves out a
    layer finer than its pixel, which moves a border by a fraction of a pixel. A different tear (the wide ones here, 38
    design px, 7 px on the coarse picture) fails it."""
    coarse = shown({"k": kind}, "k", (60, 90))
    fine = shown({"k": kind}, "k", (120, 180))
    assert coarse.max() <= fine.max()
    blocks = np.stack([fine[a::2, b::2] for a in (0, 1) for b in (0, 1)])  # the four fine pixels of each coarse one
    # which fine piece each coarse piece is: the one most often found in its blocks
    votes = np.zeros((coarse.max() + 1, fine.max() + 1), int)
    for block in blocks:
        np.add.at(votes, (coarse.ravel(), block.ravel()), 1)
    match = votes.argmax(axis=1)
    roomy = np.bincount(coarse.ravel()) >= 4  # a piece thinner than a pixel may be seen as its neighbour
    assert len(set(match[roomy])) == roomy.sum()
    found = (blocks == match[coarse]).any(axis=0)
    assert (~found).mean() < 0.01


@pytest.mark.parametrize("amount", [4.8, 24], ids=["a small tear", "a wide tear"])  # design px
@pytest.mark.parametrize(
    "straight",
    [
        Grid(columns=3, rows=3),
        Shards(count=12),
        Cuts(lines=[Line(angle=276)]),
        Cuts(lines=[Curve(path=WAVE)]),
        Svg(file=MOSAIC),
    ],
    ids=["grid", "shards", "cut", "curve", "svg"],
)
def test_a_torn_edge_moves_the_borders_by_about_its_amount_and_nothing_else(straight, amount, shown):
    """The coordinates are shifted by noise of unit standard deviation times `amount` (design px, here scaled to the
    200 px long side), so a pixel
    may change piece only if a straight border lies within a few standard deviations: 6, as the noise is smooth and a
    value that far out has a chance below 1e-8 of showing in a picture of this size."""
    shape = (150, 200)
    torn = straight.model_copy(update={"edge": Edge(style="torn", amount=amount, scale=144, seed=4)})
    before = shown({"k": straight}, "k", shape)
    after = shown({"k": torn}, "k", shape)
    assert after.max() == before.max()
    on_border = np.zeros(shape, bool)
    on_border[1:] |= before[1:] != before[:-1]
    on_border[:, 1:] |= before[:, 1:] != before[:, :-1]
    from_border = ndimage.distance_transform_edt(~on_border)
    moved = ~same_pieces(before, after)
    assert moved.any()  # it tears at all
    assert from_border[moved].max() <= 6 * amount * 200 / 480 + 1


@pytest.mark.parametrize(
    ("smoother", "rougher"),
    [
        pytest.param({"persistence": 0.0}, {"persistence": 1.0}, id="persistence"),
        pytest.param({"octaves": 1}, {"octaves": 8}, id="octaves"),
        pytest.param({"scale": 240}, {"scale": 9.6}, id="scale"),
    ],
)
def test_a_rougher_tear_has_longer_borders(smoother, rougher, shown):
    """Raising `persistence` or `octaves` adds fine layers to the wander, lowering `scale` makes it finer."""
    shape = (240, 320)

    def tear(**fields: float) -> Grid:
        return Grid(columns=2, rows=2, edge=Edge(style="torn", amount=9.6, **{"scale": 48, **fields}))

    assert border_length(shown({"k": tear(**smoother)}, "k", shape)) < border_length(
        shown({"k": tear(**rougher)}, "k", shape)
    )


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
    "fit names bands": ({"b": Bands(of="depth"), "g": Grid(fit="b")}, "'g'.*fit.*'b'.*partition.*selection"),
    "pick of a band": ({"n": Band(of="depth"), "p": Pick(of="n")}, "'p'.*'n'.*selection.*partition"),
    "a band of nothing named": ({"n": Band()}, "'n'.*of.*''.*no region"),
    "a band of a partition": ({"g": Grid(), "n": Band(of="g")}, "'n'.*of.*'g'.*partition.*selection"),
    "bands of a region that is not there": ({"b": Bands(of="gone")}, "'b'.*of.*'gone'.*no region.*valid.*depth"),
    "a combination of nothing named": ({"c": Combine()}, "'c'.*of.*''.*no region"),
    "a combination with a partition": ({"g": Grid(), "c": Combine(of=("depth", "g"))}, "'c'.*of.*'g'.*partition"),
    "a band of itself": ({"n": Band(of="n")}, "cycle.*'n'.*'n'"),
    "a combination through a band": (
        {"c": Combine(of=("depth", "n")), "n": Band(of="c")},
        "cycle.*'c'.*'n'",
    ),
    "a name taken by the built-in light": ({"light": Grid()}, "'light'.*built-in"),
}


@pytest.mark.parametrize(("regions", "complaint"), BAD_DECLARATIONS.values(), ids=BAD_DECLARATIONS.keys())
def test_a_declaration_that_refers_wrongly_is_refused_naming_the_region_and_what_is_valid(
    regions, complaint, photo, depth_probe
):
    with pytest.raises(RecipeError, match=complaint):
        render(
            photo,
            Recipe([Step("depth_probe")], name="refers wrongly", regions=regions),
            maps={"depth": ramp(photo.shape[:2])},
        )


def test_the_references_are_checked_even_for_regions_no_step_will_ask_for(photo, depth_probe):
    with pytest.raises(RecipeError, match="unused"):
        render(photo, Recipe([Step("depth_probe")], regions={"g": Grid(), "unused": Pick(of="missing")}))


def test_laying_a_kind_over_a_selection_that_chooses_nothing_is_refused_naming_the_region(shown):
    nothing = Band(of="depth", low=1.0, high=1.0, feather=0)  # the ramp is nowhere as near as 1
    with pytest.raises(RecipeError, match=r"'s'.*empty.*nothing"):
        shown({"empty": nothing, "s": Shards(fit="empty")}, "s", (20, 30))


def test_pick_selects_exactly_the_pixels_of_the_pieces_listed_and_what_exists_when_there_are_fewer(shown):
    regions = {"g": Grid(columns=3, rows=2), "p": Pick(of="g", pieces=[1, 4])}
    pieces = shown(regions, "g", (20, 30))
    picked = shown(regions, "p", (20, 30))
    np.testing.assert_array_equal(picked, np.isin(pieces, [1, 4]).astype(np.float32))
    assert picked.sum() == 2 * 10 * 10
    thumbnail = {"g": Grid(columns=3, rows=2), "p": Pick(of="g", pieces=[0, 5])}  # a 1 x 1 picture has one piece
    assert shown(thumbnail, "p", (1, 1)).tolist() == [[1.0]]


# ---- the models ----------------------------------------------------------------------------------------------------

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
    "an edge amount that is too large": (Shards, {"edge": {"amount": 240}}, "amount"),
    "an edge with a misspelt field": (Grid, {"edge": {"stlye": "torn"}}, "stlye"),
    "no pieces to pick": (Pick, {"pieces": []}, "pieces"),
    "a negative piece": (Pick, {"pieces": [0, -1]}, "pieces"),
    "context past 1": (Subject, {"context": 1.5}, "context"),
    "negative context": (Subject, {"context": -0.1}, "context"),
    "threshold past 1": (Subject, {"threshold": 1.1}, "threshold"),
    "negative threshold": (Subject, {"threshold": -0.1}, "threshold"),
    "a per that is no text": (Subject, {"per": 3}, "per"),
    "low above high": (Band, {"low": 0.6, "high": 0.3}, r"low.*0\.6.*high.*0\.3"),
    "the default low above a high below it": (Band, {"high": 0.2}, r"low.*0\.5.*high.*0\.2"),
    "high past 1": (Band, {"high": 1.5}, "high"),
    "a negative low": (Band, {"low": -0.1}, "low"),
    "feather past 1": (Band, {"feather": 1.1}, "feather"),
    "a misspelt band field": (Band, {"nearest": 1}, "nearest"),
    "an of that is no text": (Band, {"of": 3}, "of"),
    "no bands": (Bands, {"count": 0}, "count"),
    "too many bands": (Bands, {"count": 101}, "count"),
    "a share of a band": (Bands, {"count": 2.5}, "count"),
    "a band count named by its kind as a recipe writes it": (Bands, {"count": 0}, "^bands: .*count"),
    "one selection combined": (Combine, {"of": ("depth",)}, "of"),
    "an operation that is not offered": (Combine, {"op": "xor"}, "op"),
    "a combination named by its kind as a recipe writes it": (Combine, {"op": "xor"}, "^combine: .*op"),
    "an angle that is not a number": (LinearGradient, {"angle": "up"}, "angle"),
    "a gradient named by its kind as a recipe writes it": (
        LinearGradient,
        {"angle": "up"},
        "^linear_gradient: .*angle",
    ),
    "a centre off the picture": (RadialGradient, {"x": 1.1}, "x"),
    "a radius of nothing": (RadialGradient, {"radius": 0}, "radius"),
    "a radius past 3": (RadialGradient, {"radius": 3.5}, "radius"),
    "a radial gradient named by its kind as a recipe writes it": (
        RadialGradient,
        {"radius": 0},
        "^radial_gradient: .*radius",
    ),
}


@pytest.mark.parametrize(("model", "values", "complaint"), BAD_VALUES.values(), ids=BAD_VALUES.keys())
def test_a_bad_value_is_refused_naming_the_field(model, values, complaint):
    with pytest.raises(RecipeError, match=complaint):
        model(**values)


def test_values_written_as_a_recipe_file_writes_them_are_accepted():
    """TOML gives lists and tables, numpy arrays come from scripts: both make the same model."""
    from_file = Grid(columns=[1, 2], rows=[1, 1, 2], edge={"style": "torn", "amount": 9.6})
    from_script = Grid(columns=np.array([1, 2]), rows=(1, 1, 2), edge=Edge(style="torn", amount=9.6))
    assert from_file == from_script
    assert Shards(impact=[0.2, 0.8]).impact == (0.2, 0.8)
    assert Pick(pieces=np.array([2, 3])).pieces == (2, 3)


def test_a_preset_sets_the_fields_and_a_field_given_beside_it_wins():
    glass = Shards(preset="glass")
    assert glass.focus > 0  # a preset's own value
    assert Shards(preset="glass", focus=0).focus == 0
    assert Shards(preset="glass", count=5).count == 5
    assert Shards(preset="glass", count=5).focus == glass.focus


@pytest.mark.parametrize(
    ("d", "complaint"),
    [("", "empty"), ("L 1 1", "start with M"), ("M 0 0 L x", "invalid number"), ("M 0 0 Q", "valid")],
    ids=["empty", "no start", "not a number", "missing numbers"],
)
def test_a_path_that_is_not_path_data_is_refused_when_the_recipe_is_made_naming_the_region_and_field(d, complaint):
    with pytest.raises(RecipeError, match=rf"region 'blob'.*`d`.*{complaint}"):
        Recipe([Step("grade")], regions={"blob": Path(d=d)})


def test_a_cut_of_two_subpaths_is_refused_when_the_recipe_is_made_naming_the_region_and_field():
    cut = Cuts(lines=[{"path": "M 0 0 L 1 1 M 0 1 L 1 0"}])
    with pytest.raises(RecipeError, match=r"region 'cut'.*`lines\[0\].path`.*single M"):
        Recipe([Step("grade")], regions={"cut": cut})


@pytest.mark.parametrize(
    "lines",
    [[{"x": 0.5, "path": "M 0 0 L 1 1"}], [{"x": 2}], [{"angle": "up"}], []],
    ids=[
        "a line and a curve at once",
        "a position outside the frame",
        "an angle that is not a number",
        "no lines",
    ],
)
def test_cuts_that_are_not_cuts_are_refused_when_declared(lines):
    with pytest.raises(RecipeError, match=r"lines|path|x|angle"):
        Cuts(lines=lines)


def test_values_written_as_a_recipe_file_writes_the_cuts_are_the_same_as_the_models():
    from_file = Cuts(lines=[{"x": 0.2, "y": 0.4, "angle": 330}, {"path": WAVE}], edge={"style": "torn"})
    assert from_file == Cuts(lines=[Line(x=0.2, y=0.4, angle=330), Curve(path=WAVE)], edge=Edge(style="torn"))


@pytest.mark.parametrize("preset", ["glass", "torn", "stone", "mosaic"])
def test_every_shipped_preset_cuts_a_picture_differently_from_the_default(preset, shown):
    shape = (96, 128)
    default = shown({"s": Shards()}, "s", shape)
    named = shown({"s": Shards(preset=preset)}, "s", shape)
    assert named.shape == default.shape
    assert not np.array_equal(named, default)


@pytest.mark.parametrize(
    "model",
    [
        Grid,
        Shards,
        Pick,
        Subject,
        Edge,
        Cuts,
        Line,
        Curve,
        Path,
        Svg,
        Band,
        Bands,
        Combine,
        LinearGradient,
        RadialGradient,
    ],
)
def test_every_field_is_documented_with_its_unit_and_range(model):
    text = model.documentation()
    for doc in model.fields_doc():
        assert doc.name in text
        assert doc.doc
        if doc.minimum is not None:
            assert doc.unit


# ---- svg: a file that is the pieces ----------------------------------------------------------------------------------

RING_AND_DISC = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
  <path id="ring" fill-rule="evenodd"
        d="M 10 50 a 40 40 0 1 0 80 0 a 40 40 0 1 0 -80 0 Z M 30 50 a 20 20 0 1 0 40 0 a 20 20 0 1 0 -40 0 Z"/>
  <circle id="inner" cx="50" cy="50" r="10"/>
  <rect id="corner" x="0" y="0" width="12" height="12"/>
  <circle id="inner-copy" cx="50" cy="50" r="5"/>
</svg>"""


@pytest.fixture
def svg_file(tmp_path):
    """`svg_file(text)`: the path of a file holding `text`."""

    def write(text: str = RING_AND_DISC) -> str:
        path = tmp_path / "shapes.svg"
        path.write_text(text, encoding="utf-8")
        return str(path)

    return write


def test_an_svg_that_cannot_be_read_is_refused_naming_the_file(tmp_path, shown):
    with pytest.raises(RecipeError, match=r"missing\.svg"):
        shown({"s": Svg(file=str(tmp_path / "missing.svg"))}, "s", (10, 10))
    with pytest.raises(RecipeError, match="file"):
        shown({"s": Svg()}, "s", (10, 10))


def test_the_limits_on_an_svg_are_fields_and_fail_naming_the_limit(svg_file, shown):
    with pytest.raises(RecipeError, match="max_shapes"):
        shown({"s": Svg(file=svg_file(), max_shapes=2)}, "s", (100, 100))
    with pytest.raises(RecipeError, match="max_kilobytes"):
        shown({"s": Svg(file=svg_file(RING_AND_DISC + " " * 4000), max_kilobytes=1)}, "s", (100, 100))
    assert shown({"s": Svg(file=svg_file(), max_shapes=4, max_kilobytes=1)}, "s", (100, 100)).max() + 1 == 5


@pytest.mark.parametrize(
    "fields", [{"max_shapes": 1}, {"max_shapes": 100000}, {"max_kilobytes": 1}, {"max_kilobytes": 100000}]
)
def test_the_svg_limits_work_at_their_minimum_and_maximum(svg_file, shown, fields):
    text = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="5" height="5"/></svg>'
    assert shown({"s": Svg(file=svg_file(text), **fields)}, "s", (100, 100)).max() + 1 == 2


def test_an_svg_gives_a_piece_for_each_shape_of_a_real_file_and_one_for_the_outside(shown):
    pieces = shown({"m": Svg(file=MOSAIC)}, "m", (240, 360))
    assert_a_region(pieces, (240, 360))
    assert pieces.max() + 1 > 5
