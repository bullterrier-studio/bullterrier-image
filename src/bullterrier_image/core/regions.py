"""The declared regions of a recipe: which pixels of a picture a step is about, as a partition (`Grid`, `Shards`,
`Cuts`, `Svg`, `Bands`, `Components`) or a selection (`Path`, `Pick`, `Subject`, `Band`, `Combine`, `Colour`,
`LinearGradient`, `RadialGradient`, `Given`); `Edge` bends the borders of the kinds with `fit`.

The kinds are pieces (see `pieces`): frozen dataclasses made by the core's answer, every field with a default and its
documentation (`help(Shards)` lists them). They are data: the core makes the regions of a picture from them.
Positions are shares of the *frame* (the picture, or the bounding box of the selection a kind is laid `fit` over); the
lengths of a torn `Edge` are design px.

A region is checked when it is made (a value out of range) and when the recipe is (an unknown name, a name of the
wrong kind of region, a cycle, a path that is no path): a `RecipeError` naming the region and what is valid.

Dependency direction: imports `pieces` and `native`; the steps build on it.
"""

import re
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass, field
from typing import Any, ClassVar, Literal, get_args

from bullterrier_image.errors import RecipeError

from . import native
from .pieces import _Piece, piece
from .types import Color as ColorForms

__all__ = [
    "FOUND_MAPS",
    "Band",
    "Bands",
    "Colour",
    "Combine",
    "Components",
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
]


FOUND_MAPS = ("subject", "depth")  # the built-in maps found in the photo: handed in (`maps=`) or by a finder


def _region(kind: str) -> Callable[[dict[str, Any]], dict[str, Any]]:
    """What makes a region of `kind`: the core's answer to its fields."""
    return lambda fields: native.region(kind, fields)


@piece(lambda fields: native.region("grid", {"edge": fields})["edge"])
@dataclass(frozen=True, kw_only=True)
class Edge(_Piece):
    """How the borders between pieces run: straight, or torn like paper by bending the coordinates they cut."""

    style: Literal["straight", "torn"] = field(
        default="straight",
        metadata={
            "doc": (
                "'straight' cuts along the exact borders of the kind; 'torn' bends every border by "
                "smooth noise, so the pieces have ragged, wandering edges (the other edge fields only "
                "matter for 'torn')."
            )
        },
    )
    amount: float = field(
        default=4.8,
        metadata={
            "doc": (
                "How far a torn border strays, in px (0 to 48): the standard deviation of the shift. "
                "Raising it tears the edge wider; 0 is straight."
            )
        },
    )
    scale: float = field(
        default=24.0,
        metadata={
            "doc": (
                "The size of the bends of a torn border, in px (1 to 480): the spacing of the "
                "coarsest noise. Raising it gives long gentle wanders, lowering it a jagged edge."
            )
        },
    )
    seed: int = field(
        default=0,
        metadata={
            "doc": ("Random seed of the tear: the same seed always tears the same way, another seed differently.")
        },
    )
    octaves: int = field(
        default=6,
        metadata={
            "doc": (
                "How many layers of noise make the tear (1 to 8): each is twice as fine as the one "
                "before. Raising it adds fine ragged detail to the long wanders; an octave finer than "
                "a pixel is left out."
            )
        },
    )
    persistence: float = field(
        default=0.55,
        metadata={
            "doc": (
                "How strong each finer layer of the tear is compared with the one before (0 to 1). "
                "Raising it makes the edge rougher; 0 leaves only the coarsest wander."
            )
        },
    )


@piece()
@dataclass(frozen=True, kw_only=True)
class _Laid(_Piece):
    """A kind laid over a frame: the picture, or the box of a selection, with borders straight or torn."""

    _make: ClassVar[dict[str, Callable[[Any], Any]]] = {"edge": lambda part: Edge(**part)}

    fit: str | None = field(
        default=None,
        metadata={
            "doc": (
                "The name of a selection (the built-in 'subject' or 'background', or a `Pick`): the "
                "kind is laid over that selection's bounding box instead of the whole picture, so its "
                "positions and lengths are shares of that box (object first: cut the subject, not the "
                "sky). Not set = the whole picture."
            )
        },
    )
    edge: Edge | Mapping[str, Any] = field(
        default=Edge(),
        metadata={
            "doc": (
                "How the borders between pieces run: an `Edge` (or its fields as a table: style, "
                "amount, scale, seed, octaves, persistence). The default is straight borders."
            )
        },
    )


