"""Outlines: SVG path data and SVG files turned into polygons in px, ready to fill (`ops.raster`).

`path_outlines` reads a bare path (`"M 0.1 0.2 C ... Z"`: lines, Béziers, arcs; M L H V C S Q T A Z, absolute and
relative) whose coordinates are shares of a frame. `read_svg` reads an SVG file from outside and places its shapes in a
frame. The SVG itself is read by `svgelements` (MIT); this module guards the way in, because a file is untrusted.

Promises:
- A path's coordinates are shares of the frame: the unit square is stretched to the frame, so an arc `A 0.1 0.1` is a
  circle only in a square frame. Curves are replaced by chords that stay within `_FLATNESS` px of them.
- An SVG shape is a `rect`, `circle`, `ellipse`, `polygon`, `polyline` or `path`, in document order (a `use` is drawn
  where it is used). Fill and stroke colours, opacity and strokes are ignored; every outline is closed, as filling does;
  the `fill-rule` is kept per shape. Lines, text, images, scripts and everything else draw nothing. Nothing is fetched
  or run.
- The file is placed by its `viewBox` and `preserveAspectRatio` (default `xMidYMid meet`: fitted inside, centred) into
  the frame; without a `viewBox` its `width` and `height` are the view box. A `transform` on the root is refused.
- Refused with a `RecipeError`: a file over `max_kilobytes`; one declaring XML entities (a DOCTYPE with an internal
  subset: the way of the billion laughs and of external entities, never expanded or followed); one with more than
  `max_shapes` shapes, counting what each `use` repeats before anything is expanded; nesting deeper than
  `_MAX_DEPTH`; XML that is not well formed; no shape; and a curve or point far outside the frame.

Dependency direction: imports `ops` and `errors` only.
"""

import io
import math
import re
from dataclasses import dataclass
from pathlib import Path
from xml.parsers import expat

import numpy as np
from svgelements import SVG, Arc, Close, CubicBezier, Line, Matrix, Move, QuadraticBezier, Shape, SimpleLine, Viewbox
from svgelements import Path as SvgPath

from bullterrier_image.errors import RecipeError
from bullterrier_image.ops.raster import FillRule

__all__ = ["SvgShape", "path_outlines", "read_svg", "svg_bytes"]

_FLATNESS = 0.05  # constant: chords stay within a twentieth of a pixel of a curve, far below what a pixel shows
_MAX_CHORDS = 1024  # constant: the most chords one curve may need; a curve needing more reaches absurdly far outside
_FAR = 1e7  # constant: no corner or control point may be farther than this from the origin, px (10^7 is 3 orders more
# than any picture side), which keeps every coordinate exact enough to fill
_MAX_DEPTH = (
    64  # constant: the deepest nesting of elements (and of `use` inside `use`); drawings stay under 20, and the
)
# reader below recurses once per level
_SHAPES = frozenset({"rect", "circle", "ellipse", "polygon", "polyline", "path"})
_SVG_NAMESPACE = "http://www.w3.org/2000/svg"
_PATH_CHARACTERS = re.compile(r"[MmLlHhVvCcSsQqTtAaZz0-9eE+\-.,\s]*")

Box = tuple[float, float, float, float]


@dataclass(frozen=True)
class SvgShape:
    """One closed shape of an SVG file: its `id` ('' if it has none), its `outlines` (`(n, 2)` arrays of corners in px,
    each closed by itself) and the fill `rule` that decides what is inside them."""

    id: str
    outlines: list[np.ndarray]
    rule: FillRule


def _checked(points: np.ndarray, what: str) -> np.ndarray:
    """`points` (px) if they are all finite and not absurdly far out, else RecipeError."""
    if not np.all(np.abs(points) <= _FAR):
        raise RecipeError(f"{what}: a coordinate is not finite or reaches too far outside the frame (over {_FAR:g} px)")
    return points


def _cubic(points: np.ndarray, what: str) -> np.ndarray:
    """The chords standing for the cubic Bézier through the four control `points` `(4, 2)`, as the `(n, 2)` corners
    after the first, `n` chosen so the curve is within `_FLATNESS` px of them: the distance of a curve to its chord
    is at most 1/8 of the curve's largest second derivative divided by n^2, and that is at most 6 times the larger of
    the two second differences of the control points."""
    bend = max(np.hypot(*(points[0] - 2 * points[1] + points[2])), np.hypot(*(points[1] - 2 * points[2] + points[3])))
    chords = max(1, math.ceil(math.sqrt(6 * bend / 8 / _FLATNESS)))
    if chords > _MAX_CHORDS:
        raise RecipeError(f"{what}: a curve reaches too far outside the frame (it would need {chords} chords)")
    t = (np.arange(1, chords + 1) / chords)[:, None]
    return (
        (1 - t) ** 3 * points[0] + 3 * (1 - t) ** 2 * t * points[1] + 3 * (1 - t) * t**2 * points[2] + t**3 * points[3]
    )


