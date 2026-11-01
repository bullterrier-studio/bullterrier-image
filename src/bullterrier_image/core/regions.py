"""The declared regions of a recipe: which pixels of a picture a step is about, as a partition (`Grid`, `Shards`,
`Cuts`, `Svg`, `Bands`) or a selection (`Path`, `Pick`, `Subject`, `Band`, `Combine`, `LinearGradient`,
`RadialGradient`, `Given`); `Edge` bends the borders of the kinds with `fit`.

The kinds are models declared like an effect's parameters, so every field has a unit, a range and a description:
`Shards.documentation()` lists them. They are data: the core makes the regions of a picture from them.
Positions are shares of the *frame* (the picture, or the bounding box of the selection a kind is laid `fit` over); the
lengths of a torn `Edge` are design px.

A declaration is checked when it is made (a value out of range) and when the recipe is (an unknown name, a name of the
wrong kind of region, a cycle, a path that is no path): a `RecipeError` naming the region and what is valid.

Dependency direction: imports `params` and `presets`; the steps build on it.
"""

import math
import re
from collections.abc import Mapping
from typing import Annotated, Any, Literal

import numpy as np
from pydantic import BeforeValidator, field_validator, model_validator

from bullterrier_image.errors import RecipeError

from .params import EffectParams, param
from .presets import SHIPPED, read

__all__ = [
    "FOUND_MAPS",
    "Band",
    "Bands",
    "Combine",
    "Curve",
    "Cuts",
    "Edge",
    "Given",
    "Grid",
    "Kind",
    "Line",
    "LinearGradient",
    "Path",
    "Pick",
    "RadialGradient",
    "Shards",
    "Subject",
    "Svg",
    "check",
    "needs",
]

BUILT_IN = ("subject", "background", "depth", "light")
FOUND_MAPS = ("subject", "depth")  # the built-in maps found in the photo: handed in (`maps=`) or by a finder

_MOST = 1000  # the most columns, rows, or shares of a grid


class _Model(EffectParams):
    """What the declarations share: a bad value is a `RecipeError` naming the kind (as a recipe writes it,
    `linear_gradient` for `LinearGradient`), the field and the value."""

    def __init__(self, /, **values: Any) -> None:
        """The declaration of `values` (field name -> value, all optional); RecipeError naming each bad field, the
        constraint and the value, and listing the fields when an unknown one is given."""
        self._init_checked(re.sub(r"(?<!^)(?=[A-Z])", "_", type(self).__name__).lower(), **values)


class Edge(_Model):
    """How the borders between pieces run: straight, or torn like paper by bending the coordinates they cut."""

    style: Literal["straight", "torn"] = param(
        "straight",
        doc="'straight' cuts along the exact borders of the kind; 'torn' bends every border by smooth noise, so the "
        "pieces have ragged, wandering edges (the other edge fields only matter for 'torn').",
    )
    amount: float = param(
        4.8,
        unit="px",
        min=0,
        max=48,
        doc="How far a torn border strays, in px (0 to 48): the standard deviation of the shift. Raising it tears "
        "the edge wider; 0 is straight.",
    )
    scale: float = param(
        24.0,
        unit="px",
        min=1,
        max=480,
        doc="The size of the bends of a torn border, in px (1 to 480): the spacing of the coarsest noise. Raising it "
        "gives long gentle wanders, lowering it a jagged edge.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed of the tear: the same seed always tears the same way, another seed differently.",
    )
    octaves: int = param(
        6,
        unit="count",
        min=1,
        max=8,
        advanced=True,
        doc="How many layers of noise make the tear (1 to 8): each is twice as fine as the one before. Raising it "
        "adds fine ragged detail to the long wanders; an octave finer than a pixel is left out.",
    )
    persistence: float = param(
        0.55,
        unit="ratio",
        min=0,
        max=1,
        advanced=True,
        doc="How strong each finer layer of the tear is compared with the one before (0 to 1). Raising it makes the "
        "edge rougher; 0 leaves only the coarsest wander.",
    )


def _as_edge(value: object) -> object:
    """A table written in a recipe file as the `Edge` it describes (anything else is left for the field to judge)."""
    return Edge.parse(value) if isinstance(value, Mapping) else value


