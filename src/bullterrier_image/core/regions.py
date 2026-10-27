"""Regions: which pixels of a picture a step is about, as a partition or a selection made from a declared kind.

A **partition** (`Partition`) gives every pixel to exactly one piece; the pieces are numbered 0..n-1 in *reading
order*: piece k comes before piece k+1 when its first pixel comes earlier in the raster scan (left to right along each
row, rows from the top). For a grid that is row after row; for any other kind it is a rule with no constants. The one
exception is `Bands`, numbered by value, highest first. A kind that makes more pieces than a small picture has
pixels (or bands than its values fill) yields only the pieces that have a pixel, so n can be smaller than asked for. A
**selection** (`Selection`) says how much each pixel is chosen, 0..1.

The kinds are models declared like an effect's parameters (partitions `Grid`, `Shards`, `Cuts`, `Svg` and `Bands`;
selections `Path`, `Pick`, `Subject`, `Band`, `Combine`, `LinearGradient`, `RadialGradient` and `Given`; `Edge` bends
the borders of the kinds with `fit`), so every field has a unit, a range and a description: `Shards.documentation()`
lists them. A recipe or a script declares them by name, and `resolve` turns the declarations into the regions of one
picture, building only what is asked for. `Subject` with `per` is *grid first*: the subject is found inside each piece
of a partition, one call of the injected finder per piece, and the finds are united. A **field** is a selection read for
its values, not only as a mask: the built-in `depth` (1 = nearest, the injected map), `light` (the picture's brightness,
made by the runner from the working picture), `subject`, a `Given` map handed in under the region's own name, a
gradient, or any other selection. `Band` chooses the pixels where a field lies in a range, `Bands` cuts a field into
equal ranges (highest first), `Combine` multiplies, takes the least or the greatest of several.
`look_weights` turns a partition and a look for each piece into the weights that blend the looks.

Promises:
- Positions are shares of the *frame* (the picture, or the bounding box of the selection a kind is laid `fit` over)
  and the other extents shares of a named reference; the lengths of a torn `Edge` are design px (`px`, written for a
  picture whose long side is `DESIGN_SIZE`, scaled to the picture like an effect's), so the same declaration cuts the
  same pieces at any resolution (a torn edge too: its noise is laid on the frame).
- The same declaration, picture size and seed give the same regions on the same machine and library versions; no global
  random state is used.
- Regions are made once per `resolve` and handed out read-only.
- A declaration is checked when it is made (a value out of range) and when it is resolved (an unknown name, a name of
  the wrong kind of region, a cycle): a `RecipeError` naming the region and what is valid.

Dependency direction: imports `params`, `presets` and `ops`; the steps and the runner build on it.
"""

import functools
import math
import re
from collections.abc import Callable, Iterable, Iterator, Mapping, Sequence
from dataclasses import dataclass, field
from typing import Annotated, Any, Literal

import numpy as np
from pydantic import BeforeValidator, field_validator, model_validator
from scipy import ndimage

from bullterrier_image.errors import DepthNotFoundError, MapNotFoundError, RecipeError, SubjectNotFoundError
from bullterrier_image.ops.blur import gaussian_blur
from bullterrier_image.ops.color import luminance
from bullterrier_image.ops.maps import map_problem
from bullterrier_image.ops.noise import frame_fractal
from bullterrier_image.ops.raster import coverage, paint
from bullterrier_image.ops.voronoi import centres, nearest

from .outlines import path_outlines, read_svg
from .params import EffectParams, design_scale, param
from .presets import SHIPPED, read

__all__ = [
    "DERIVED",
    "Band",
    "Bands",
    "BoxFinder",
    "Combine",
    "Curve",
    "Cuts",
    "Edge",
    "Given",
    "Grid",
    "Kind",
    "Line",
    "LinearGradient",
    "Partition",
    "Path",
    "Pick",
    "RadialGradient",
    "Regions",
    "Selection",
    "Shards",
    "Subject",
    "Svg",
    "as_partition",
    "check",
    "check_map",
    "derived_maps",
    "look_weights",
    "map_error",
    "maps_used",
    "needs",
    "needs_finder",
    "resolve",
    "used",
]