def _outlines(segments: list, affine: np.ndarray, what: str) -> list[np.ndarray]:
    """The outlines of svgelements `segments`, one for each subpath, with every point moved by the 2 x 3 `affine`
    (the segments' coordinates to px). Arcs and quadratic curves become cubic ones, so an `affine` that stretches
    stretches them exactly."""

    def px(*points: object) -> np.ndarray:
        xy = np.array([[p.x, p.y] for p in points], float)  # type: ignore[attr-defined,type-var]
        return _checked(xy @ affine[:, :2].T + affine[:, 2], what)

    outlines: list[list[np.ndarray]] = []
    current: list[np.ndarray] | None = None
    for segment in segments:
        if isinstance(segment, Move):
            current = [px(segment.end)]
            outlines.append(current)
            continue
        if isinstance(segment, Close):
            current = None
            continue
        if current is None:  # drawing goes on from where a closed subpath began
            current = [px(segment.start)]
            outlines.append(current)
        if isinstance(segment, Line):
            current.append(px(segment.end))
        elif isinstance(segment, QuadraticBezier):
            start, control, end = px(segment.start, segment.control, segment.end)
            current.append(
                _cubic(np.array([start, start + 2 / 3 * (control - start), end + 2 / 3 * (control - end), end]), what)
            )
        elif isinstance(segment, CubicBezier):
            current.append(_cubic(px(segment.start, segment.control1, segment.control2, segment.end), what))
        elif isinstance(segment, Arc):
            curves = list(segment.as_cubic_curves())
            current.extend(_cubic(px(c.start, c.control1, c.control2, c.end), what) for c in curves)
            if not curves:  # an arc of no length or no radius is the straight line it degenerates to
                current.append(px(segment.end))
    return [np.concatenate(parts) for parts in outlines]


def path_outlines(d: str, box: Box) -> list[np.ndarray]:
    """The outlines (px, `(n, 2)` arrays, one for each subpath, not closed by a repeated corner) of the SVG path data
    `d`, whose coordinates are shares of `box` = (left, top, right, bottom) in px. RecipeError when `d` is empty, does
    not start with M, has a character or number that is not path data, or reaches far outside the frame."""
    if not d.strip():
        raise RecipeError("path data is empty: write it like 'M 0.1 0.1 L 0.9 0.1 L 0.5 0.9 Z'")
    if not d.lstrip().startswith(("M", "m")):
        raise RecipeError(f"path data must start with M (a move to the first point), got {d!r}")
    if not _PATH_CHARACTERS.fullmatch(d):
        stray = next(c for c in d if not _PATH_CHARACTERS.fullmatch(c))
        raise RecipeError(f"path data {d!r} has {stray!r}, which is no path command or number")
    try:
        segments = list(SvgPath(d))
    except ValueError:
        raise RecipeError(
            f"path data {d!r} is not valid SVG path data (a command lacks or has too many numbers)"
        ) from None
    left, top, right, bottom = box
    affine = np.array([[right - left, 0, left], [0, bottom - top, top]], float)
    return _outlines(segments, affine, f"path data {d!r}")


def svg_bytes(file: str | Path, max_kilobytes: int) -> bytes:
    """The content of the SVG `file`; RecipeError naming the file when it cannot be read or is over `max_kilobytes`
    (the size is judged before the file is read into memory)."""
    path = Path(file)
    try:
        size = path.stat().st_size
        if size > max_kilobytes * 1024:
            raise RecipeError(
                f"svg file {path.name!r} is {size / 1024:.0f} KB, over the limit max_kilobytes = {max_kilobytes}"
            )
        return path.read_bytes()
    except OSError as error:
        raise RecipeError(f"svg file {path.name!r} cannot be read: {error.strerror or error}") from None


@dataclass
class _Node:
    """An element as the guard sees it: whether it is a shape, a `defs`, what it `use`s, and what it holds."""

    tag: str
    id: str
    uses: str
    children: list["_Node"]


def _tree(data: bytes, what: str) -> _Node:
    """The elements of the XML `data` as a tree, parsed by expat with entity declarations refused. RecipeError when the
    XML is not well formed, declares entities, is deeper than `_MAX_DEPTH`, or its root is no `svg` element."""
    parser = expat.ParserCreate(namespace_separator=" ")
    root = _Node("", "", "", [])
    open_nodes = [root]

    def doctype(_name: str, _system: str | None, _public: str | None, internal: bool) -> None:
        if internal:
            raise RecipeError(f"{what} declares XML entities (a DOCTYPE with an internal subset), which are refused")

    def start(name: str, attributes: dict[str, str]) -> None:
        if len(open_nodes) > _MAX_DEPTH:
            raise RecipeError(f"{what} is nested deeper than {_MAX_DEPTH} levels (deep nesting is refused)")
        space, _, local = name.rpartition(" ")
        link = attributes.get("href") or attributes.get("http://www.w3.org/1999/xlink href") or ""
        node = _Node(
            local if space == _SVG_NAMESPACE else "",
            attributes.get("id", ""),
            link[1:] if link.startswith("#") else "",
            [],
        )
        open_nodes[-1].children.append(node)
        open_nodes.append(node)

    parser.StartDoctypeDeclHandler = doctype
    parser.StartElementHandler = start
    parser.EndElementHandler = lambda name: open_nodes.pop()
    try:
        parser.Parse(data, True)
    except expat.ExpatError as error:
        raise RecipeError(f"{what} is not well-formed XML: {expat.ErrorString(error.code)}") from None
    if not root.children or root.children[0].tag != "svg":
        raise RecipeError(f"{what} is not an svg file (its root element is not <svg> in the SVG namespace)")
    return root.children[0]