def _as_shares(value: object) -> object:
    """What a count-or-shares field is written as, as the tuple of shares it holds: a count N is N equal shares, a
    list or 1-d numpy array is its numbers."""
    if isinstance(value, int) and not isinstance(value, bool):
        return (1.0,) * max(value, 0)
    if isinstance(value, np.ndarray) and value.ndim == 1:
        return tuple(value.tolist())
    return tuple(value) if isinstance(value, list) else value


class _Laid(_Model):
    """A kind laid over a frame: the picture, or the box of a selection, with borders straight or torn."""

    fit: str | None = param(
        None,
        doc="The name of a selection (the built-in 'subject' or 'background', or a `Pick`): the kind is laid over that "
        "selection's bounding box instead of the whole picture, so its positions and lengths are shares of that box "
        "(object first: cut the subject, not the sky). Not set = the whole picture.",
    )
    edge: Annotated[Edge, BeforeValidator(_as_edge)] = param(
        Edge(),
        doc="How the borders between pieces run: an `Edge` (or its fields as a table: style, amount, scale, seed, "
        "octaves, persistence). The default is straight borders.",
    )


class Grid(_Laid):
    """A grid of rectangular pieces, any number of columns and rows, equal or in uneven shares."""

    columns: Annotated[tuple[float, ...], BeforeValidator(_as_shares)] = param(
        (1.0,),
        doc="How the frame is divided from left to right: a whole number (1 to 1000) of equal columns, or a list of 1 "
        "to 1000 positive numbers, the relative widths ([1, 2, 1] is a narrow, a wide and a narrow column); held as "
        "that list. A pixel belongs to the column whose range holds its centre; a centre exactly on a border goes to "
        "the later column. Beyond a `fit` box the outer columns run to the picture's edge.",
    )
    rows: Annotated[tuple[float, ...], BeforeValidator(_as_shares)] = param(
        (1.0,),
        doc="How the frame is divided from top to bottom: as `columns` (a whole number 1 to 1000, or a list of 1 to "
        "1000 positive relative heights). Pieces are numbered row after row.",
    )

    @field_validator("columns", "rows")
    @classmethod
    def _within_limits(cls, shares: tuple[float, ...]) -> tuple[float, ...]:
        """1 to 1000 positive finite shares."""
        if not 1 <= len(shares) <= _MOST or not all(0 < share < math.inf for share in shares):
            raise ValueError(f"must be a count or a list of positive numbers, 1 to {_MOST} of them, got {list(shares)}")
        return shares


class Shards(_Laid):
    """Irregular pieces like broken glass or stone: the cells round seeds scattered over the frame."""

    count: int = param(
        12,
        unit="count",
        min=1,
        max=500,
        doc="How many shards (1 to 500): exactly this many seeds are scattered, all inside the `fit` selection when "
        "there is one; a picture too small to give each seed a pixel has fewer pieces. Raising it makes smaller "
        "shards.",
    )
    jitter: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="How far each seed strays from its place in a regular honeycomb, as a share of the spacing (0 to 1): 0 "
        "cuts a regular honeycomb of equal pieces, 1 lets every seed anywhere in its own place, an uneven scatter.",
    )
    impact: tuple[float, float] = param(
        (0.5, 0.5),
        doc="Where the shards crowd to when `focus` is above 0, as (x, y) shares of the frame, each 0 to 1 (0, 0 is "
        "the top left corner; 0.5, 0.5 the middle).",
    )
    focus: float = param(
        0.0,
        unit="ratio",
        min=0,
        max=5,
        doc="How much the shards crowd round `impact` (0 to 5): a seed at a share r of the way to the farthest corner "
        "moves to r to the power 1 + focus of it, so 0 leaves them spread evenly and 2.5 makes small splinters at "
        "the impact point and big pieces far from it, like cracked glass.",
    )
    seed: int = param(
        0,
        unit="count",
        min=0,
        max=2**32 - 1,
        doc="Random seed for where the seeds lie: the same seed always cuts the same shards, another seed other ones.",
    )
    preset: str | None = param(
        None,
        doc="The name of a shipped set of these fields ('glass', 'torn', 'stone' or 'mosaic'): its values are used for "
        "every field not given beside it (a given `edge` replaces the preset's whole `edge`). Not set = the defaults.",
    )

    @model_validator(mode="before")
    @classmethod
    def _lay_preset_under(cls, values: Any) -> Any:
        """The values with the named preset's fields under them (the explicit ones win). Anything but a mapping with a
        preset name (a declaration of another kind, tried by a union) is left for the model itself to judge."""
        name = values.get("preset") if isinstance(values, dict) else None
        if not isinstance(name, str):
            return values
        found = read("shards", cls, SHIPPED / "regions")
        if name not in found:
            raise ValueError(f"no preset {name!r}; the presets are: {', '.join(found)}")
        return {**found[name].params, **values}

    @field_validator("impact")
    @classmethod
    def _inside_the_frame(cls, impact: tuple[float, float]) -> tuple[float, float]:
        """Both shares from 0 to 1."""
        if not all(0 <= share <= 1 for share in impact):
            raise ValueError(f"must be two shares of the frame, each 0 to 1, got {list(impact)}")
        return impact