@piece(_region("grid"))
@dataclass(frozen=True, kw_only=True)
class Grid(_Laid):
    """A grid of rectangular pieces, any number of columns and rows, equal or in uneven shares."""

    columns: int | Sequence[float] = field(
        default=(1.0,),
        metadata={
            "doc": (
                "How the frame is divided from left to right: a whole number (1 to 1000) of equal "
                "columns, or a list of 1 to 1000 positive numbers, the relative widths ([1, 2, 1] is "
                "a narrow, a wide and a narrow column); held as that list. `columns` x `rows` make at "
                "most 10000 pieces. A pixel belongs to the column whose range holds its centre; a "
                "centre exactly on a border goes to the later column. Beyond a `fit` box the outer "
                "columns run to the picture's edge."
            )
        },
    )
    rows: int | Sequence[float] = field(
        default=(1.0,),
        metadata={
            "doc": (
                "How the frame is divided from top to bottom: as `columns` (a whole number 1 to 1000, "
                "or a list of 1 to 1000 positive relative heights; at most 10000 pieces with the "
                "columns). Pieces are numbered row after row."
            )
        },
    )


@piece(_region("shards"))
@dataclass(frozen=True, kw_only=True)
class Shards(_Laid):
    """Irregular pieces like broken glass or stone: the cells round seeds scattered over the frame."""

    count: int = field(
        default=12,
        metadata={
            "doc": (
                "How many shards (1 to 500): exactly this many seeds are scattered, all inside the "
                "`fit` selection when there is one; a picture too small to give each seed a pixel has "
                "fewer pieces. Raising it makes smaller shards."
            )
        },
    )
    jitter: float = field(
        default=0.5,
        metadata={
            "doc": (
                "How far each seed strays from its place in a regular honeycomb, as a share of the "
                "spacing (0 to 1): 0 cuts a regular honeycomb of equal pieces, 1 lets every seed "
                "anywhere in its own place, an uneven scatter."
            )
        },
    )
    impact: Sequence[float] = field(
        default=(0.5, 0.5),
        metadata={
            "doc": (
                "Where the shards crowd to when `focus` is above 0, as (x, y) shares of the frame, "
                "each 0 to 1 (0, 0 is the top left corner; 0.5, 0.5 the middle)."
            )
        },
    )
    focus: float = field(
        default=0.0,
        metadata={
            "doc": (
                "How much the shards crowd round `impact` (0 to 5): a seed at a share r of the way to "
                "the farthest corner moves to r to the power 1 + focus of it, so 0 leaves them spread "
                "evenly and 2.5 makes small splinters at the impact point and big pieces far from it, "
                "like cracked glass."
            )
        },
    )
    seed: int = field(
        default=0,
        metadata={
            "doc": (
                "Random seed for where the seeds lie: the same seed always cuts the same shards, "
                "another seed other ones."
            )
        },
    )
    preset: str | None = field(
        default=None,
        metadata={
            "doc": (
                "The name of a shipped set of these fields ('glass', 'torn', 'stone' or 'mosaic'): "
                "its values are used for every field not given beside it (a given `edge` replaces the "
                "preset's whole `edge`). Not set = the defaults."
            )
        },
    )


@piece(_region("pick"))
@dataclass(frozen=True, kw_only=True)
class Pick(_Piece):
    """A selection: the pixels of some pieces of a partition."""

    of: str = field(
        default="",
        metadata={
            "doc": (
                "The name of the partition (grid, shards, cuts, svg or bands) whose pieces are picked; it must be set."
            )
        },
    )
    pieces: Sequence[int | str] = field(
        default=(0,),
        metadata={
            "doc": (
                "The pieces to choose, at least one: numbers counted from 0 in piece order (reading "
                "order; bands highest first) or, for an `Svg` partition, the `id` of a shape (text; "
                "every piece of every shape with that id). A number the partition has no piece for "
                "chooses nothing, so a recipe also works on a picture too small to have that many "
                "pieces; an id the file lacks is an error."
            )
        },
    )


