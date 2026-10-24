"""Tests of `core/outlines.py`: SVG path data and SVG files turned into outlines in px, placed in a frame.

Oracles written here: corners worked out by hand, the exact area of a circle and an ellipse, the distance from a Bézier
curve's own formula to the chords that stand for it, and placement worked out from the SVG specification's rule. The
hostile files are written to `tmp_path`; the network is closed for the whole module, so a reader that fetched a link
would fail.
"""

import math
import socket
from pathlib import Path

import numpy as np
import pytest

from bullterrier_image import RecipeError
from bullterrier_image.core.outlines import path_outlines, read_svg, svg_bytes
from bullterrier_image.ops.raster import coverage

ROOT = Path(__file__).resolve().parents[2]
BOX = (10.0, 20.0, 110.0, 70.0)  # left, top, right, bottom: 100 px wide, 50 px tall
SVG = 'xmlns="http://www.w3.org/2000/svg"'


@pytest.fixture(autouse=True)
def closed_network(monkeypatch):
    """No test here may open a connection: a reader that fetched a link or an external entity would fail."""

    def refuse(*args, **kwargs):
        raise AssertionError("the reader tried to open a network connection")

    monkeypatch.setattr(socket, "socket", refuse)
    monkeypatch.setattr(socket, "create_connection", refuse)


def area(outlines: list[np.ndarray], size: tuple[int, int], rule: str = "nonzero") -> float:
    """The area the outlines cover on a picture of `size`, by pixel coverage."""
    return float(coverage(outlines, size, rule, 8).sum())  # type: ignore[arg-type]


def extent(outlines: list[np.ndarray]) -> tuple[float, float, float, float]:
    """(left, top, right, bottom) of all the outlines' corners."""
    points = np.concatenate(outlines)
    return (*points.min(axis=0), *points.max(axis=0))


# ---- path data -----------------------------------------------------------------------------------------------------


def test_coordinates_are_shares_of_the_frame():
    (outline,) = path_outlines("M 0 0 L 1 0 L 1 1 L 0.5 1 Z", BOX)
    np.testing.assert_allclose(outline, [[10, 20], [110, 20], [110, 70], [60, 70]])


@pytest.mark.parametrize(
    ("relative", "absolute"),
    [
        ("m 0.1 0.1 l 0.4 0 l 0 0.4 z", "M 0.1 0.1 L 0.5 0.1 L 0.5 0.5 Z"),
        ("M 0.1 0.1 h 0.4 v 0.4 H 0.1 Z", "M 0.1 0.1 L 0.5 0.1 L 0.5 0.5 L 0.1 0.5 Z"),
        ("M.1.1L.5.1.5.5Z", "M 0.1 0.1 L 0.5 0.1 L 0.5 0.5 Z"),
        ("M 0.1,0.1 c 0.1 0 0.3 0 0.4 0.4 z", "M 0.1 0.1 C 0.2 0.1 0.4 0.1 0.5 0.5 Z"),
        (
            "M 0.1 0.1 C 0.2 0.1 0.4 0.1 0.5 0.5 s 0.2 0.2 0.3 0 Z",
            "M 0.1 0.1 C 0.2 0.1 0.4 0.1 0.5 0.5 C 0.6 0.9 0.7 0.7 0.8 0.5 Z",
        ),
        ("M 0.1 0.5 Q 0.3 0.1 0.5 0.5 T 0.9 0.5", "M 0.1 0.5 Q 0.3 0.1 0.5 0.5 Q 0.7 0.9 0.9 0.5"),
        ("M1e-1 .1l4e-1 0", "M 0.1 0.1 L 0.5 0.1"),
    ],
    ids=[
        "relative lines",
        "H and V",
        "no separators",
        "relative cubic",
        "smooth cubic",
        "smooth quadratic",
        "exponent",
    ],
)
def test_relative_smooth_and_compact_commands_mean_what_the_absolute_ones_do(relative, absolute):
    got, want = path_outlines(relative, BOX), path_outlines(absolute, BOX)
    assert len(got) == len(want)
    for one, other in zip(got, want, strict=True):
        np.testing.assert_allclose(one, other, atol=1e-9)