class Pick(_Model):
    """A selection: the pixels of some pieces of a partition."""

    of: str = param(
        "",
        doc="The name of the partition (grid, shards, cuts, svg or bands) whose pieces are picked; it must be set.",
    )
    pieces: tuple[int | str, ...] = param(
        (0,),
        min_length=1,
        doc="The pieces to choose, at least one: numbers counted from 0 in piece order (reading order; bands "
        "highest first) or, for an `Svg` partition, the `id` of a shape (text; every piece of every shape with that "
        "id). A number the partition has no piece for chooses nothing, so a recipe also works on a picture too small "
        "to have that many pieces; an id the file lacks is an error.",
    )

    @field_validator("pieces")
    @classmethod
    def _numbers(cls, pieces: tuple[int | str, ...]) -> tuple[int | str, ...]:
        """Piece numbers count from 0."""
        if min((piece for piece in pieces if isinstance(piece, int)), default=0) < 0:
            raise ValueError(f"piece numbers count from 0, got {list(pieces)}")
        return pieces


_MOST_LINES = 32


class Line(_Model):
    """One straight cut: a line through a point at an angle, from border to border."""

    x: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="Where the line passes, across, as a share of the frame's width (0 to 1): 0 is the left edge, 1 the right.",
    )
    y: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="Where the line passes, down, as a share of the frame's height (0 to 1): 0 is the top edge, 1 the bottom.",
    )
    angle: float = param(
        0.0,
        unit="degrees",
        min=0,
        max=360,
        cyclic=True,
        doc="The direction of the line, in degrees clockwise from the right as seen on the picture (0 to 360; any "
        "other angle is the same direction inside that range): 0 and 180 run left to right, 90 and 270 straight up "
        "and down, 45 falls to the right, 315 rises to the right. A line has no head, so an angle and the one 180 "
        "further cut alike. The angle is measured on the picture's pixels, whatever the frame's shape.",
    )


class Curve(_Model):
    """One curved cut: an open SVG path from border to border."""

    path: str = param(
        "M 0 0 L 1 1",
        doc="An open SVG path (one M, then lines, Béziers or arcs: M L H V C S Q T A, absolute or relative) whose "
        "coordinates are shares of the frame, 0 to 1 across and down (the unit square is stretched to the frame). It "
        "should run from one border of the frame to another; an end that does not is carried straight on to the "
        "nearest border. Example: 'M 0.38 0 C 0.15 0.45, 0.85 0.55, 0.62 1'.",
    )


def _as_lines(value: object) -> object:
    """What a list of cuts is written as (tables with `x`, `y`, `angle` or with `path`), as the models they describe;
    anything else is left for the field to judge."""
    if not isinstance(value, list | tuple):
        return value
    return tuple(
        (Curve if "path" in each else Line).parse(each, "a cut") if isinstance(each, Mapping) else each
        for each in value
    )


class Cuts(_Laid):
    """Straight and curved cuts across the frame: the pieces are the regions the cuts divide it into."""

    lines: Annotated[tuple[Line | Curve, ...], BeforeValidator(_as_lines)] = param(
        (Line(),),
        min_length=1,
        max_length=_MOST_LINES,
        doc="The cuts, 1 to 32: each a `Line` (`x`, `y`, `angle`: straight, through that point) or a `Curve` (`path`: "
        "an open SVG path), from border to border of the frame. The pieces are the regions the cuts divide the frame "
        "into, numbered in reading order: n cuts that cross nowhere make n + 1 pieces, more cuts crossing give more.",
    )