@piece(lambda fields: native.region("cuts", {"lines": [fields]})["lines"][0])
@dataclass(frozen=True, kw_only=True)
class Line(_Piece):
    """One straight cut: a line through a point at an angle, from border to border."""

    x: float = field(
        default=0.5,
        metadata={
            "doc": (
                "Where the line passes, across, as a share of the frame's width (0 to 1): 0 is the "
                "left edge, 1 the right."
            )
        },
    )
    y: float = field(
        default=0.5,
        metadata={
            "doc": (
                "Where the line passes, down, as a share of the frame's height (0 to 1): 0 is the top "
                "edge, 1 the bottom."
            )
        },
    )
    angle: float = field(
        default=0.0,
        metadata={
            "doc": (
                "The direction of the line, in degrees clockwise from the right as seen on the "
                "picture (0 to 360; any other angle is the same direction inside that range): 0 and "
                "180 run left to right, 90 and 270 straight up and down, 45 falls to the right, 315 "
                "rises to the right. A line has no head, so an angle and the one 180 further cut "
                "alike. The angle is measured on the picture's pixels, whatever the frame's shape."
            )
        },
    )


@piece(lambda fields: native.region("cuts", {"lines": [{"path": "M 0 0 L 1 1", **fields}]})["lines"][0])
@dataclass(frozen=True, kw_only=True)
class Curve(_Piece):
    """One curved cut: an open SVG path from border to border."""

    path: str = field(
        default="M 0 0 L 1 1",
        metadata={
            "doc": (
                "An open SVG path (one M, then lines, Béziers or arcs: M L H V C S Q T A, absolute or "
                "relative) whose coordinates are shares of the frame, 0 to 1 across and down (the "
                "unit square is stretched to the frame). It should run from one border of the frame "
                "to another; an end that does not is carried straight on to the nearest border. "
                "Example: 'M 0.38 0 C 0.15 0.45, 0.85 0.55, 0.62 1'."
            )
        },
    )


@piece(_region("cuts"))
@dataclass(frozen=True, kw_only=True)
class Cuts(_Laid):
    """Straight and curved cuts across the frame: the pieces are the regions the cuts divide it into."""

    _make: ClassVar[dict[str, Callable[[Any], Any]]] = {
        "edge": lambda part: Edge(**part),
        "lines": lambda parts: tuple(Curve(**each) if "path" in each else Line(**each) for each in parts),
    }

    lines: Sequence[Line | Curve | Mapping[str, Any]] = field(
        default=(Line(),),
        metadata={
            "doc": (
                "The cuts, 1 to 32: each a `Line` (`x`, `y`, `angle`: straight, through that point) "
                "or a `Curve` (`path`: an open SVG path), from border to border of the frame. The "
                "pieces are the regions the cuts divide the frame into, numbered in reading order: n "
                "cuts that cross nowhere make n + 1 pieces, more cuts crossing give more."
            )
        },
    )