def test_an_arc_circle_has_the_area_of_a_circle_and_in_a_wide_frame_that_of_an_ellipse():
    circle = "M 0.25 0.5 A 0.25 0.25 0 1 0 0.75 0.5 A 0.25 0.25 0 1 0 0.25 0.5 Z"
    square = path_outlines(circle, (0.0, 0.0, 200.0, 200.0))
    assert area(square, (200, 200)) == pytest.approx(math.pi * 50**2, rel=2e-3)
    wide = path_outlines(circle, (0.0, 0.0, 400.0, 100.0))  # the unit square is stretched to the frame
    assert area(wide, (100, 400)) == pytest.approx(math.pi * 100 * 25, rel=2e-3)


@pytest.mark.parametrize(
    ("large", "sweep", "up", "down"),
    [(0, 1, 10, 0), (1, 1, 90, 0), (0, 0, 0, 10), (1, 0, 0, 90)],
    ids=["short clockwise", "long clockwise", "short anticlockwise", "long anticlockwise"],
)
def test_arc_flags_choose_between_the_four_arcs_through_two_points(large, sweep, up, down):
    """From (0.2, 0.5) to (0.8, 0.5) with radius 0.5 in a 100 px square: the chord is 60 px, so the circle's centre
    is 40 px above or below its middle, and the arc rises (clockwise, sweep 1) or sinks (sweep 0) by 50 - 40 = 10 px the
    short way and 50 + 40 = 90 px the long way (SVG 1.1, F.6.2)."""
    (outline,) = path_outlines(f"M 0.2 0.5 A 0.5 0.5 0 {large} {sweep} 0.8 0.5", (0.0, 0.0, 100.0, 100.0))
    assert 50 - outline[:, 1].min() == pytest.approx(up, abs=0.2)
    assert outline[:, 1].max() - 50 == pytest.approx(down, abs=0.2)


CURVES = {
    "a cubic": ("M 0.05 0.9 C 0.1 -0.4 0.9 1.4 0.95 0.1", [[0.05, 0.9], [0.1, -0.4], [0.9, 1.4], [0.95, 0.1]]),
    "a quadratic": ("M 0.05 0.9 Q 0.5 -0.6 0.95 0.1", [[0.05, 0.9], [0.5, -0.6], [0.95, 0.1]]),
}


@pytest.mark.parametrize(("d", "controls"), CURVES.values(), ids=CURVES.keys())
def test_a_curve_stays_within_a_tenth_of_a_pixel_of_the_curve(d, controls):
    """The curve's own Bernstein form, written here, against the chords that stand for it."""
    (outline,) = path_outlines(d, (0.0, 0.0, 400.0, 300.0))
    p = np.array(controls) * (400, 300)
    t = np.linspace(0, 1, 4001)[:, None]
    n = len(p) - 1
    curve = sum(math.comb(n, k) * (1 - t) ** (n - k) * t**k * p[k] for k in range(n + 1))
    a, b = outline[:-1], outline[1:]
    ab = b - a
    along = np.clip(np.einsum("ncd,cd->nc", curve[:, None, :] - a[None], ab) / np.einsum("cd,cd->c", ab, ab), 0, 1)
    nearest = a[None] + along[..., None] * ab[None]
    worst = np.hypot(*(curve[:, None, :] - nearest).transpose(2, 0, 1)).min(axis=1).max()
    assert worst < 0.1
    np.testing.assert_allclose(outline[0], p[0])
    np.testing.assert_allclose(outline[-1], p[-1])


def test_drawing_after_a_close_goes_on_from_where_the_closed_subpath_began():
    closed, second = path_outlines("M 0.1 0.1 L 0.5 0.1 L 0.5 0.5 Z L 0.9 0.9", BOX)
    np.testing.assert_allclose(second, [closed[0], [100, 65]])


def test_every_subpath_is_an_outline_of_its_own():
    outlines = path_outlines("M 0 0 L 0.5 0 L 0.5 0.5 Z M 0.6 0.6 L 1 0.6 L 1 1 Z", BOX)
    assert [len(outline) for outline in outlines] == [3, 3]