class Path(_Laid):
    """A selection: everything inside a closed SVG path, softly edged."""

    d: str = param(
        "M 0 0 H 1 V 1 H 0 Z",
        doc="A closed SVG path (M L H V C S Q T A Z, absolute or relative) whose coordinates are shares of the "
        "frame, 0 to 1 across and down: the unit square is stretched to the frame, so an arc `A 0.25 0.25` is a "
        "circle only in a square frame. Each subpath is closed as filling does, and a pixel is inside by the nonzero "
        "rule (an inner outline drawn the other way round is a hole). Example: 'M 0.1 0.4 C 0.1 0.2, 0.4 0.2, 0.4 "
        "0.4 Z'.",
    )
    samples: int = param(
        4,
        unit="count",
        min=1,
        max=16,
        advanced=True,
        doc="How many evenly spaced scanlines measure how much of a border pixel the path covers (1 to 16): 1 looks at "
        "the middle of the pixel row only, more give a smoother slanting border and cost more time.",
    )


class Svg(_Laid):
    """Shapes from an SVG file: every shape is a piece, and the area outside all of them one more piece."""

    file: str = param(
        "",
        doc="The SVG file to read; it must be set. A relative path in a recipe file is relative to that file. The "
        "file is untrusted input: nothing in it is run or fetched. Each rect, circle, ellipse, polygon, polyline and "
        "path is a piece (colours, strokes and fill are ignored; the fill rule is kept, so holes are holes); later "
        "shapes lie on top of earlier ones; a `use` is drawn where it is used. The file's `viewBox` and "
        "`preserveAspectRatio` (by default fitted inside and centred) place it in the frame. Pieces are numbered in "
        "reading order like any partition, the area outside every shape included (piece 0 when the top left pixel is "
        "outside every shape); `Pick` chooses a shape by its `id`.",
    )
    max_shapes: int = param(
        1000,
        unit="count",
        min=1,
        max=100000,
        advanced=True,
        doc="The most shapes the file may hold, counting what each `use` repeats (1 to 100000); a file with more is "
        "refused, which guards against a small file that draws a huge number of shapes.",
    )
    max_kilobytes: int = param(
        1024,
        unit="count",
        min=1,
        max=100000,
        advanced=True,
        doc="The largest file to read, in kilobytes (1 to 100000); a bigger one is refused, reading no more than that.",
    )


class Subject(_Model):
    """A selection: the subject of the picture, or, with `per`, the subject found inside each piece of a partition
    (grid first). Without `per` it is the built-in `subject`, and the other fields do nothing."""

    per: str | None = param(
        None,
        doc="The name of a partition (grid, shards, cuts, svg or bands): the subject is found inside each of "
        "its pieces separately and the finds are united, so small things a whole-picture search misses (a calf "
        "behind its mother, a fox on a rock) are found. Not set = the subject of the whole picture, as the built-in "
        "'subject'.",
    )
    context: float = param(
        0.25,
        unit="fraction",
        min=0,
        max=1,
        doc="How much of its surroundings the finder sees round a piece, as a share of the piece's own size added on "
        "each side (0 to 1); the view is clipped to the picture. Only the piece's own pixels count. Raising it shows "
        "the finder more context, so a subject cut by the piece's border is recognised; 0 shows the bare piece. "
        "Only matters with `per`.",
    )
    threshold: float = param(
        0.9,
        unit="fraction",
        min=0,
        max=1,
        doc="How sure the finder must be of a pixel for it to count as subject (0 to 1): a pixel it gives a value of "
        "this or less is dropped. Raising it keeps only what the finder is certain of and drops the faint subjects it "
        "invents in a plain piece, but also the uncertain parts of a real one; 0 keeps every pixel the finder gives. "
        "Only matters with `per`.",
    )