BUILT_IN = ("subject", "background", "depth", "light")
BUILT_IN_MAPS = ("subject", "depth", "light")  # the built-in regions that are a map of the photo as it is
# The built-in maps made from the working picture itself, by name: not found, handed in or cached, so the runner makes
# them for each render. `light` is the picture's brightness (Rec. 709 weights, `ops.color.luminance`).
DERIVED = {"light": luminance}

_HALF = 0.5  # constant: a pixel is chosen when more than half of it is (the hard edge of a selection)
_LACUNARITY = 2.0  # constant: each octave of fractal noise has twice the frequency of the one before (the standard)
_TRUNCATE = 4.0  # constant: a feather's Gaussian is cut off at 4 sigmas, as scipy does by default (error < 1e-4)
_HEX_ROWS = math.sqrt(3) / 2  # constant: the distance between rows of a hexagonal lattice, in units of its spacing
_MOST = 1000  # the most columns, rows, or shares of a grid


@dataclass(frozen=True, eq=False)
class Partition:
    """Every pixel in exactly one piece. `labels` is int32 `(height, width)`: the piece of each pixel, numbered
    0..`count`-1 in piece order (the module docstring: reading order, `Bands` highest first); every number
    occurs. `ids` maps an SVG id to the numbers of the pieces of the shapes that have it (empty for a shape that is
    covered completely, and for every kind but `Svg`). Read-only when made by `resolve`."""

    labels: np.ndarray
    count: int
    ids: dict[str, tuple[int, ...]] = field(default_factory=dict)


@dataclass(frozen=True, eq=False)
class Selection:
    """How much each pixel is chosen. `alpha` is float32 `(height, width)`, 0..1. Read-only when made by `resolve`."""

    alpha: np.ndarray


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


_UNIT_FRAME = (0.0, 0.0, 1.0, 1.0)  # the frame a path is first checked in, when only its syntax is known
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

    @field_validator("path")
    @classmethod
    def _one_open_path(cls, path: str) -> str:
        """Readable path data of a single subpath."""
        if len(path_outlines(path, _UNIT_FRAME)) != 1:
            raise ValueError("must be one path with a single M (a cut is one curve from border to border)")
        return path


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

    @field_validator("d")
    @classmethod
    def _readable(cls, d: str) -> str:
        """Readable path data."""
        path_outlines(d, _UNIT_FRAME)
        return d


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
        doc="The largest file to read, in kilobytes (1 to 100000); a bigger one is refused without being read.",
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
# Finds the subject inside a box of the picture, `(left, top, right, bottom)` in px: float32 `(bottom - top, right -
# left)` alpha, 0..1, or SubjectNotFoundError when nothing stands out there.
BoxFinder = Callable[[tuple[int, int, int, int]], np.ndarray]


def map_error(name: str) -> type[MapNotFoundError]:
    """The error for a problem with the map called `name`: `SubjectNotFoundError` for the subject, `DepthNotFoundError`
    for the depth, `MapNotFoundError` for any other map."""
    return {"subject": SubjectNotFoundError, "depth": DepthNotFoundError}.get(name, MapNotFoundError)


def check_map(name: str, layer: np.ndarray, error: type[MapNotFoundError]) -> np.ndarray:
    """`layer`, once its values are known to be finite and within 0..1; else `error` naming the map `name` and
    `map_problem`'s answer."""
    if (problem := map_problem(layer)) is not None:
        raise error(f"{name} has {problem}: a map must be finite and within 0..1")
    return layer


def _frozen(array: np.ndarray) -> np.ndarray:
    """`array`, read-only: regions are shared by every step that asks for them."""
    array.setflags(write=False)
    return array