@pytest.mark.parametrize(
    ("d", "mentions"),
    [
        pytest.param("", "empty", id="nothing"),
        pytest.param("L 0.5 0.5", "start with M", id="no start"),
        pytest.param("M 0 0 L foo", "foo", id="not a number"),
        pytest.param("M 0 0 X 1 1", "X", id="unknown command"),
        pytest.param("M 0 0 L 1", "L", id="missing coordinate"),
        pytest.param("M 0 0 L 1e999 0", "finite", id="infinite"),
        pytest.param("M 0 0 C 0 0 1e9 1e9 0 0", "too far", id="curve far beyond the frame"),
        pytest.param("M 0 0 C 0 0 5000 5000 0 0", "too far", id="curve needing more chords than allowed"),
        pytest.param("M 0 0 C 0 0 1e999 1e999 0 0", "finite", id="curve with infinite control points"),
    ],
)
def test_bad_path_data_is_refused_naming_the_problem(d, mentions):
    with pytest.raises(RecipeError, match=mentions):
        path_outlines(d, BOX)


# ---- SVG files -----------------------------------------------------------------------------------------------------


def write(tmp_path: Path, text: str | bytes, name: str = "shapes.svg") -> Path:
    path = tmp_path / name
    path.write_bytes(text.encode() if isinstance(text, str) else text)
    return path


def read(path: Path, box=(0.0, 0.0, 200.0, 200.0), *, max_kilobytes=1024, max_shapes=100):
    return read_svg(path, box, max_kilobytes=max_kilobytes, max_shapes=max_shapes)


def test_a_shape_is_placed_by_the_view_box_inside_the_frame_centred_by_default(tmp_path):
    path = write(tmp_path, f'<svg {SVG} viewBox="0 0 100 50"><rect id="r" x="0" y="0" width="100" height="50"/></svg>')
    (shape,) = read(path)  # a 2:1 picture fitted inside a square frame: 200 wide, 100 tall, centred
    assert extent(shape.outlines) == pytest.approx((0, 50, 200, 150))
    assert shape.id == "r"


@pytest.mark.parametrize(
    ("aspect", "box"),
    [
        ("xMinYMin meet", (0, 0, 200, 100)),
        ("xMaxYMax meet", (0, 100, 200, 200)),
        ("none", (0, 0, 200, 200)),
        ("xMidYMid slice", (-100, 0, 300, 200)),
        ("xMinYMid slice", (0, 0, 400, 200)),
    ],
)
def test_preserve_aspect_ratio_places_the_view_box_as_the_svg_specification_says(tmp_path, aspect, box):
    text = f'<svg {SVG} viewBox="0 0 100 50" preserveAspectRatio="{aspect}"><rect width="100" height="50"/></svg>'
    (shape,) = read(write(tmp_path, text))
    assert extent(shape.outlines) == pytest.approx(box)


def test_the_view_box_origin_and_a_frame_that_does_not_start_at_zero_are_honoured(tmp_path):
    text = f'<svg {SVG} viewBox="-10 -20 50 50"><rect x="0" y="0" width="10" height="10"/></svg>'
    (shape,) = read(write(tmp_path, text), box=(100.0, 40.0, 200.0, 140.0))  # 100 x 100: scale 2
    assert extent(shape.outlines) == pytest.approx((120, 80, 140, 100))


def test_without_a_view_box_the_width_and_height_are_the_view_box(tmp_path):
    text = f'<svg {SVG} width="50" height="25"><rect width="50" height="25"/></svg>'
    (shape,) = read(write(tmp_path, text), box=(0.0, 0.0, 100.0, 100.0))
    assert extent(shape.outlines) == pytest.approx((0, 25, 100, 75))


@pytest.mark.parametrize(
    ("text", "mentions"),
    [
        (f"<svg {SVG}><rect width='5' height='5'/></svg>", "viewBox"),
        (f'<svg {SVG} viewBox="0 0 100 50" transform="scale(2)"><rect width="5" height="5"/></svg>', "transform"),
        (f'<svg {SVG} viewBox="0 0 0 50"><rect width="5" height="5"/></svg>', "viewBox"),
        ("<html><body/></html>", "svg"),
        (f"<svg {SVG} viewBox='0 0 9 9'><rect", "XML"),
        (f"<svg {SVG} viewBox='0 0 9 9'></svg>", "no shapes"),
    ],
    ids=["no size", "root transform", "empty view box", "not an svg", "not well formed", "nothing to draw"],
)
def test_a_file_that_is_no_usable_svg_is_refused_naming_the_problem(tmp_path, text, mentions):
    with pytest.raises(RecipeError, match=mentions):
        read(write(tmp_path, text))