class Band(_Model):
    """A selection: the pixels where another selection (a field of the photo) lies between two values, fading out
    linearly beyond them. The values are shares of the field's own scale (0 to 1)."""

    of: str = param(
        "",
        doc="The name of the selection whose values are cut, which must be set: a built-in one (`depth`: 1 is the "
        "nearest; `light`: the picture's brightness, 1 is white; `subject`) or one the recipe declares (a `given` map, "
        "a gradient, another band). Its value at a pixel decides how far the pixel is chosen.",
    )
    low: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="The lowest value chosen fully (0 to 1, at most `high`): every pixel whose value lies from `low` to `high` "
        "is chosen. Raising it leaves out the lower values; 0 reaches down to the bottom of the scale.",
    )
    high: float = param(
        1.0,
        unit="fraction",
        min=0,
        max=1,
        doc="The highest value chosen fully (0 to 1): see `low`. Lowering it leaves out the higher values; 1 reaches "
        "up to the top of the scale.",
    )
    feather: float = param(
        0.1,
        unit="fraction",
        min=0,
        max=1,
        doc="How far beyond `low` and `high` the choice fades out, as a share of the scale (0 to 1): it falls linearly "
        "from 1 at the bound to 0 this far past it. Raising it softens the edge; 0 cuts it hard.",
    )

    @model_validator(mode="after")
    def _low_not_above_high(self) -> "Band":
        """`low` is no higher than `high`."""
        if self.low > self.high:
            raise ValueError(f"`low` {self.low} is above `high` {self.high}: low must be at most high")
        return self


class Bands(_Model):
    """A partition into bands of a field: equal ranges of its scale, numbered highest first."""

    of: str = param(
        "",
        doc="The name of the selection whose values are cut into bands, which must be set (see `Band.of`). Band 0 "
        "holds its highest values: the nearest for `depth`, the brightest for `light`.",
    )
    count: int = param(
        3,
        unit="count",
        min=1,
        max=100,
        doc="How many equal ranges the scale is cut into (1 to 100). Band 0 is the highest, the last the lowest: by "
        "value, not in reading order. A value on a border goes to the lower band (0 to the lowest); a band no pixel "
        "lies in has no piece and the rest are renumbered, highest first. Raising it makes thinner bands.",
    )


class Combine(_Model):
    """A selection made of other selections: where all of them are chosen (multiply, min) or any of them is (max)."""

    op: Literal["multiply", "min", "max"] = param(
        "multiply",
        doc="How the selections are put together at each pixel: 'multiply' (their product: all of them, fading where "
        "any fades), 'min' (the least of them: all of them, hard where any is hard) or 'max' (the greatest: any of "
        "them).",
    )
    of: tuple[str, ...] = param(
        ("", ""),
        min_length=2,
        doc="The names of the selections to combine, at least two, which must be set: built-in ones (`subject`, "
        "`background`, `depth`, `light`) or ones the recipe declares.",
    )


class LinearGradient(_Model):
    """A selection that rises evenly across the picture in one direction: 0 at one edge, 1 at the opposite one."""

    angle: float = param(
        270.0,
        unit="degrees",
        min=0,
        max=360,
        cyclic=True,
        doc="The direction in which the value rises, clockwise from the right (0 to 360): 0 rises to the "
        "right, 90 downward, 180 to the left, 270 upward. The value is a share of the picture's extent along that "
        "direction, so the far corners are 0 and 1 at any aspect ratio.",
    )


class RadialGradient(_Model):
    """A selection that rises evenly from a point: 0 at the centre, 1 at `radius` and beyond."""

    x: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="Where the centre is across the picture, as a share of its width (0 to 1): 0 is the left edge, 1 the "
        "right.",
    )
    y: float = param(
        0.5,
        unit="fraction",
        min=0,
        max=1,
        doc="Where the centre is down the picture, as a share of its height (0 to 1): 0 is the top edge, 1 the bottom.",
    )
    radius: float = param(
        0.5,
        unit="fraction",
        min=0.01,
        max=3,
        doc="The distance from the centre at which the value reaches 1, as a share of the picture's shorter side "
        "(0.01 to 3). Raising it spreads the gradient wider; 0.5, centred, reaches the midpoints of the shorter sides' "
        "edges.",
    )


class Given(_Model):
    """A selection: a map of the photo that the caller hands in at render time under this region's own name (`maps=
    {"star": alpha}`, `--map star=star.png`, or found by a finder of that name): float32 `(height, width)`, 0..1, the
    photo's size. It moves with the photo through the recipe's framing (a turn or a border leaves 0 where the picture
    is not). It has no fields."""