@piece(_region("path"))
@dataclass(frozen=True, kw_only=True)
class Path(_Laid):
    """A selection: everything inside a closed SVG path, softly edged."""

    d: str = field(
        default="M 0 0 H 1 V 1 H 0 Z",
        metadata={
            "doc": (
                "A closed SVG path (M L H V C S Q T A Z, absolute or relative) whose coordinates are "
                "shares of the frame, 0 to 1 across and down: the unit square is stretched to the "
                "frame, so an arc `A 0.25 0.25` is a circle only in a square frame. Each subpath is "
                "closed as filling does, and a pixel is inside by the nonzero rule (an inner outline "
                "drawn the other way round is a hole). Example: 'M 0.1 0.4 C 0.1 0.2, 0.4 0.2, 0.4 "
                "0.4 Z'."
            )
        },
    )
    samples: int = field(
        default=4,
        metadata={
            "doc": (
                "How many evenly spaced scanlines measure how much of a border pixel the path covers "
                "(1 to 16): 1 looks at the middle of the pixel row only, more give a smoother "
                "slanting border and cost more time."
            )
        },
    )

    @classmethod
    def rect(cls, x: float, y: float, w: float, h: float, **fields: Any) -> "Path":
        """The rectangle with top-left corner (`x`, `y`), `w` wide and `h` high, as shares of the frame (0 to 1 across
        and down): `Path.rect(0.1, 0.2, 0.5, 0.3)`. `w` and `h` are above 0. Other fields (`samples`) as for `Path`."""
        _positive(w=w, h=h)
        return cls(d=f"M {x:.9g} {y:.9g} H {x + w:.9g} V {y + h:.9g} H {x:.9g} Z", **fields)

    @classmethod
    def ellipse(cls, cx: float, cy: float, rx: float, ry: float, **fields: Any) -> "Path":
        """The ellipse centred on (`cx`, `cy`) with radii `rx` across and `ry` down, as shares of the frame (0 to 1):
        `Path.ellipse(0.5, 0.5, 0.3, 0.3)`. `rx` and `ry` are above 0. Shares stretch with the frame, so the ellipse is
        a circle only in a square frame; in a wider one give `rx` a smaller share than `ry`."""
        _positive(rx=rx, ry=ry)
        left, right = f"{cx - rx:.9g} {cy:.9g}", f"{cx + rx:.9g} {cy:.9g}"
        arc = f"A {rx:.9g} {ry:.9g} 0 0 1"
        return cls(d=f"M {left} {arc} {right} {arc} {left} Z", **fields)


def _positive(**sizes: float) -> None:
    """Refuse a size that is not above 0, naming it and its value."""
    for name, value in sizes.items():
        if not value > 0:  # also refuses nan
            raise RecipeError(f"`{name}` must be above 0, got {value}")


@piece(_region("svg"))
@dataclass(frozen=True, kw_only=True)
class Svg(_Laid):
    """Shapes from an SVG file: every shape is a piece, and the area outside all of them one more piece."""

    file: str = field(
        default="",
        metadata={
            "doc": (
                "The SVG file to read; it must be set. A relative path in a recipe file is relative "
                "to that file. The file is untrusted input: nothing in it is run or fetched. Each "
                "rect, circle, ellipse, polygon, polyline and path is a piece (colours, strokes and "
                "fill are ignored; the fill rule is kept, so holes are holes); later shapes lie on "
                "top of earlier ones; a `use` is drawn where it is used. The file's `viewBox` and "
                "`preserveAspectRatio` (by default fitted inside and centred) place it in the frame. "
                "Pieces are numbered in reading order like any partition, the area outside every "
                "shape included (piece 0 when the top left pixel is outside every shape); `Pick` "
                "chooses a shape by its `id`."
            )
        },
    )
    max_shapes: int = field(
        default=1000,
        metadata={
            "doc": (
                "The most shapes the file may hold, counting what each `use` repeats (1 to 100000); a "
                "file with more is refused, which guards against a small file that draws a huge "
                "number of shapes."
            )
        },
    )
    max_kilobytes: int = field(
        default=1024,
        metadata={
            "doc": (
                "The largest file to read, in kilobytes (1 to 100000); a bigger one is refused, "
                "reading no more than that."
            )
        },
    )


@piece(_region("subject"))
@dataclass(frozen=True, kw_only=True)
class Subject(_Piece):
    """A selection: the subject of the picture, or, with `per`, the subject found inside each piece of a partition
    (grid first). Without `per` it is the built-in `subject`, and the other fields do nothing."""

    per: str | None = field(
        default=None,
        metadata={
            "doc": (
                "The name of a partition (grid, shards, cuts, svg or bands): the subject is found "
                "inside each of its pieces separately and the finds are united, so small things a "
                "whole-picture search misses (a calf behind its mother, a fox on a rock) are found. "
                "Not set = the subject of the whole picture, as the built-in 'subject'."
            )
        },
    )
    context: float = field(
        default=0.25,
        metadata={
            "doc": (
                "How much of its surroundings the finder sees round a piece, as a share of the "
                "piece's own size added on each side (0 to 1); the view is clipped to the picture. "
                "Only the piece's own pixels count. Raising it shows the finder more context, so a "
                "subject cut by the piece's border is recognised; 0 shows the bare piece. Only "
                "matters with `per`."
            )
        },
    )
    threshold: float = field(
        default=0.9,
        metadata={
            "doc": (
                "How sure the finder must be of a pixel for it to count as subject (0 to 1): a pixel "
                "it gives a value of this or less is dropped. Raising it keeps only what the finder "
                "is certain of and drops the faint subjects it invents in a plain piece, but also the "
                "uncertain parts of a real one; 0 keeps every pixel the finder gives. Only matters "
                "with `per`."
            )
        },
    )