def test_a_missing_file_is_refused_naming_it(tmp_path):
    with pytest.raises(RecipeError, match=r"nowhere\.svg"):
        read(tmp_path / "nowhere.svg")


def test_nested_groups_transforms_and_every_kind_of_shape_come_out_in_document_order(tmp_path):
    text = f"""<svg {SVG} viewBox="0 0 100 100">
      <g transform="translate(10 10)"><g transform="scale(2)"><rect id="rect" x="1" y="1" width="3" height="4"/></g></g>
      <circle id="circle" cx="50" cy="50" r="10"/>
      <ellipse id="ellipse" cx="50" cy="80" rx="20" ry="5"/>
      <polygon id="polygon" points="0,0 10,0 10,10"/>
      <polyline id="polyline" points="80,80 90,80 90,90"/>
      <path d="M 5 5 h 4 v 4 z"/>
      <line x1="0" y1="0" x2="50" y2="50" stroke="black"/>
      <text x="5" y="5">words</text>
      <rect id="hidden" width="9" height="9" display="none"/>
    </svg>"""
    shapes = read(write(tmp_path, text), box=(0.0, 0.0, 100.0, 100.0))
    assert [shape.id for shape in shapes] == ["rect", "circle", "ellipse", "polygon", "polyline", ""]
    assert extent(shapes[0].outlines) == pytest.approx((12, 12, 18, 20))
    # (inscribed chords within 0.05 px lose up to 2/3 of that along the border: 0.7 % of a circle 10 px across)
    assert area(shapes[1].outlines, (100, 100)) == pytest.approx(math.pi * 100, rel=1e-2)
    assert area(shapes[2].outlines, (100, 100)) == pytest.approx(math.pi * 20 * 5, rel=1e-2)


def test_a_rotation_turns_the_shape_about_its_centre(tmp_path):
    text = f"""<svg {SVG} viewBox="0 0 100 100"><g transform="rotate(90 50 50)">
      <rect x="10" y="40" width="30" height="10"/></g></svg>"""
    (shape,) = read(write(tmp_path, text), box=(0.0, 0.0, 100.0, 100.0))
    assert extent(shape.outlines) == pytest.approx((50, 10, 60, 40))  # (x, y) goes to (100 - y, x)


def test_shapes_in_defs_that_nothing_uses_are_not_counted(tmp_path):
    defs = "<defs>" + '<rect width="1" height="1"/>' * 10 + "</defs>"
    path = write(tmp_path, f'<svg {SVG} viewBox="0 0 10 10">{defs}<rect id="drawn" width="5" height="5"/></svg>')
    assert [shape.id for shape in read(path, max_shapes=3)] == ["drawn"]


def test_the_fill_rule_is_read_and_inherited_from_the_group(tmp_path):
    text = f"""<svg {SVG} viewBox="0 0 10 10"><g fill-rule="evenodd"><path id="a" d="M0 0h4v4z"/></g>
      <path id="b" d="M0 0h4v4z"/><path id="c" fill-rule="evenodd" d="M0 0h4v4z"/></svg>"""
    assert [shape.rule for shape in read(write(tmp_path, text))] == ["evenodd", "nonzero", "evenodd"]


def test_a_shape_with_a_hole_leaves_the_hole_unfilled(tmp_path):
    ring = "M 10 50 a 40 40 0 1 0 80 0 a 40 40 0 1 0 -80 0 Z M 30 50 a 20 20 0 1 1 40 0 a 20 20 0 1 1 -40 0 Z"
    text = f'<svg {SVG} viewBox="0 0 100 100"><path fill-rule="evenodd" d="{ring}"/></svg>'
    (shape,) = read(write(tmp_path, text), box=(0.0, 0.0, 100.0, 100.0))
    assert area(shape.outlines, (100, 100), shape.rule) == pytest.approx(math.pi * (40**2 - 20**2), rel=3e-3)


def test_a_used_shape_is_drawn_where_it_is_used_and_not_where_it_is_defined(tmp_path):
    text = f"""<svg {SVG} viewBox="0 0 100 100"><defs><rect id="r" width="10" height="10"/></defs>
      <use href="#r" x="30" y="40"/><use href="#r" x="60" y="60"/></svg>"""
    shapes = read(write(tmp_path, text), box=(0.0, 0.0, 100.0, 100.0))
    assert [extent(shape.outlines) for shape in shapes] == [(30, 40, 40, 50), (60, 60, 70, 70)]