Kind = (
    Grid
    | Shards
    | Cuts
    | Path
    | Svg
    | Pick
    | Subject
    | Band
    | Bands
    | Combine
    | LinearGradient
    | RadialGradient
    | Given
)


def _type_of(name: str, declared: Mapping[str, Kind]) -> Literal["partition", "selection"] | None:
    """What the region `name` is, or None when no such region is declared or built in."""
    if name in BUILT_IN:
        return "selection"
    match declared.get(name):
        case Grid() | Shards() | Cuts() | Svg() | Bands():
            return "partition"
        case Pick() | Path() | Subject() | Band() | Combine() | LinearGradient() | RadialGradient() | Given():
            return "selection"
        case _:
            return None


def needs(
    owner: str,
    field: str,
    name: str,
    wanted: Literal["partition", "selection"] | None,
    declared: Mapping[str, Kind],
) -> None:
    """RecipeError unless `name` (what `owner`'s `field` refers to, say "region 'glass'" or "step 2") is a region of
    the `wanted` kind, or any region when `wanted` is None. The message names the owner, field and name and lists the
    valid names (the built-in ones and those in `declared`)."""
    have = _type_of(name, declared)
    if have is not None and wanted in (None, have):
        return
    kind = wanted or "region"
    valid = (
        ", ".join(other for other in (*BUILT_IN, *declared) if wanted in (None, _type_of(other, declared))) or "none"
    )
    problem = "which is no region" if have is None else f"a {have}, but it needs a {wanted}"
    hint = (
        "" if declared else " (this recipe declares no regions of its own: add them with `regions=` or [region.NAME])"
    )
    raise RecipeError(f"{owner}: `{field}` names {name!r}, {problem}; valid {kind}s: {valid}{hint}")


def _references(kind: Kind) -> tuple[str, ...]:
    """The names of the regions that `kind` is made from."""
    match kind:
        case Pick() | Band() | Bands():
            return (kind.of,)
        case Combine():
            return kind.of
        case Subject():
            return () if kind.per is None else (kind.per,)
        case Given() | LinearGradient() | RadialGradient():
            return ()
        case _:
            return () if kind.fit is None else (kind.fit,)


def check(declared: Mapping[str, Kind]) -> None:
    """RecipeError unless the declared regions are consistent: a `fit` names a selection, a `Pick`'s `of` a partition, a
    `Band`'s, `Bands`' or `Combine`'s `of` a selection, no name is one of the built-ins and regions do not refer to each
    other in a cycle. Builds nothing, needs no picture; the
    message names the region and lists the valid names."""
    for name, kind in declared.items():
        if name in BUILT_IN:
            raise RecipeError(f"region {name!r} is a built-in region ({', '.join(BUILT_IN)}) and cannot be declared")
        if isinstance(kind, Pick):
            needs(f"region {name!r}", "of", kind.of, "partition", declared)
        elif isinstance(kind, Band | Bands):
            needs(f"region {name!r}", "of", kind.of, "selection", declared)
        elif isinstance(kind, Combine):
            for of in kind.of:
                needs(f"region {name!r}", "of", of, "selection", declared)
        elif isinstance(kind, Subject):
            if kind.per is not None:
                needs(f"region {name!r}", "per", kind.per, "partition", declared)
        elif isinstance(kind, _Laid) and kind.fit is not None:
            needs(f"region {name!r}", "fit", kind.fit, "selection", declared)
    _no_cycle(declared)


def _no_cycle(declared: Mapping[str, Kind]) -> None:
    """RecipeError if declared regions refer to each other in a cycle. One walk that visits every region once (and
    keeps no Python stack), however many ways lead to a region and however long the chains."""
    done: set[str] = set()
    for start in declared:
        if start in done:
            continue
        path, walks = [start], [iter(_references(declared[start]))]
        while walks:
            for ref in walks[-1]:
                if ref in path:
                    loop = " -> ".join(repr(each) for each in [*path[path.index(ref) :], ref])
                    raise RecipeError(f"regions refer to each other in a cycle: {loop}")
                if ref in declared and ref not in done:
                    path.append(ref)
                    walks.append(iter(_references(declared[ref])))
                    break
            else:  # every reference of the last region on the path is checked
                done.add(path.pop())
                walks.pop()
