"""Regions: which pixels of a picture a step is about, as a partition or a selection made from a declared kind.

A **partition** (`Partition`) gives every pixel to exactly one piece; the pieces are numbered 0..n-1 in *reading
order*: piece k comes before piece k+1 when its first pixel comes earlier in the raster scan (left to right along each
row, rows from the top). For a grid that is row after row; for any other kind it is a rule with no constants. A kind
that makes more pieces than a small picture has pixels yields only the pieces that have a pixel, so n can be smaller
than asked for. A **selection** (`Selection`) says how much each pixel is chosen, 0..1.

The kinds are models declared like an effect's parameters (`Grid`, `Shards`, `Pick`; `Edge` bends the borders of the
first two), so every field has a unit, a range and a description: `Shards.documentation()` lists them. A recipe or a
script declares them by name, and `resolve` turns the declarations into the regions of one picture, building only what
is asked for. `look_weights` turns a partition and a look for each piece into the weights that blend the looks.

Promises:
- Positions are shares of the *frame* (the picture, or the bounding box of the selection a kind is laid `fit` over) and
  lengths are shares of the picture's long side, so the same declaration cuts the same pieces at any resolution
  (a torn edge too: its noise is laid on the frame).
- The same declaration, picture size and seed give the same regions on any machine; no global random state is used.
- Regions are made once per `resolve` and handed out read-only.
- A declaration is checked when it is made (a value out of range) and when it is resolved (an unknown name, a name of
  the wrong kind of region, a cycle): a `RecipeError` naming the region and what is valid.

Dependency direction: imports `params`, `presets` and `ops`; the steps and the runner build on it.
"""

import math
from collections.abc import Callable, Iterable, Iterator, Mapping, Sequence
from dataclasses import dataclass
from typing import Annotated, Any, Literal

import numpy as np
from pydantic import BeforeValidator, field_validator, model_validator

from bullterrier_image.errors import RecipeError, SubjectNotFoundError
from bullterrier_image.ops.blur import gaussian_blur
from bullterrier_image.ops.noise import frame_fractal
from bullterrier_image.ops.voronoi import centres, nearest

from .params import EffectParams, param
from .presets import SHIPPED, read

__all__ = [
    "Edge",
    "Grid",
    "Partition",
    "Pick",
    "Regions",
    "Selection",
    "Shards",
    "as_partition",
    "check",
    "look_weights",
    "needs",
    "needs_subject",
    "resolve",
    "used",
]

BUILT_IN = ("subject", "background")

_HALF = 0.5  # constant: a pixel is chosen when more than half of it is (the hard edge of a selection)
_LACUNARITY = 2.0  # constant: each octave of fractal noise has twice the frequency of the one before (the standard)
_TRUNCATE = 4.0  # constant: a feather's Gaussian is cut off at 4 sigmas, as scipy does by default (error < 1e-4)
_HEX_ROWS = math.sqrt(3) / 2  # constant: the distance between rows of a hexagonal lattice, in units of its spacing
_MOST = 1000  # the most columns, rows, or shares of a grid


@dataclass(frozen=True, eq=False)
class Partition:
    """Every pixel in exactly one piece. `labels` is int32 `(height, width)`: the piece of each pixel, numbered
    0..`count`-1 in reading order (the module docstring defines it); every number occurs. Read-only when made by
    `resolve`."""

    labels: np.ndarray
    count: int


@dataclass(frozen=True, eq=False)
class Selection:
    """How much each pixel is chosen. `alpha` is float32 `(height, width)`, 0..1. Read-only when made by `resolve`."""

    alpha: np.ndarray


class _Model(EffectParams):
    """What the declarations share: a bad value is a `RecipeError` naming the kind, the field and the value."""

    def __init__(self, /, **values: Any) -> None:
        """The declaration of `values` (field name -> value, all optional); RecipeError naming each bad field, the
        constraint and the value, and listing the fields when an unknown one is given."""
        self._init_checked(type(self).__name__.lower(), **values)