def _renumber(raw: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """The integer array `raw` (any numbers, same value = same piece) renumbered 0..n-1 by where each value is first met
    in the raster scan, and the value of each new piece."""
    values, first, inverse = np.unique(raw.ravel(), return_index=True, return_inverse=True)
    order = np.argsort(first)
    number = np.empty(len(values), np.int32)
    number[order] = np.arange(len(values), dtype=np.int32)
    return number[inverse].reshape(raw.shape), values[order]


def _partition(raw: np.ndarray) -> Partition:
    """The partition whose pieces are the distinct values of the integer array `raw`, in reading order."""
    labels, values = _renumber(raw)
    return Partition(_frozen(labels), len(values))


def as_partition(selection: Selection) -> Partition:
    """A hard-edged selection as a partition of the pixels it chooses (more than half) and those it does not; a
    selection that chooses all or none of the picture has one piece. Numbered in reading order like any partition."""
    return _partition((selection.alpha > _HALF).astype(np.int32))


def _coordinates(shape: tuple[int, int], edge: Edge) -> tuple[np.ndarray, np.ndarray]:
    """The `(x, y)` that every kind classifies, in px: the pixel centres, bent by noise for a torn `edge` (whose px
    are design px, scaled to `shape`). They broadcast against each other to `shape`."""
    x, y = centres(shape)
    if edge.style == "straight":
        return x, y
    edge = edge.scaled(design_scale(shape))
    long_side = max(shape)
    bend = [
        frame_fractal(
            shape,
            scale=edge.scale / long_side,
            octaves=edge.octaves,
            persistence=edge.persistence,
            lacunarity=_LACUNARITY,
            seed=[edge.seed, axis],
        )
        * edge.amount
        for axis in (0, 1)
    ]
    return x + bend[0], y + bend[1]


def _borders(shares: tuple[float, ...], low: float, high: float) -> np.ndarray:
    """The positions in px, from `low` to `high`, of the borders between pieces of the relative widths `shares`."""
    widths = np.asarray(shares, np.float64)
    return low + (high - low) * np.cumsum(widths)[:-1] / widths.sum()


def _sites(
    kind: Shards, shape: tuple[int, int], box: tuple[int, int, int, int], chosen: np.ndarray | None
) -> np.ndarray:
    """The `kind.count` seeds `(x, y)` in px: a jittered hexagonal lattice over `box` (sized so that about `count`
    fall in the chosen pixels), crowded towards the impact point, kept to the chosen pixels, then made exactly `count`:
    seeds beyond it are dropped at random, missing ones are added at random chosen pixels."""
    height, width = shape
    left, top, right, bottom = box
    allowed = np.ones(shape, bool) if chosen is None else chosen
    rng = np.random.default_rng(kind.seed)
    across, down = right - left, bottom - top
    wanted = kind.count * across * down / allowed.sum()  # lattice points over the box, so `count` land on the pixels
    columns = max(1, round(math.sqrt(wanted * across * _HEX_ROWS / down)))  # rows are _HEX_ROWS apart, as columns are 1
    rows = max(1, round(wanted / columns))
    row, column = np.mgrid[0:rows, 0:columns]
    # alternate rows are shifted by half a spacing; the quarter keeps both shifts inside the box
    sites = np.stack([left + (column + 0.25 + 0.5 * (row % 2)) / columns * across, top + (row + 0.5) / rows * down])
    sites = sites.reshape(2, -1).T
    sites += (rng.random(sites.shape) - 0.5) * kind.jitter * (across / columns, down / rows)
    impact = np.array([left + kind.impact[0] * across, top + kind.impact[1] * down])
    reach = max(math.hypot(x - impact[0], y - impact[1]) for x in (left, right) for y in (top, bottom))
    away = sites - impact
    sites = impact + away * (np.hypot(away[:, 0], away[:, 1]) / reach)[:, None] ** kind.focus
    x, y = np.floor(sites).astype(int).T
    kept = (x >= 0) & (x < width) & (y >= 0) & (y < height)
    kept[kept] = allowed[y[kept], x[kept]]
    sites = sites[kept]
    if len(sites) > kind.count:
        sites = sites[np.sort(rng.choice(len(sites), kind.count, replace=False))]
    elif len(sites) < kind.count:
        pixels = rng.choice(np.flatnonzero(allowed), kind.count - len(sites))
        sites = np.concatenate(
            [sites, np.stack([pixels % width, pixels // width], axis=1) + rng.random((len(pixels), 2))]
        )
    return sites


# ---- cuts, paths and svg files: shapes drawn on the picture, read at the (possibly bent) coordinates ----------------


def _sample(raster: np.ndarray, x: np.ndarray, y: np.ndarray) -> np.ndarray:
    """`raster` read at the pixel holding each coordinate pair (which broadcast against each other); a coordinate bent
    beyond the picture reads the pixel at its edge."""
    rows = np.clip(np.floor(y), 0, raster.shape[0] - 1).astype(np.intp)
    columns = np.clip(np.floor(x), 0, raster.shape[1] - 1).astype(np.intp)
    return raster[rows, columns]


def _along(point: np.ndarray, box: tuple[int, int, int, int]) -> float:
    """Where on the frame's border the point nearest to `point` lies, as a position from 0 to 4 clockwise from the top
    left corner: the top edge is 0 to 1, the right edge 1 to 2, the bottom 2 to 3 (right to left), the left 3 to 4."""
    left, top, right, bottom = box
    u = float(np.clip((point[0] - left) / (right - left), 0, 1))
    v = float(np.clip((point[1] - top) / (bottom - top), 0, 1))
    return min((v, u), (1 - u, 1 + v), (1 - v, 3 - u), (u, 4 - v))[1]


def _border_point(position: float, box: tuple[int, int, int, int]) -> np.ndarray:
    """The point on the frame's border at `position` (see `_along`)."""
    left, top, right, bottom = box
    corners = np.array([(left, top), (right, top), (right, bottom), (left, bottom)], float)
    corner, share = int(position) % 4, position - int(position)
    return corners[corner] + share * (corners[(corner + 1) % 4] - corners[corner])


def _beyond(curve: np.ndarray, box: tuple[int, int, int, int], shape: tuple[int, int]) -> np.ndarray:
    """Which pixel centres lie on one side of the open `curve`: the curve is closed by carrying each end straight to the
    nearest border and going clockwise along the border from its last end to its first, and what that encloses is
    one side (which one does not matter: pieces are numbered by where they start)."""
    end, start = _along(curve[-1], box), _along(curve[0], box)
    corners = [k for k in sorted(range(4), key=lambda k: (k - end) % 4) if 0 < (k - end) % 4 < (start - end) % 4]
    closed = np.vstack([curve, *(_border_point(position, box) for position in (end, *corners, start))])
    side = np.zeros(shape, bool)
    paint(side, True, [closed], rule="nonzero")
    return side


def _cut(
    kind: Cuts, shape: tuple[int, int], box: tuple[int, int, int, int], x: np.ndarray, y: np.ndarray
) -> np.ndarray:
    """A number for every pixel that is the same exactly for pixels on the same side of every cut of `kind`."""
    left, top, right, bottom = box
    pieces = np.zeros(shape, np.int64)
    for bit, cut in enumerate(kind.lines):
        if isinstance(cut, Line):
            angle = math.radians(cut.angle)
            # the line runs along (cos, sin) on the screen (y points down), so the normal is (-sin, cos)
            side = (y - (top + cut.y * (bottom - top))) * math.cos(angle) - (
                x - (left + cut.x * (right - left))
            ) * math.sin(angle) > 0
        else:
            (curve,) = path_outlines(cut.path, box)
            side = _sample(_beyond(curve, box, shape), x, y)
        pieces |= side.astype(np.int64) << bit
    return pieces


def _enclosed(
    kind: Path, shape: tuple[int, int], box: tuple[int, int, int, int], x: np.ndarray, y: np.ndarray
) -> np.ndarray:
    """How much of each pixel the path of `kind` covers; with a torn edge read (bilinearly) at the bent coordinates."""
    covered = coverage(path_outlines(kind.d, box), shape, "nonzero", kind.samples)
    if kind.edge.style == "straight":
        return covered
    rows, columns = np.broadcast_arrays(y - 0.5, x - 0.5)  # pixel centres are at half-integers
    return ndimage.map_coordinates(covered, [rows, columns], order=1, mode="nearest")


def _drawn(
    kind: Svg, shape: tuple[int, int], box: tuple[int, int, int, int], x: np.ndarray, y: np.ndarray
) -> Partition:
    """The pieces of the svg file of `kind`: shape k + 1 (later ones on top) or 0 for none, at the bent coordinates."""
    if not kind.file:
        raise RecipeError("`file` is not set: name the svg file to read")
    shapes = read_svg(kind.file, box, max_kilobytes=kind.max_kilobytes, max_shapes=kind.max_shapes)
    drawing = np.zeros(shape, np.int32)
    for number, one in enumerate(shapes, start=1):
        paint(drawing, number, one.outlines, rule=one.rule)
    labels, drawn = _renumber(_sample(drawing, x, y))
    ids: dict[str, tuple[int, ...]] = {one.id: () for one in shapes if one.id}
    for piece, number in enumerate(drawn):
        if number and shapes[number - 1].id:
            ids[shapes[number - 1].id] += (piece,)
    return Partition(_frozen(labels), len(drawn), ids)


def _band(kind: Band, field: np.ndarray) -> Selection:
    """The selection `kind` makes of the field `field` (float32 `(height, width)`, 0..1)."""
    outside = np.maximum(np.maximum(kind.low - field, field - kind.high), 0)  # how far beyond low..high
    alpha = np.clip(1 - outside / kind.feather, 0, 1) if kind.feather else outside == 0
    return Selection(_frozen(alpha.astype(np.float32)))


def _bands(kind: Bands, field: np.ndarray) -> Partition:
    """The partition `kind` makes of the field `field` (float32 `(height, width)`, 0..1): band 0 holds the highest."""
    bands = np.minimum(np.floor((1 - field.astype(np.float64)) * kind.count), kind.count - 1)  # 0 = the highest
    present, labels = np.unique(bands.ravel(), return_inverse=True)  # numbered by band, so still highest first
    return Partition(_frozen(labels.reshape(field.shape).astype(np.int32)), len(present))


def _combined(kind: Combine, fields: Sequence[np.ndarray]) -> Selection:
    """The selection `kind` makes of the selections `fields` (each float32 `(height, width)`, 0..1)."""
    combine = {"multiply": np.multiply, "min": np.minimum, "max": np.maximum}[kind.op]
    return Selection(_frozen(functools.reduce(combine, fields).astype(np.float32)))


def _linear(kind: LinearGradient, shape: tuple[int, int]) -> Selection:
    """The linear gradient `kind` over a picture of `shape`: its position along the direction as a share of the
    picture's extent that way, taken at each pixel's centre."""
    height, width = shape
    x, y = centres(shape)
    angle = math.radians(kind.angle)
    extent = width * abs(math.cos(angle)) + height * abs(math.sin(angle))
    along = ((x - width / 2) * math.cos(angle) + (y - height / 2) * math.sin(angle)) / extent + _HALF
    return Selection(_frozen(np.clip(along, 0, 1).astype(np.float32)))


def _radial(kind: RadialGradient, shape: tuple[int, int]) -> Selection:
    """The radial gradient `kind` over a picture of `shape`: a pixel's distance from the centre as a share of
    `radius`, from 0 at the centre to 1."""
    height, width = shape
    x, y = centres(shape)
    distance = np.hypot(x - kind.x * width, y - kind.y * height)
    return Selection(_frozen(np.clip(distance / (kind.radius * min(shape)), 0, 1).astype(np.float32)))


def look_weights(partition: Partition, assignment: Sequence[int], count: int, *, feather: float) -> np.ndarray:
    """The weights that blend `count` looks over a picture: float32 `(count, height, width)`, between 0 and 1 and
    summing to 1 at every pixel. `assignment[k]` is the look (0..`count`-1) of piece k, so there is one entry per
    piece; a look no piece has weighs 0 everywhere.

    A look's weight is 1 inside its pieces and 0 outside, softened across the borders by a Gaussian of `feather` px
    (its standard deviation; 0 = hard edges, exactly the pieces). The picture's border is not a border: the weights
    there go on as they are. The sums are 1 and the weights within 0..1 to float32 rounding (1e-6). It needs room for
    the `count` maps and little else: no supersampling, one blur at a time."""
    looks = np.asarray(assignment, np.int32)[partition.labels]
    weights = np.empty((count, *looks.shape), np.float32)
    for look in range(count):
        weights[look] = looks == look
        if feather > 0:
            weights[look] = gaussian_blur(weights[look], feather, truncate=_TRUNCATE, edge="nearest")
    return weights


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


class Regions(Mapping[str, Partition | Selection]):
    """The regions of one picture by name: the built-in `subject`, `background`, `depth` and `light` and every declared
    region. A region is made the first time it is asked for (with what it is made from), then kept."""

    def __init__(
        self,
        declared: Mapping[str, Kind],
        shape: tuple[int, int],
        maps: Mapping[str, np.ndarray],
        find: BoxFinder | None,
    ) -> None:
        """Made by `resolve`, which checks `declared` first."""
        self._declared, self._shape, self._maps, self._find = declared, shape, maps, find
        self._made: dict[str, Partition | Selection] = {}

    def __getitem__(self, name: str) -> Partition | Selection:
        """The region `name`; KeyError when it is neither built in nor declared. The error of a map (`map_error`: a
        `SubjectNotFoundError`, a `DepthNotFoundError`, else a `MapNotFoundError`), naming the region, when it needs a
        map that was not given; RecipeError when its `fit` selection chooses nothing in this picture."""
        if name not in self._made:
            self._made[name] = self._make(name)
        return self._made[name]

    def partition(self, name: str) -> Partition:
        """The region `name` as a partition; RecipeError when it is a selection (KeyError when there is none)."""
        region = self[name]
        if not isinstance(region, Partition):
            raise RecipeError(f"region {name!r} is a selection, but a partition is needed here")
        return region

    def pieces(self, name: str) -> Partition:
        """The region `name` as a partition: a partition as it is, a selection as `as_partition` splits it (KeyError
        when there is none)."""
        region = self[name]
        return region if isinstance(region, Partition) else as_partition(region)

    def selection(self, name: str) -> Selection:
        """The region `name` as a selection; RecipeError when it is a partition (KeyError when there is none)."""
        region = self[name]
        if not isinstance(region, Selection):
            raise RecipeError(f"region {name!r} is a partition, but a selection is needed here")
        return region

    def __contains__(self, name: object) -> bool:
        """Whether `name` is a built-in or declared region, without making it."""
        return name in BUILT_IN or name in self._declared

    def __iter__(self) -> Iterator[str]:
        """The built-in names, then the declared ones in the order declared."""
        return iter((*BUILT_IN, *self._declared))

    def __len__(self) -> int:
        """How many regions there are."""
        return len(BUILT_IN) + len(self._declared)

    def _make(self, name: str) -> Partition | Selection:
        match name:
            case "subject" | "depth" | "light":
                return Selection(self._map(name, f"the built-in region {name!r}"))
            case "background":
                return Selection(_frozen(1 - self.selection("subject").alpha))
        kind = self._declared[name]
        if isinstance(kind, Given):
            return Selection(self._map(name, f"region {name!r}"))
        if isinstance(kind, Subject):
            return self._subject_of(name, kind)
        if isinstance(kind, Pick):
            return self._pick(name, kind)
        match kind:
            case Band():
                return _band(kind, self._values(name, kind.of))
            case Bands():
                return _bands(kind, self._values(name, kind.of))
            case Combine():
                return _combined(kind, [self._values(name, of) for of in kind.of])
            case LinearGradient():
                return _linear(kind, self._shape)
            case RadialGradient():
                return _radial(kind, self._shape)
        box, chosen = self._frame(name, kind.fit)
        x, y = _coordinates(self._shape, kind.edge)
        try:
            match kind:
                case Grid():
                    left, top, right, bottom = box
                    across = _borders(kind.columns, left, right)
                    column = np.searchsorted(across, x, side="right")
                    row = np.searchsorted(_borders(kind.rows, top, bottom), y, side="right")
                    return _partition(row * (len(across) + 1) + column)
                case Shards():
                    return _partition(nearest(_sites(kind, self._shape, box, chosen), x, y)[1])
                case Cuts():
                    return _partition(_cut(kind, self._shape, box, x, y))
                case Path():
                    return Selection(_frozen(_enclosed(kind, self._shape, box, x, y)))
                case Svg():
                    return _drawn(kind, self._shape, box, x, y)
        except RecipeError as error:
            raise RecipeError(f"region {name!r}: {error}") from None

    def _values(self, user: str, of: str) -> np.ndarray:
        """The values of the selection `of`, read by the region `user`: a built-in map straight from the maps given, so
        that a missing one is named for `user`; any other selection as it is made."""
        if of in BUILT_IN_MAPS:
            return self._map(of, f"region {user!r}")
        return self.selection(of).alpha

    def _map(self, name: str, user: str) -> np.ndarray:
        """The map `name` of the whole picture, read-only as float32 (a view: the giver's array is left writable);
        `map_error(name)` naming `user`, which needs it, when it was not given."""
        if name not in self._maps:
            raise map_error(name)(f"{user} is made from the {name} map of the picture, but none was given")
        return _frozen(np.asarray(self._maps[name], np.float32).view())

    def _pick(self, name: str, kind: Pick) -> Selection:
        """The pixels of the pieces `kind` lists: a number as it is, an SVG id as the pieces of its shapes."""
        partition = self.partition(kind.of)
        chosen = [piece for piece in kind.pieces if isinstance(piece, int)]
        for piece in kind.pieces:
            if isinstance(piece, str):
                if piece not in partition.ids:
                    known = (
                        f"its ids: {', '.join(partition.ids)}"
                        if partition.ids
                        else "it has no ids (only svg regions do)"
                    )
                    raise RecipeError(
                        f"region {name!r}: `pieces` names {piece!r}, which is no id of {kind.of!r}; {known}"
                    )
                chosen += partition.ids[piece]
        return Selection(_frozen(np.isin(partition.labels, chosen).astype(np.float32)))

    def _subject_of(self, name: str, kind: Subject) -> Selection:
        """The built-in subject without `per`; with it the subject found in every piece, united. For each piece the
        finder is asked once about the piece's box grown by `context`; of its answer only the piece's own pixels the
        finder is surer of than `threshold` are kept. A piece the finder finds nothing in (SubjectNotFoundError) adds
        nothing; an answer that does not fit the view, or is not finite and within 0..1, is a SubjectNotFoundError
        naming the piece."""
        if kind.per is None:
            return self.selection("subject")
        if self._find is None:
            raise SubjectNotFoundError(
                f"region {name!r} finds the subject in each piece of {kind.per!r}, but no subject finder was given"
            )
        labels = self.partition(kind.per).labels
        height, width = self._shape
        found = np.zeros(self._shape, np.float32)
        for piece, (rows, columns) in enumerate(ndimage.find_objects(labels + 1)):
            grow_y = round(kind.context * (rows.stop - rows.start))
            grow_x = round(kind.context * (columns.stop - columns.start))
            box = (
                max(0, columns.start - grow_x),
                max(0, rows.start - grow_y),
                min(width, columns.stop + grow_x),
                min(height, rows.stop + grow_y),
            )
            try:
                matte = np.asarray(self._find(box), np.float32)
            except SubjectNotFoundError:
                continue
            left, top, right, bottom = box
            if matte.shape != (bottom - top, right - left):
                view = (bottom - top, right - left)
                raise SubjectNotFoundError(
                    f"the subject finder gave a matte of shape {matte.shape} for a view of {view}"
                )
            check_map(f"the subject finder's matte of piece {piece} (view {box})", matte, SubjectNotFoundError)
            own = labels[top:bottom, left:right] == piece
            sure = own & (matte > kind.threshold)
            found[top:bottom, left:right][sure] = matte[sure]
        return Selection(_frozen(found))

    def _frame(self, name: str, fit: str | None) -> tuple[tuple[int, int, int, int], np.ndarray | None]:
        """The frame region `name` is laid over, `(left, top, right, bottom)` in px, and the pixels chosen by its
        `fit` selection (None when it has none: the frame is the picture)."""
        height, width = self._shape
        if fit is None:
            return (0, 0, width, height), None
        chosen = self.selection(fit).alpha > _HALF
        used_rows, used_columns = chosen.any(axis=1), chosen.any(axis=0)
        if not used_rows.any():
            raise RecipeError(f"region {name!r}: `fit` {fit!r} chooses nothing in this picture, so it has no frame")
        top, left = int(used_rows.argmax()), int(used_columns.argmax())
        bottom, right = height - int(used_rows[::-1].argmax()), width - int(used_columns[::-1].argmax())
        return (left, top, right, bottom), chosen


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


def used(declared: Mapping[str, Kind], names: Iterable[str]) -> set[str]:
    """The regions made when `names` (checked ones, as `check` leaves them) are asked for: those, and every region they
    are laid over or picked from, down to the built-in ones."""
    seen: set[str] = set()
    todo = list(names)
    while todo:
        name = todo.pop()
        if name not in seen:
            seen.add(name)
            todo += [] if name in BUILT_IN else _references(declared[name])
    return seen


def _wanted(declared: Mapping[str, Kind], names: Iterable[str]) -> set[str]:
    """The names of the maps (found or derived) that making the regions `names` reads."""
    wanted: set[str] = set()
    for name in used(declared, names):
        if name in BUILT_IN:
            wanted.add("subject" if name == "background" else name)
            continue
        match declared[name]:
            case Subject(per=None):
                wanted.add("subject")
            case Given():
                wanted.add(name)
    return wanted


def maps_used(declared: Mapping[str, Kind], names: Iterable[str]) -> set[str]:
    """The names of the maps of the photo that making the regions `names` reads and the caller must supply: `subject`
    for the built-in `subject` and `background` and for a `Subject` without `per`, `depth` for the built-in `depth`
    (so for a `Band` or `Bands` of it, however indirectly), and a `Given` region's own name. Not `light`, which is made
    from the picture itself (see `derived_maps`)."""
    return _wanted(declared, names) - DERIVED.keys()


def derived_maps(declared: Mapping[str, Kind], names: Iterable[str]) -> set[str]:
    """The names of the maps made from the working picture (`DERIVED`) that making the regions `names` reads: `light`
    for the built-in `light`, however indirectly."""
    return _wanted(declared, names) & DERIVED.keys()


def needs_finder(declared: Mapping[str, Kind], names: Iterable[str]) -> bool:
    """Whether making the regions `names` needs a finder to ask about pieces: one of the regions `used` is a `Subject`
    with `per`."""
    return any(
        name not in BUILT_IN and isinstance(kind := declared[name], Subject) and kind.per is not None
        for name in used(declared, names)
    )


def resolve(
    declared: Mapping[str, Kind],
    shape: tuple[int, int],
    *,
    maps: Mapping[str, np.ndarray],
    find: BoxFinder | None = None,
) -> Regions:
    """The regions `declared` (name -> kind) make over a picture of `shape` (height, width), plus the built-in
    `subject`, `background`, `depth` and `light`, built when asked for (`regions[name]`).

    `maps` are the maps of the picture by name (`subject`, `depth`, a `Given` region's name, and the `DERIVED` ones such
    as `light`), each float `(height, width)` in 0..1: the ones `maps_used` and `derived_maps` say the regions in use
    read (this module never loads a picture or a model); the caller passes maps of the picture's `(height, width)`,
    finite and within 0..1 (the runner checks them). `find` finds the subject inside a box of the picture (see
    `BoxFinder`), once for each piece of a `Subject` with `per`, its answers checked here (`check_map`); None when there
    is none.

    Every reference is checked here, for all declared regions, whether asked for or not (see `check`). RecipeError
    naming the region and the valid names."""
    check(declared)
    return Regions(declared, shape, maps, find)