def _shape_count(root: _Node, what: str) -> int:
    """How many shapes drawing `root` makes, counting each `use` as the shapes of what it uses, before anything is
    expanded. RecipeError when a `use` contains itself or uses are nested deeper than `_MAX_DEPTH`."""
    by_id: dict[str, _Node] = {}
    stack = [root]
    while stack:
        node = stack.pop()
        if node.id:
            by_id.setdefault(node.id, node)
        stack += node.children
    sizes: dict[int, int] = {}
    working: set[int] = set()

    def size(node: _Node, depth: int) -> int:
        """The shapes in `node` and below, with what its uses repeat (what a `use` of it would draw)."""
        if depth > _MAX_DEPTH:
            raise RecipeError(f"{what} nests uses deeper than {_MAX_DEPTH} levels (deep nesting is refused)")
        if id(node) in working:
            raise RecipeError(f"{what} has a use that contains itself")
        if id(node) not in sizes:
            working.add(id(node))
            total = int(node.tag in _SHAPES) + sum(size(child, depth + 1) for child in node.children)
            if node.tag == "use" and node.uses in by_id:
                total += size(by_id[node.uses], depth + 1)
            working.discard(id(node))
            sizes[id(node)] = total
        return sizes[id(node)]

    def drawn(node: _Node) -> int:
        """The shapes drawn at `node`'s own place: nothing inside `defs`, unless something uses it."""
        if node.tag == "defs":
            return 0
        own = int(node.tag in _SHAPES) + sum(drawn(child) for child in node.children)
        return own + (size(by_id[node.uses], 1) if node.tag == "use" and node.uses in by_id else 0)

    return drawn(root)


def read_svg(file: str | Path, box: Box, *, max_kilobytes: int, max_shapes: int) -> list[SvgShape]:
    """The shapes of the SVG `file` in document order, placed in `box` = (left, top, right, bottom) px (see the module
    docstring for what a shape is and how the file is placed). RecipeError, naming the file and the limit and value,
    for everything the module docstring says is refused."""
    path = Path(file)
    what = f"svg file {path.name!r}"
    data = svg_bytes(path, max_kilobytes)
    count = _shape_count(_tree(data, what), what)
    if count > max_shapes:
        raise RecipeError(f"{what} has {count} shapes, over the limit max_shapes = {max_shapes}")
    document = SVG.parse(io.BytesIO(data), reify=True)
    if document.values.get("transform"):
        raise RecipeError(f"{what}: a transform on the root <svg> element is not supported (put it on a group)")
    view = document.viewbox
    if view is None:
        if "width" not in document.values or "height" not in document.values:
            raise RecipeError(f"{what} has no viewBox and no width and height, so it has no size to place")
        view = Viewbox(f"0 0 {document.width} {document.height}")
    elif not (view.width > 0 and view.height > 0):
        raise RecipeError(f"{what} has a viewBox without area: {view}")
    left, top, right, bottom = box
    # the library has already placed everything by the file's own width and height; move from there to the frame
    placed = Matrix(document.viewbox_transform) if document.viewbox is not None else Matrix()
    onto = Matrix(
        Viewbox.viewbox_transform(
            left, top, right - left, bottom - top, view.x, view.y, view.width, view.height, view.preserve_aspect_ratio
        )
    )
    m = ~placed * onto  # svgelements applies the left matrix first
    affine = np.array([[m.a, m.c, m.e], [m.b, m.d, m.f]], float)
    shapes = []
    for element in document.elements():
        if isinstance(element, Shape) and not isinstance(element, SimpleLine):
            rule: FillRule = "evenodd" if element.values.get("fill-rule") == "evenodd" else "nonzero"
            shapes.append(SvgShape(element.id or "", _outlines(element.segments(transformed=True), affine, what), rule))
    if not shapes:
        raise RecipeError(f"{what} has no shapes (it needs a rect, circle, ellipse, polygon, polyline or path)")
    if len(shapes) > max_shapes:
        raise RecipeError(f"{what} has {len(shapes)} shapes, over the limit max_shapes = {max_shapes}")
    return shapes