class Edge(_Model):
    """How the borders between pieces run: straight, or torn like paper by bending the coordinates they cut."""

    style: Literal["straight", "torn"] = param(
        "straight",
        doc="'straight' cuts along the exact borders of the kind; 'torn' bends every border by smooth noise, so the "
        "pieces have ragged, wandering edges (the other edge fields only matter for 'torn').",
    )
    amount: float = param(
        0.01,
        unit="fraction",
        min=0,
        max=0.1,
        doc="How far a torn border strays, as a share of the picture's long side (0 to 0.1): the standard deviation "
        "of the shift. Raising it tears the edge wider; 0 is straight.",
    )
    scale: float = param(
        0.05,
        unit="fraction",
        min=0.01,
        max=1,
        doc="The size of the bends of a torn border, as a share of the picture's long side (0.01 to 1): the spacing "
        "of the coarsest noise. Raising it gives long gentle wanders, lowering it a jagged edge.",
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
        doc="The name of the partition (a `Grid` or `Shards` region) whose pieces are picked; it must be set.",
    )
    pieces: tuple[int, ...] = param(
        (0,),
        min_length=1,
        doc="The numbers of the pieces to choose, counted from 0 in reading order (top to bottom, then left to right): "
        "at least one. A number the partition has no piece for chooses nothing, so a recipe also works on a picture "
        "too small to have that many pieces.",
    )

    @field_validator("pieces")
    @classmethod
    def _numbers(cls, pieces: tuple[int, ...]) -> tuple[int, ...]:
        """Piece numbers count from 0."""
        if min(pieces) < 0:
            raise ValueError(f"piece numbers count from 0, got {list(pieces)}")
        return pieces


def _frozen(array: np.ndarray) -> np.ndarray:
    """`array`, read-only: regions are shared by every step that asks for them."""
    array.setflags(write=False)
    return array


def _partition(raw: np.ndarray) -> Partition:
    """The partition whose pieces are the distinct values of the integer array `raw` (any numbers, same value = same
    piece), renumbered 0..n-1 by where each is first met in the raster scan."""
    flat = raw.ravel()
    values, first = np.unique(flat, return_index=True)
    number = np.zeros(int(values.max()) + 1, np.int32)
    number[values[np.argsort(first)]] = np.arange(len(values), dtype=np.int32)
    return Partition(_frozen(number[raw]), len(values))


def as_partition(selection: Selection) -> Partition:
    """A hard-edged selection as a partition of the pixels it chooses (more than half) and those it does not; a
    selection that chooses all or none of the picture has one piece. Numbered in reading order like any partition."""
    return _partition((selection.alpha > _HALF).astype(np.int32))