@piece(_region("band"))
@dataclass(frozen=True, kw_only=True)
class Band(_Piece):
    """A selection: the pixels where another selection (a field of the photo) lies between two values, fading out
    linearly beyond them. The values are shares of the field's own scale (0 to 1)."""

    of: str = field(
        default="",
        metadata={
            "doc": (
                "The name of the selection whose values are cut, which must be set: a built-in one "
                "(`depth`: 1 is the nearest; `light`: the picture's brightness, 1 is white; "
                "`subject`) or one the recipe declares (a `given` map, a gradient, another band). Its "
                "value at a pixel decides how far the pixel is chosen."
            )
        },
    )
    low: float = field(
        default=0.5,
        metadata={
            "doc": (
                "The lowest value chosen fully (0 to 1, at most `high`): every pixel whose value lies "
                "from `low` to `high` is chosen. Raising it leaves out the lower values; 0 reaches "
                "down to the bottom of the scale."
            )
        },
    )
    high: float = field(
        default=1.0,
        metadata={
            "doc": (
                "The highest value chosen fully (0 to 1): see `low`. Lowering it leaves out the "
                "higher values; 1 reaches up to the top of the scale."
            )
        },
    )
    feather: float = field(
        default=0.1,
        metadata={
            "doc": (
                "How far beyond `low` and `high` the choice fades out, as a share of the scale (0 to "
                "1): it falls linearly from 1 at the bound to 0 this far past it. Raising it softens "
                "the edge; 0 cuts it hard."
            )
        },
    )


@piece(_region("bands"))
@dataclass(frozen=True, kw_only=True)
class Bands(_Piece):
    """A partition into bands of a field: equal ranges of its scale, numbered highest first."""

    of: str = field(
        default="",
        metadata={
            "doc": (
                "The name of the selection whose values are cut into bands, which must be set (see "
                "`Band.of`). Band 0 holds its highest values: the nearest for `depth`, the brightest "
                "for `light`."
            )
        },
    )
    count: int = field(
        default=3,
        metadata={
            "doc": (
                "How many equal ranges the scale is cut into (1 to 100). Band 0 is the highest, the "
                "last the lowest: by value, not in reading order. A value on a border goes to the "
                "lower band (0 to the lowest); a band no pixel lies in has no piece and the rest are "
                "renumbered, highest first. Raising it makes thinner bands."
            )
        },
    )


@piece(_region("components"))
@dataclass(frozen=True, kw_only=True)
class Components(_Piece):
    """A partition into the connected pieces of a selection: the windows of a facade, the letters of a sign."""

    of: str = field(
        default="",
        metadata={
            "doc": (
                "The name of the selection whose pieces are found, which must be set (see `Band.of`). "
                "Pixels where it is below `threshold`, and those of pieces dropped as too small, make "
                "one more piece, numbered like any by its first pixel."
            )
        },
    )
    threshold: float = field(
        default=0.5,
        metadata={
            "doc": (
                "The value (0 to 1) from which a pixel of the selection is in a piece. Raising it keeps "
                "only what the selection is surer of; lowering it takes in more of a soft edge."
            )
        },
    )
    min_area: float = field(
        default=0.001,
        metadata={
            "doc": (
                "The smallest piece kept, as a share of the frame's area (0 to 1): a smaller one joins "
                "the rest. Raising it drops specks; 0 keeps every piece, down to a single pixel."
            )
        },
    )
    max_pieces: int = field(
        default=1000,
        metadata={
            "doc": (
                "The most pieces the selection may have after the small ones are dropped (1 to 10000); "
                "more is refused. Raising it allows a busier picture."
            )
        },
    )