def test_a_realistic_file_gives_its_shapes_inside_the_frame_with_the_hole_and_the_transforms_applied():
    frame = (0.0, 0.0, 600.0, 600.0)  # the 3:2 view box is fitted inside: 600 x 400 centred, scale 2, 100 px down
    shapes = {shape.id: shape for shape in read(ROOT / "examples" / "assets" / "mosaic.svg", frame)}
    assert list(shapes) == [
        "hill",
        "sun",
        "ring",
        "leaf",
        "vein-left",
        "vein-right",
        "tile",
        "star",
        "pebble",
        "wave",
        "drop",
    ]
    sun = extent(shapes["sun"].outlines)  # the group moves the circle by (-10, 5) before the view box scales it
    assert sun == pytest.approx(
        (2 * (255 - 10 - 28), 100 + 2 * (48 + 5 - 28), 2 * (255 - 10 + 28), 100 + 2 * (48 + 5 + 28)), abs=0.5
    )
    ring = shapes["ring"]
    assert ring.rule == "evenodd"
    assert area(ring.outlines, (600, 600), "evenodd") == pytest.approx(2 * 2 * math.pi * (32**2 - 16**2), rel=3e-3)
    leaf = extent(shapes["leaf"].outlines)  # rotate(-14) and scale(0.8) bend and shrink it: it stays inside the picture
    assert 0 < leaf[0] < leaf[2] < 600
    assert 100 < leaf[1] < leaf[3] < 500
    assert area(shapes["pebble"].outlines, (600, 600)) == pytest.approx(math.pi * 4 * 20 * 9, rel=3e-3)


# ---- hostile files -------------------------------------------------------------------------------------------------

BILLION_LAUGHS = f"""<?xml version="1.0"?>
<!DOCTYPE svg [
  <!ENTITY a "aaaaaaaaaa">
  <!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;">
  <!ENTITY c "&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;">
  <!ENTITY d "&c;&c;&c;&c;&c;&c;&c;&c;&c;&c;">
  <!ENTITY e "&d;&d;&d;&d;&d;&d;&d;&d;&d;&d;">
]>
<svg {SVG} viewBox="0 0 10 10"><rect id="&e;" width="5" height="5"/></svg>"""

EXTERNAL_ENTITY = f"""<?xml version="1.0"?>
<!DOCTYPE svg [ <!ENTITY secret SYSTEM "file:///etc/passwd"> <!ENTITY remote SYSTEM "http://127.0.0.1:9/x"> ]>
<svg {SVG} viewBox="0 0 10 10"><rect id="&secret;&remote;" width="5" height="5"/></svg>"""


@pytest.mark.parametrize("text", [BILLION_LAUGHS, EXTERNAL_ENTITY], ids=["billion laughs", "external entity"])
def test_a_document_that_declares_entities_is_refused_before_anything_is_expanded_or_fetched(tmp_path, text):
    with pytest.raises(RecipeError, match="entit"):
        read(write(tmp_path, text))


@pytest.mark.parametrize("encoding", ["utf-16", "utf-8"])
def test_entities_are_found_whatever_the_encoding(tmp_path, encoding):
    text = BILLION_LAUGHS.replace('<?xml version="1.0"?>\n', "").replace("<?xml", "<?xml")
    with pytest.raises(RecipeError, match="entit"):
        read(write(tmp_path, text.encode(encoding)))


def test_a_document_type_without_declarations_is_harmless_and_its_dtd_is_never_fetched(tmp_path):
    text = (
        '<?xml version="1.0"?><!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" '
        '"http://127.0.0.1:9/DTD/svg11.dtd">'
        f'<svg {SVG} viewBox="0 0 10 10"><rect width="5" height="5"/></svg>'
    )
    assert len(read(write(tmp_path, text))) == 1


def test_scripts_and_links_are_ignored_and_never_followed(tmp_path):
    text = f"""<svg {SVG} xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 10 10" onload="alert(1)">
      <script>fetch('http://127.0.0.1:9/')</script>
      <image href="http://127.0.0.1:9/x.png" width="5" height="5"/>
      <image xlink:href="file:///etc/passwd" width="5" height="5"/>
      <a href="http://127.0.0.1:9/"><rect id="inside-a-link" width="5" height="5"/></a>
      <use href="http://127.0.0.1:9/other.svg#shape"/>
      <style>@import url(http://127.0.0.1:9/x.css);</style>
    </svg>"""
    assert [shape.id for shape in read(write(tmp_path, text))] == ["inside-a-link"]