def _coordinates(shape: tuple[int, int], edge: Edge) -> tuple[np.ndarray, np.ndarray]:
    """The `(x, y)` that every kind classifies, in px: the pixel centres, bent by noise for a torn `edge`. They
    broadcast against each other to `shape`."""
    x, y = centres(shape)
    if edge.style == "straight":
        return x, y
    long_side = max(shape)
    bend = [
        frame_fractal(
            shape,
            scale=edge.scale,
            octaves=edge.octaves,
            persistence=edge.persistence,
            lacunarity=_LACUNARITY,
            seed=[edge.seed, axis],
        )
        * (edge.amount * long_side)
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


def _type_of(name: str, declared: Mapping[str, Grid | Shards | Pick]) -> Literal["partition", "selection"] | None:
    """What the region `name` is, or None when no such region is declared or built in."""
    if name in BUILT_IN:
        return "selection"
    match declared.get(name):
        case Grid() | Shards():
            return "partition"
        case Pick():
            return "selection"
        case _:
            return None


def needs(
    owner: str,
    field: str,
    name: str,
    wanted: Literal["partition", "selection"] | None,
    declared: Mapping[str, Grid | Shards | Pick],
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


def _reference(kind: Grid | Shards | Pick) -> str | None:
    """The name of the region that `kind` is made from, if any."""
    return kind.of if isinstance(kind, Pick) else kind.fit


class Regions(Mapping[str, Partition | Selection]):
    """The regions of one picture by name: the built-in `subject` and `background` and every declared region. A
    region is made the first time it is asked for (with what it is made from), then kept; the subject finder runs at
    most once, and not at all when no region needs it."""

    def __init__(
        self,
        declared: Mapping[str, Grid | Shards | Pick],
        shape: tuple[int, int],
        subject: Callable[[], np.ndarray] | None,
    ) -> None:
        """Made by `resolve`, which checks `declared` first."""
        self._declared, self._shape, self._subject = declared, shape, subject
        self._made: dict[str, Partition | Selection] = {}

    def __getitem__(self, name: str) -> Partition | Selection:
        """The region `name`; KeyError when it is neither built in nor declared. SubjectNotFoundError when it needs
        the subject and there is no finder, or the finder's matte is not the picture's size; RecipeError when its
        `fit` selection chooses nothing in this picture."""
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
            case "subject":
                if self._subject is None:
                    raise SubjectNotFoundError(
                        "a region needs the subject of the picture, but no subject finder was given"
                    )
                matte = np.asarray(
                    self._subject(), np.float32
                ).view()  # a view: frozen alone, the finder's array untouched
                if matte.shape != self._shape:
                    raise SubjectNotFoundError(
                        f"the subject finder gave a matte of shape {matte.shape}, not {self._shape}"
                    )
                return Selection(_frozen(matte))
            case "background":
                return Selection(_frozen(1 - self.selection("subject").alpha))
        kind = self._declared[name]
        if isinstance(kind, Pick):
            labels = self.partition(kind.of).labels
            return Selection(_frozen(np.isin(labels, kind.pieces).astype(np.float32)))
        box, chosen = self._frame(name, kind.fit)
        x, y = _coordinates(self._shape, kind.edge)
        if isinstance(kind, Grid):
            left, top, right, bottom = box
            across = _borders(kind.columns, left, right)
            column = np.searchsorted(across, x, side="right")
            row = np.searchsorted(_borders(kind.rows, top, bottom), y, side="right")
            return _partition(row * (len(across) + 1) + column)
        return _partition(nearest(_sites(kind, self._shape, box, chosen), x, y)[1])

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


def check(declared: Mapping[str, Grid | Shards | Pick]) -> None:
    """RecipeError unless the declared regions are consistent: a `fit` names a selection, an `of` a partition, no name
    is one of the built-ins and regions do not refer to each other in a cycle. Builds nothing, needs no picture; the
    message names the region and lists the valid names."""
    for name, kind in declared.items():
        if name in BUILT_IN:
            raise RecipeError(f"region {name!r} is a built-in region ({', '.join(BUILT_IN)}) and cannot be declared")
        if isinstance(kind, Pick):
            needs(f"region {name!r}", "of", kind.of, "partition", declared)
        elif kind.fit is not None:
            needs(f"region {name!r}", "fit", kind.fit, "selection", declared)
    for start in declared:
        chain = [start]
        while (ref := _reference(declared[chain[-1]])) is not None and ref in declared:
            if ref in chain:
                loop = " -> ".join(repr(each) for each in [*chain[chain.index(ref) :], ref])
                raise RecipeError(f"regions refer to each other in a cycle: {loop}")
            chain.append(ref)


def used(declared: Mapping[str, Grid | Shards | Pick], names: Iterable[str]) -> set[str]:
    """The regions made when `names` (checked ones, as `check` leaves them) are asked for: those, and every region they
    are laid over or picked from, down to the built-in ones."""
    seen: set[str] = set()
    todo = list(names)
    while todo:
        name = todo.pop()
        if name not in seen:
            seen.add(name)
            ref = None if name in BUILT_IN else _reference(declared[name])
            todo += [] if ref is None else [ref]
    return seen


def needs_subject(declared: Mapping[str, Grid | Shards | Pick], names: Iterable[str]) -> bool:
    """Whether making the regions `names` needs the subject of the picture: one of the regions `used` is the built-in
    `subject` or `background`."""
    return not set(BUILT_IN).isdisjoint(used(declared, names))


def resolve(
    declared: Mapping[str, Grid | Shards | Pick],
    shape: tuple[int, int],
    *,
    subject: Callable[[], np.ndarray] | None,
) -> Regions:
    """The regions `declared` (name -> kind) make over a picture of `shape` (height, width), plus the built-in
    `subject` and `background`, built when asked for (`regions[name]`).

    `subject` finds the subject: a function returning its matte, float `(height, width)` in 0..1, called at most once
    and only if a region needs it (this module never loads a picture or a model); None when there is no finder.

    Every reference is checked here, for all declared regions, whether asked for or not (see `check`). RecipeError
    naming the region and the valid names."""
    check(declared)
    return Regions(declared, shape, subject)