@piece(_region("combine"))
@dataclass(frozen=True, kw_only=True)
class Combine(_Piece):
    """A selection made of other selections: where all of them are chosen (multiply, min) or any of them is (max)."""

    op: Literal["multiply", "min", "max"] = field(
        default="multiply",
        metadata={
            "doc": (
                "How the selections are put together at each pixel: 'multiply' (their product: all of "
                "them, fading where any fades), 'min' (the least of them: all of them, hard where any "
                "is hard) or 'max' (the greatest: any of them)."
            )
        },
    )
    of: Sequence[str] = field(
        default=("", ""),
        metadata={
            "doc": (
                "The names of the selections to combine, at least two, which must be set: built-in "
                "ones (`subject`, `background`, `depth`, `light`) or ones the recipe declares."
            )
        },
    )


@piece(_region("colour"))
@dataclass(frozen=True, kw_only=True)
class Colour(_Piece):
    """A selection of the pixels near a colour: 1 within `tolerance` of `target`, falling to 0 over `feather` beyond.
    The distance is Euclidean in Oklab, where equal distances look about equally different and black to white is 1.
    It is read from the photo as framed and sized, before any step, as `light` is."""

    target: ColorForms = field(
        default="#000000",
        metadata={"doc": "The colour looked for: '#rrggbb', '#rgb' or three numbers 0..1 (sRGB)."},
    )
    tolerance: float = field(
        default=0.1,
        metadata={
            "doc": (
                "How far from `target` a pixel may be and still be chosen fully (0 to 1), as a share of the "
                "Oklab distance from black to white. Raising it takes in more shades of the colour; 0 chooses "
                "only the exact colour."
            )
        },
    )
    feather: float = field(
        default=0.05,
        metadata={
            "doc": (
                "The extra distance beyond `tolerance` over which the selection falls linearly from 1 to 0 "
                "(0 to 1), in the same distance. Raising it softens the edge; 0 cuts hard."
            )
        },
    )


@piece(_region("linear_gradient"))
@dataclass(frozen=True, kw_only=True)
class LinearGradient(_Piece):
    """A selection that rises evenly across the picture in one direction: 0 at one edge, 1 at the opposite one."""

    angle: float = field(
        default=270.0,
        metadata={
            "doc": (
                "The direction in which the value rises, clockwise from the right (0 to 360): 0 rises "
                "to the right, 90 downward, 180 to the left, 270 upward. The value is a share of the "
                "picture's extent along that direction, so the far corners are 0 and 1 at any aspect "
                "ratio."
            )
        },
    )


@piece(_region("radial_gradient"))
@dataclass(frozen=True, kw_only=True)
class RadialGradient(_Piece):
    """A selection that rises evenly from a point: 0 at the centre, 1 at `radius` and beyond."""

    x: float = field(
        default=0.5,
        metadata={
            "doc": (
                "Where the centre is across the picture, as a share of its width (0 to 1): 0 is the "
                "left edge, 1 the right."
            )
        },
    )
    y: float = field(
        default=0.5,
        metadata={
            "doc": (
                "Where the centre is down the picture, as a share of its height (0 to 1): 0 is the "
                "top edge, 1 the bottom."
            )
        },
    )
    radius: float = field(
        default=0.5,
        metadata={
            "doc": (
                "The distance from the centre at which the value reaches 1, as a share of the "
                "picture's shorter side (0.01 to 3). Raising it spreads the gradient wider; 0.5, "
                "centred, reaches the midpoints of the shorter sides' edges."
            )
        },
    )


@piece(_region("given"))
@dataclass(frozen=True, kw_only=True)
class Given(_Piece):
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
    | Components
    | Combine
    | Colour
    | LinearGradient
    | RadialGradient
    | Given
)


def kind_name(region: object) -> str:
    """The name the core gives the kind of `region`: the class's name in snake case (`LinearGradient` is
    `linear_gradient`)."""
    return re.sub(r"(?<!^)(?=[A-Z])", "_", type(region).__name__).lower()


KINDS: dict[str, type[Kind]] = {re.sub(r"(?<!^)(?=[A-Z])", "_", cls.__name__).lower(): cls for cls in get_args(Kind)}
"""The region classes by the name of their kind, as a recipe file writes `kind`."""