def test_a_file_over_the_size_limit_is_refused_naming_the_limit_and_its_size(tmp_path):
    path = write(tmp_path, f'<svg {SVG} viewBox="0 0 10 10"><rect width="5" height="5"/></svg>' + " " * 3000)
    with pytest.raises(RecipeError, match=r"max_kilobytes.*2"):
        read(path, max_kilobytes=2)
    assert len(read(path, max_kilobytes=4)) == 1


@pytest.mark.parametrize(("size", "refused"), [(2000, False), (2048, False), (2049, True)], ids=lambda v: str(v))
def test_the_size_limit_is_in_kilobytes_of_1024_bytes_and_the_limit_itself_is_allowed(tmp_path, size, refused):
    text = f'<svg {SVG} viewBox="0 0 10 10"><rect width="5" height="5"/></svg>'
    path = write(tmp_path, text + " " * (size - len(text)))
    if refused:
        with pytest.raises(RecipeError, match="max_kilobytes"):
            read(path, max_kilobytes=2)
    else:
        assert len(read(path, max_kilobytes=2)) == 1


def test_a_file_with_more_shapes_than_the_limit_is_refused_naming_the_limit_and_the_count(tmp_path):
    rects = '<rect width="1" height="1"/>' * 7
    path = write(tmp_path, f'<svg {SVG} viewBox="0 0 10 10">{rects}</svg>')
    with pytest.raises(RecipeError, match=r"max_shapes.*6.*7|7.*max_shapes.*6"):
        read(path, max_shapes=6)
    assert len(read(path, max_shapes=7)) == 7


def test_uses_that_multiply_shapes_are_counted_before_they_are_expanded(tmp_path):
    """Ten groups each using the one before ten times: 10^10 shapes from a tiny file, as in the billion laughs."""
    levels = "".join(f'<g id="g{level}">' + f'<use href="#g{level - 1}"/>' * 10 + "</g>" for level in range(1, 11))
    defs = f'<defs><rect id="g0" width="1" height="1"/>{levels}</defs>'
    text = f'<svg {SVG} viewBox="0 0 10 10">{defs}<use href="#g10"/></svg>'
    with pytest.raises(RecipeError, match="max_shapes"):
        read(write(tmp_path, text), max_shapes=1000)


def test_a_use_that_contains_itself_is_refused(tmp_path):
    group = '<g id="a"><rect width="1" height="1"/><use href="#a"/></g>'
    text = f'<svg {SVG} viewBox="0 0 10 10">{group}<use href="#a"/></svg>'
    with pytest.raises(RecipeError, match="itself"):
        read(write(tmp_path, text))


def test_a_chain_of_uses_that_is_too_deep_is_refused(tmp_path):
    links = "".join(f'<g id="g{k}"><use href="#g{k - 1}"/></g>' for k in range(1, 40))
    defs = f'<defs><rect id="g0" width="1" height="1"/>{links}</defs>'
    text = f'<svg {SVG} viewBox="0 0 10 10">{defs}<use href="#g39"/></svg>'
    with pytest.raises(RecipeError, match="deep"):
        read(write(tmp_path, text))


def test_an_absurdly_deep_nesting_is_refused(tmp_path):
    depth = 1000
    text = (
        f'<svg {SVG} viewBox="0 0 10 10">' + "<g>" * depth + '<rect width="1" height="1"/>' + "</g>" * depth + "</svg>"
    )
    with pytest.raises(RecipeError, match="deep"):
        read(write(tmp_path, text))


def test_svg_bytes_gives_the_file_for_a_digest_and_refuses_what_read_svg_refuses(tmp_path):
    path = write(tmp_path, "<svg/>" + " " * 3000)
    assert svg_bytes(path, 4).startswith(b"<svg/>")
    with pytest.raises(RecipeError, match="max_kilobytes"):
        svg_bytes(path, 1)
    with pytest.raises(RecipeError, match="nowhere"):
        svg_bytes(tmp_path / "nowhere.svg", 4)
