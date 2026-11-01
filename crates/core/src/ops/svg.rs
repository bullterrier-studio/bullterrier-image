//! SVG files read as shapes: the outlines of a drawing in px, each with the `id` and fill rule it was drawn with.
//!
//! The file is untrusted, and [`read`] is the one door in; everything it refuses it refuses before the work it guards
//! against. Nothing in a file is run or fetched, and nothing but the bytes handed in is opened.
//!
//! What is drawn, in document order: `rect`, `circle`, `ellipse`, `polygon`, `polyline` and `path`; `g`, `a`, `switch` and
//! a nested `svg` are walked; `defs`, `symbol`, `clipPath`, `mask`, `pattern` and `marker` draw only through a `use`, and a
//! `use` draws what it points at where it stands (a `symbol` scaled to the `use`'s `width` and `height` by its `viewBox`).
//! `display="none"` (attribute or `style`) hides an element and all inside it. Colours, strokes, opacity and blend modes are
//! ignored; every outline is closed; the fill rule is the shape's own `fill-rule`, else its nearest ancestor's. Lines, text,
//! images, scripts and everything else draw nothing. Lengths are numbers of px (`px` or none), as `transform` and `points`
//! are numbers.
//!
//! The file is placed in a frame by its `viewBox` (else its `width` and `height`) and `preserveAspectRatio`, by the SVG
//! specification. Refused: a file that is not UTF-8 or declares XML entities (a billion laughs, an external entity); XML
//! that is not well formed or has more nodes than the limit; a root that is no `svg` in the SVG namespace, with a
//! `transform` or no size; elements nested deeper than the limit; a `use` that contains itself; more shapes than
//! `max_shapes`, counted before any `use` is expanded; a transform, length or path that cannot be read; a coordinate not
//! finite or too far outside the frame; too many corners; a file with no shape.

mod paint;
mod place;
mod scan;
mod shape;

use roxmltree::{Document, Node, ParsingOptions};
use svgtypes::{AspectRatio, Transform, ViewBox};

use self::place::Affine;
use self::shape::{length, path_data};
use super::outlines::{self, Frame, Limits};
use super::raster::{Polygons, Rule};
pub(crate) use paint::{Ids, Pieces};

/// One shape of a drawing.
struct Shape {
    /// The `id` of the element, "" if it has none.
    id: String,
    /// What decides what is inside the outlines.
    rule: Rule,
    /// The outlines in px.
    outlines: Polygons,
}

/// The shapes of an SVG file in px, back to front.
pub(crate) struct Drawing {
    shapes: Vec<Shape>,
}

/// The value of the property `name` of `node`: its `style` declaration, else its attribute.
fn property<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attribute("style")
        .and_then(|style| {
            style.split(';').find_map(|declaration| {
                let (key, value) = declaration.split_once(':')?;
                (key.trim() == name).then_some(value.trim())
            })
        })
        .or_else(|| node.attribute(name))
}

/// The fill rule `node` sets, if it sets one.
fn fill_rule(node: Node<'_, '_>) -> Option<Rule> {
    match property(node, "fill-rule")? {
        "evenodd" => Some(Rule::EvenOdd),
        "nonzero" => Some(Rule::NonZero),
        _ => None,
    }
}

/// The aspect ratio `node` sets: `xMidYMid meet` unless `preserveAspectRatio` says otherwise.
fn aspect(node: Node<'_, '_>) -> Result<AspectRatio, String> {
    node.attribute("preserveAspectRatio").map_or_else(
        || Ok(AspectRatio::default()),
        |text| {
            text.parse::<AspectRatio>()
                .map_err(|error| format!("has a preserveAspectRatio ({text:?}) that cannot be read: {error}"))
        },
    )
}

/// The view box of `node`, if it has one.
fn view_box(node: Node<'_, '_>) -> Result<Option<ViewBox>, String> {
    node.attribute("viewBox")
        .map(|text| {
            text.parse::<ViewBox>()
                .map_err(|error| format!("has a viewBox ({text:?}) that is no view box with an area: {error}"))
        })
        .transpose()
}

/// The map from the units inside `inner` (an `svg` or `symbol`) to the units outside it, with the origin of its viewport at
/// (0, 0): by its view box, fitted into the width and height `sized_by` gives (the view box's own, if it gives none). None
/// if the viewport has no area, which draws nothing; the identity if `inner` has no view box.
fn viewport(inner: Node<'_, '_>, sized_by: Node<'_, '_>) -> Result<Option<Affine>, String> {
    let Some(view) = view_box(inner)? else {
        return Ok(Some(Affine::IDENTITY));
    };
    let width = length(sized_by, "width")?.unwrap_or(view.w);
    let height = length(sized_by, "height")?.unwrap_or(view.h);
    let aspect = aspect(inner)?;
    Ok((width > 0.0 && height > 0.0).then(|| place::fit(view, aspect, [0.0, 0.0, width, height])))
}

/// The map from the root's units to the frame, and what to refuse about the root.
fn root_map(root: Node<'_, '_>, frame: Frame) -> Result<Affine, String> {
    let view = if let Some(view) = view_box(root)? {
        view
    } else {
        let (Some(width), Some(height)) = (length(root, "width")?, length(root, "height")?) else {
            return Err("has no viewBox and no width and height, so it has no size to place".to_owned());
        };
        if !(width > 0.0 && height > 0.0) {
            return Err(format!("has a size without area ({width} x {height})"));
        }
        ViewBox::new(0.0, 0.0, width, height)
    };
    let [left, top, right, bottom] = frame;
    Ok(place::fit(view, aspect(root)?, [left, top, right - left, bottom - top]))
}

/// The transform an element sets, the identity if it sets none.
fn transform(node: Node<'_, '_>) -> Result<Affine, String> {
    node.attribute("transform").map_or(Ok(Affine::IDENTITY), |text| {
        text.parse::<Transform>()
            .map(Affine::from)
            .map_err(|error| format!("has a transform ({text:?}) that cannot be read: {error}"))
    })
}

/// The shapes `root` draws, in document order, placed by `base` (root units to px); all their corners together stay within
/// `limits`.
fn draw(root: Node<'_, '_>, base: Affine, index: &scan::Index<'_, '_>, limits: &Limits) -> Result<Vec<Shape>, String> {
    /// An element to draw, with the map and fill rule it inherits.
    struct Pending<'a, 'i> {
        node: Node<'a, 'i>,
        map: Affine,
        rule: Rule,
    }
    // the children of `node` go on the stack last first, so that the first is drawn first
    fn push_children<'a, 'i>(stack: &mut Vec<Pending<'a, 'i>>, node: Node<'a, 'i>, map: Affine, rule: Rule) {
        for child in scan::drawn_children(node).into_iter().rev() {
            stack.push(Pending { node: child, map, rule });
        }
    }
    let mut shapes: Vec<Shape> = Vec::new();
    let mut corners = 0_usize; // constant: no corner has been drawn yet
    let mut stack = Vec::new();
    if property(root, "display") != Some("none") {
        push_children(&mut stack, root, base, fill_rule(root).unwrap_or(Rule::NonZero));
    }
    while let Some(Pending { node, map, rule }) = stack.pop() {
        if property(node, "display") == Some("none") {
            continue;
        }
        let name = scan::tag(node);
        let rule = fill_rule(node).unwrap_or(rule);
        let map = map.after(&transform(node)?);
        if scan::is_shape(name) {
            let Some(d) = path_data(node, name)? else {
                continue;
            };
            let outlines = outlines::flatten(&d, limits, corners, |x, y| map.apply(x, y))
                .map_err(|said| format!("has a <{name}> that cannot be drawn: {said}"))?;
            corners += outlines.corners();
            shapes.push(Shape {
                id: node.attribute("id").unwrap_or_default().to_owned(),
                rule,
                outlines,
            });
        } else if name == "svg" {
            // a nested svg: its viewport is at (x, y), its units by its own view box
            let (x, y) = (length(node, "x")?.unwrap_or(0.0), length(node, "y")?.unwrap_or(0.0));
            if let Some(inner) = viewport(node, node)? {
                push_children(
                    &mut stack,
                    node,
                    map.after(&Affine::translate(x, y)).after(&inner),
                    rule,
                );
            }
        } else if name == "use" {
            let Some(target) = index.target(node) else {
                continue;
            };
            let (x, y) = (length(node, "x")?.unwrap_or(0.0), length(node, "y")?.unwrap_or(0.0));
            let map = map.after(&Affine::translate(x, y));
            if scan::tag(target) == "symbol" {
                // a symbol is scaled to the width and height of the `use`
                if let Some(inner) = viewport(target, node)? {
                    push_children(&mut stack, target, map.after(&inner), fill_rule(target).unwrap_or(rule));
                }
            } else {
                stack.push(Pending {
                    node: target,
                    map,
                    rule,
                });
            }
        } else {
            // `g`, `a` and `switch`: everything inside is drawn
            push_children(&mut stack, node, map, rule);
        }
    }
    Ok(shapes)
}

/// The drawing in the SVG file `bytes`, placed in `frame` (px).
///
/// # Errors
/// A message completing "svg file 'x' ...", for everything the module's documentation says is refused: a file with more
/// than `max_shapes` shapes, or any of the `limits` exceeded.
pub(crate) fn read(bytes: &[u8], frame: Frame, max_shapes: u32, limits: &Limits) -> Result<Drawing, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "is not UTF-8 (only UTF-8 files are read)".to_owned())?;
    // before the parser sees it: entities are how a small file becomes a huge one, or names another file
    if text.contains("<!ENTITY") {
        return Err("declares XML entities (a DOCTYPE with an internal subset), which are refused".to_owned());
    }
    let options = ParsingOptions {
        allow_dtd: true, // a DOCTYPE that declares nothing is harmless, and exporters write one
        nodes_limit: limits.nodes,
        ..ParsingOptions::default()
    };
    let document = Document::parse_with_options(text, options).map_err(|error| match error {
        roxmltree::Error::NodesLimitReached => format!("has more than {} nodes", limits.nodes),
        other => format!("is not well-formed XML: {other}"),
    })?;
    let root = document.root_element();
    if scan::tag(root) != "svg" {
        return Err("is not an svg file (its root element is not <svg> in the SVG namespace)".to_owned());
    }
    if root.has_attribute("transform") {
        return Err("has a transform on the root <svg> element, which is not supported (put it on a group)".to_owned());
    }
    let base = root_map(root, frame)?;
    let index = scan::Index::new(&document);
    let count = scan::shapes(root, &index, limits.depth)?;
    if count > u64::from(max_shapes) {
        return Err(format!("has {count} shapes, over the limit max_shapes = {max_shapes}"));
    }
    let shapes = draw(root, base, &index, limits)?;
    if shapes.is_empty() {
        return Err("has no shapes (it needs a rect, circle, ellipse, polygon, polyline or path)".to_owned());
    }
    Ok(Drawing { shapes })
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::{Drawing, Shape, read};
    use crate::ops::length;
    use crate::ops::outlines::{Frame, Limits};
    use crate::ops::raster::{self, Rule};

    const NS: &str = r#"xmlns="http://www.w3.org/2000/svg""#;
    /// A 200 x 200 frame at the origin.
    const SQUARE: Frame = [0.0, 0.0, 200.0, 200.0];

    fn drawing(text: &str, frame: Frame) -> Result<Drawing, String> {
        read(text.as_bytes(), frame, 100, &Limits::STANDARD)
    }

    fn svg(attributes: &str, inside: &str) -> String {
        format!("<svg {NS} {attributes}>{inside}</svg>")
    }

    /// `(left, top, right, bottom)` of the corners of the shape.
    fn extent(shape: &Shape) -> [f64; 4] {
        let corners: Vec<[f64; 2]> = (0..shape.outlines.outlines())
            .flat_map(|number| shape.outlines.outline(number).to_vec())
            .collect();
        let along = |axis: usize| {
            corners
                .iter()
                .map(|corner| corner[axis])
                .fold((f64::MAX, f64::MIN), |(low, high), value| {
                    (low.min(value), high.max(value))
                })
        };
        [along(0).0, along(1).0, along(0).1, along(1).1]
    }

    /// How many pixels of a `size` x `size` picture the shape paints (by its own rule).
    fn area(shape: &Shape, size: usize) -> f64 {
        let mut painted = vec![0_i32; size * size];
        raster::fill(&mut painted, size, size, &shape.outlines, shape.rule, 1);
        length::float(painted.iter().filter(|&&pixel| pixel == 1).count())
    }

    fn close(actual: [f64; 4], expected: [f64; 4]) -> bool {
        actual.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-6)
    }

    fn ids(drawing: &Drawing) -> Vec<&str> {
        drawing.shapes.iter().map(|shape| shape.id.as_str()).collect()
    }

    #[test]
    fn a_shape_is_placed_by_the_view_box_inside_the_frame_centred_by_default() -> Result<(), String> {
        // a 2:1 picture fitted inside a square frame: 200 wide, 100 tall, centred
        let drawn = drawing(
            &svg(
                r#"viewBox="0 0 100 50""#,
                r#"<rect id="r" x="0" y="0" width="100" height="50"/>"#,
            ),
            SQUARE,
        )?;
        assert!(close(extent(&drawn.shapes[0]), [0.0, 50.0, 200.0, 150.0]));
        assert_eq!(ids(&drawn), ["r"]);
        Ok(())
    }

    #[test]
    fn preserve_aspect_ratio_places_the_view_box_as_the_svg_specification_says() -> Result<(), String> {
        // (preserveAspectRatio, where the whole 100 x 50 view box lands in the 200 x 200 frame: left, top, right, bottom)
        for (aspect, expected) in [
            ("xMinYMin meet", [0.0, 0.0, 200.0, 100.0]),
            ("xMaxYMax meet", [0.0, 100.0, 200.0, 200.0]),
            ("none", [0.0, 0.0, 200.0, 200.0]),
            ("xMidYMid slice", [-100.0, 0.0, 300.0, 200.0]),
            ("xMinYMid slice", [0.0, 0.0, 400.0, 200.0]),
            ("xMaxYMin slice", [-200.0, 0.0, 200.0, 200.0]),
        ] {
            let text = svg(
                &format!(r#"viewBox="0 0 100 50" preserveAspectRatio="{aspect}""#),
                r#"<rect width="100" height="50"/>"#,
            );
            let drawn = drawing(&text, SQUARE)?;
            assert!(
                close(extent(&drawn.shapes[0]), expected),
                "{aspect}: {:?}",
                extent(&drawn.shapes[0])
            );
        }
        Ok(())
    }

    #[test]
    fn every_alignment_puts_the_free_room_before_the_view_box_by_the_share_its_name_gives() -> Result<(), String> {
        // xMin, xMid and xMax leave none, half or all of the free room on the left, and yMin, yMid and yMax above. A wide
        // view box (100 x 50) in the 200 x 200 frame has room above or below when it fits inside (it is then 200 x 100) and
        // to the left or right when it covers (400 x 200); a tall one (50 x 100) the other way round
        for (x, fx) in [("xMin", 0.0), ("xMid", 0.5), ("xMax", 1.0)] {
            for (y, fy) in [("YMin", 0.0), ("YMid", 0.5), ("YMax", 1.0)] {
                // (view box, meet or slice, where the whole view box lands: left, top, right, bottom)
                let rows = [
                    ("100 50", "meet", [0.0, 100.0 * fy, 200.0, 100.0 + 100.0 * fy]),
                    ("50 100", "meet", [100.0 * fx, 0.0, 100.0 + 100.0 * fx, 200.0]),
                    ("100 50", "slice", [-200.0 * fx, 0.0, 400.0 - 200.0 * fx, 200.0]),
                    ("50 100", "slice", [0.0, -200.0 * fy, 200.0, 400.0 - 200.0 * fy]),
                ];
                for (size, how, expected) in rows {
                    let text = svg(
                        &format!(r#"viewBox="0 0 {size}" preserveAspectRatio="{x}{y} {how}""#),
                        &format!(
                            r#"<rect width="{}" height="{}"/>"#,
                            &size[..size.find(' ').unwrap_or(0)],
                            &size[size.find(' ').map_or(0, |at| at + 1)..]
                        ),
                    );
                    let placed = extent(&drawing(&text, SQUARE)?.shapes[0]);
                    assert!(close(placed, expected), "{x}{y} {how} {size}: {placed:?}");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn the_view_box_origin_and_a_frame_that_does_not_start_at_zero_are_honoured() -> Result<(), String> {
        // a 100 x 100 frame at (100, 40) for a 50 x 50 view box from (-10, -20): scale 2
        let text = svg(
            r#"viewBox="-10 -20 50 50""#,
            r#"<rect x="0" y="0" width="10" height="10"/>"#,
        );
        let drawn = drawing(&text, [100.0, 40.0, 200.0, 140.0])?;
        assert!(close(extent(&drawn.shapes[0]), [120.0, 80.0, 140.0, 100.0]));
        Ok(())
    }

    #[test]
    fn without_a_view_box_the_width_and_height_are_the_view_box() -> Result<(), String> {
        for sized in [r#"width="50" height="25""#, r#"width="50px" height="25px""#] {
            let drawn = drawing(
                &svg(sized, r#"<rect width="50" height="25"/>"#),
                [0.0, 0.0, 100.0, 100.0],
            )?;
            assert!(close(extent(&drawn.shapes[0]), [0.0, 25.0, 100.0, 75.0]), "{sized}");
        }
        Ok(())
    }

    #[test]
    fn a_file_that_is_no_usable_svg_is_refused_naming_the_problem() {
        let rect = r#"<rect width="5" height="5"/>"#;
        let cases = [
            ("no size", svg("", rect), "no viewBox"),
            ("only a width", svg(r#"width="10""#, rect), "no viewBox"),
            (
                "root transform",
                svg(r#"viewBox="0 0 100 50" transform="scale(2)""#, rect),
                "transform",
            ),
            ("empty view box", svg(r#"viewBox="0 0 0 50""#, rect), "viewBox"),
            ("zero size", svg(r#"width="0" height="0""#, rect), "size without area"),
            (
                "size that is no number",
                svg(r#"width="abc" height="10""#, rect),
                "width",
            ),
            (
                "size in other units",
                svg(r#"width="10cm" height="10cm""#, rect),
                "no number of px",
            ),
            ("view box of letters", svg(r#"viewBox="a b c d""#, rect), "viewBox"),
            ("view box of nan", svg(r#"viewBox="0 0 nan nan""#, rect), "viewBox"),
            ("not an svg", "<html><body/></html>".to_owned(), "not an svg"),
            (
                "svg outside its namespace",
                "<svg viewBox='0 0 9 9'><rect width='5' height='5'/></svg>".to_owned(),
                "not an svg",
            ),
            (
                "not well formed",
                format!("<svg {NS} viewBox='0 0 9 9'><rect"),
                "not well-formed XML",
            ),
            ("nothing to draw", svg("viewBox='0 0 9 9'", ""), "no shapes"),
            (
                "a rect without size",
                svg("viewBox='0 0 9 9'", "<rect width='5'/>"),
                "no shapes",
            ),
            (
                "transform without numbers",
                svg(
                    "viewBox='0 0 9 9'",
                    r#"<g transform="rotate(a)"><rect width="5" height="5"/></g>"#,
                ),
                "transform",
            ),
            (
                "path data that is no path",
                svg("viewBox='0 0 9 9'", r#"<path d="M 0 0 L foo"/>"#),
                "cannot be drawn",
            ),
            ("empty path data", svg("viewBox='0 0 9 9'", r#"<path d=""/>"#), "empty"),
            (
                "a length that is no number",
                svg("viewBox='0 0 9 9'", r#"<rect width="5" height="x"/>"#),
                "height",
            ),
        ];
        for (what, text, mentions) in cases {
            let refused = drawing(&text, SQUARE);
            assert!(
                matches!(&refused, Err(said) if said.contains(mentions)),
                "{what}: {:?}",
                refused.err()
            );
        }
    }

    #[test]
    fn nested_groups_transforms_and_every_kind_of_shape_come_out_in_document_order() -> Result<(), String> {
        let inside = r#"
          <g transform="translate(10 10)"><g transform="scale(2)"><rect id="rect" x="1" y="1" width="3" height="4"/></g></g>
          <circle id="circle" cx="50" cy="50" r="10"/>
          <ellipse id="ellipse" cx="50" cy="80" rx="20" ry="5"/>
          <polygon id="polygon" points="0,0 10,0 10,10"/>
          <polyline id="polyline" points="80,80 90,80 90,90"/>
          <path d="M 5 5 h 4 v 4 z"/>
          <line x1="0" y1="0" x2="50" y2="50" stroke="black"/>
          <text x="5" y="5">words</text>
          <rect id="hidden" width="9" height="9" display="none"/>
          <g style="display:none"><rect id="in-hidden" width="9" height="9"/></g>"#;
        let drawn = drawing(&svg(r#"viewBox="0 0 100 100""#, inside), [0.0, 0.0, 100.0, 100.0])?;
        assert_eq!(ids(&drawn), ["rect", "circle", "ellipse", "polygon", "polyline", ""]);
        assert!(close(extent(&drawn.shapes[0]), [12.0, 12.0, 18.0, 20.0]));
        // pixels painted: pi r^2 and pi rx ry, within 1 % (the border pixels decide)
        let circle = area(&drawn.shapes[1], 100);
        assert!((circle / (std::f64::consts::PI * 100.0) - 1.0).abs() < 1e-2, "{circle}");
        let ellipse = area(&drawn.shapes[2], 100);
        assert!(
            (ellipse / (std::f64::consts::PI * 100.0) - 1.0).abs() < 2e-2,
            "{ellipse}"
        );
        // a triangle of legs 10 is 50 (pixels whose centres are inside; those on the diagonal go by the rule at edges)
        let triangle = area(&drawn.shapes[3], 100);
        assert!((45.0..=55.0).contains(&triangle), "{triangle}");
        Ok(())
    }

    #[test]
    fn a_rounded_rectangle_has_the_area_of_a_rectangle_less_its_four_corners() -> Result<(), String> {
        // 80 x 60 with radii 10 x 10: 4800 - (4 - pi) * 100; a lone rx gives the same radius to ry
        for corners in [r#"rx="10" ry="10""#, r#"rx="10""#, r#"ry="10""#] {
            let text = svg(
                r#"viewBox="0 0 100 100""#,
                &format!(r#"<rect x="10" y="10" width="80" height="60" {corners}/>"#),
            );
            let drawn = drawing(&text, [0.0, 0.0, 100.0, 100.0])?;
            let painted = area(&drawn.shapes[0], 100);
            let exact = 4800.0 - (4.0 - std::f64::consts::PI) * 100.0;
            assert!((painted / exact - 1.0).abs() < 4e-3, "{corners}: {painted} vs {exact}");
        }
        // radii beyond half the sides are cut to half: an ellipse of 40 x 30 radii, however large the radius asked
        let text = svg(
            r#"viewBox="0 0 100 100""#,
            r#"<rect x="10" y="10" width="80" height="60" rx="100"/>"#,
        );
        let painted = area(&drawing(&text, [0.0, 0.0, 100.0, 100.0])?.shapes[0], 100);
        assert!(
            (painted / (std::f64::consts::PI * 1200.0) - 1.0).abs() < 1e-2,
            "{painted}"
        );
        Ok(())
    }

    #[test]
    fn a_rotation_turns_the_shape_about_its_centre() -> Result<(), String> {
        let text = svg(
            r#"viewBox="0 0 100 100""#,
            r#"<g transform="rotate(90 50 50)"><rect x="10" y="40" width="30" height="10"/></g>"#,
        );
        let drawn = drawing(&text, [0.0, 0.0, 100.0, 100.0])?;
        // (x, y) goes to (100 - y, x)
        assert!(close(extent(&drawn.shapes[0]), [50.0, 10.0, 60.0, 40.0]));
        Ok(())
    }

    #[test]
    fn a_transform_applies_to_its_element_before_the_transforms_outside_it() -> Result<(), String> {
        // the inner scale(2) first: (1, 1) to (2, 2), then the outer translate(10, 0): (12, 2). The other order would give (22, 2)
        let text = svg(
            r#"viewBox="0 0 100 100""#,
            r#"<g transform="translate(10 0)"><rect transform="scale(2)" x="1" y="1" width="1" height="1"/></g>"#,
        );
        let drawn = drawing(&text, [0.0, 0.0, 100.0, 100.0])?;
        assert!(close(extent(&drawn.shapes[0]), [12.0, 2.0, 14.0, 4.0]));
        Ok(())
    }

    #[test]
    fn shapes_in_defs_that_nothing_uses_are_not_counted_or_drawn() -> Result<(), String> {
        let defs = format!("<defs>{}</defs>", r#"<rect width="1" height="1"/>"#.repeat(10));
        let text = svg(
            r#"viewBox="0 0 10 10""#,
            &format!(r#"{defs}<rect id="drawn" width="5" height="5"/>"#),
        );
        let drawn = read(text.as_bytes(), SQUARE, 3, &Limits::STANDARD)?;
        assert_eq!(ids(&drawn), ["drawn"]);
        Ok(())
    }

    #[test]
    fn the_fill_rule_is_read_from_the_shape_or_its_style_and_inherited_from_the_group() -> Result<(), String> {
        let triangle = r#"d="M0 0h4v4z""#;
        let text = svg(
            r#"viewBox="0 0 10 10""#,
            &format!(
                r#"<g fill-rule="evenodd"><path id="a" {triangle}/></g><path id="b" {triangle}/>
                   <path id="c" fill-rule="evenodd" {triangle}/><path id="d" style="fill-rule:evenodd" {triangle}/>
                   <g fill-rule="evenodd"><path id="e" fill-rule="nonzero" {triangle}/></g>"#
            ),
        );
        let rules: Vec<Rule> = drawing(&text, SQUARE)?.shapes.iter().map(|shape| shape.rule).collect();
        assert_eq!(
            rules,
            [
                Rule::EvenOdd,
                Rule::NonZero,
                Rule::EvenOdd,
                Rule::EvenOdd,
                Rule::NonZero
            ]
        );
        Ok(())
    }

    #[test]
    fn a_shape_with_a_hole_leaves_the_hole_unfilled_under_even_odd_and_filled_under_nonzero() -> Result<(), String> {
        let ring = "M 10 50 a 40 40 0 1 0 80 0 a 40 40 0 1 0 -80 0 Z M 30 50 a 20 20 0 1 1 40 0 a 20 20 0 1 1 -40 0 Z";
        let frame = [0.0, 0.0, 100.0, 100.0];
        let holed = drawing(
            &svg(
                r#"viewBox="0 0 100 100""#,
                &format!(r#"<path fill-rule="evenodd" d="{ring}"/>"#),
            ),
            frame,
        )?;
        let exact = std::f64::consts::PI * (40.0 * 40.0 - 20.0 * 20.0);
        let painted = area(&holed.shapes[0], 100);
        assert!((painted / exact - 1.0).abs() < 3e-3, "{painted} vs {exact}");
        // the inner circle is drawn the other way round (sweep flag 1 against 0), so nonzero leaves the hole as well
        let nonzero = drawing(
            &svg(r#"viewBox="0 0 100 100""#, &format!(r#"<path d="{ring}"/>"#)),
            frame,
        )?;
        assert!((area(&nonzero.shapes[0], 100) / exact - 1.0).abs() < 3e-3);
        // two circles drawn the same way round: even-odd leaves a hole, nonzero fills it
        let same = "M 10 50 a 40 40 0 1 0 80 0 a 40 40 0 1 0 -80 0 Z M 30 50 a 20 20 0 1 0 40 0 a 20 20 0 1 0 -40 0 Z";
        let rule = |name: &str| -> Result<f64, String> {
            let text = svg(
                r#"viewBox="0 0 100 100""#,
                &format!(r#"<path fill-rule="{name}" d="{same}"/>"#),
            );
            Ok(area(&drawing(&text, frame)?.shapes[0], 100))
        };
        assert!((rule("evenodd")? / exact - 1.0).abs() < 3e-3);
        assert!((rule("nonzero")? / (std::f64::consts::PI * 1600.0) - 1.0).abs() < 3e-3);
        Ok(())
    }

    #[test]
    fn a_used_shape_is_drawn_where_it_is_used_and_not_where_it_is_defined() -> Result<(), String> {
        let text = svg(
            r#"viewBox="0 0 100 100""#,
            r##"<defs><rect id="r" width="10" height="10"/></defs><use href="#r" x="30" y="40"/><use href="#r" x="60" y="60"/>"##,
        );
        let drawn = drawing(&text, [0.0, 0.0, 100.0, 100.0])?;
        assert_eq!(drawn.shapes.len(), 2);
        assert!(close(extent(&drawn.shapes[0]), [30.0, 40.0, 40.0, 50.0]));
        assert!(close(extent(&drawn.shapes[1]), [60.0, 60.0, 70.0, 70.0]));
        // a used shape carries the id of the element it uses
        assert_eq!(ids(&drawn), ["r", "r"]);
        Ok(())
    }

    #[test]
    fn a_nested_svg_and_a_used_symbol_are_placed_by_their_own_view_boxes() -> Result<(), String> {
        // a nested svg at (60, 40), 60 x 40, showing a 6 x 4 view box: scale 10
        let nested = svg(
            r#"viewBox="0 0 200 200""#,
            r#"<svg x="60" y="40" width="60" height="40" viewBox="0 0 6 4"><rect width="6" height="4"/></svg>"#,
        );
        assert!(close(
            extent(&drawing(&nested, SQUARE)?.shapes[0]),
            [60.0, 40.0, 120.0, 80.0]
        ));
        // a symbol of a 10 x 10 view box used at (20, 30) with width and height 60: scale 6 (the symbol is not drawn alone)
        let used = svg(
            r#"viewBox="0 0 200 200""#,
            r##"<symbol id="s" viewBox="0 0 10 10"><rect width="5" height="5"/></symbol><use href="#s" x="20" y="30" width="60" height="60"/>"##,
        );
        let drawn = drawing(&used, SQUARE)?;
        assert_eq!(drawn.shapes.len(), 1);
        assert!(close(extent(&drawn.shapes[0]), [20.0, 30.0, 50.0, 60.0]));
        Ok(())
    }

    #[test]
    fn a_realistic_file_gives_its_shapes_inside_the_frame_with_the_hole_and_the_transforms_applied()
    -> Result<(), String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/mosaic.svg");
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        // the 3:2 view box is fitted inside: 600 x 400 centred, scale 2, 100 px down
        let drawn = read(&bytes, [0.0, 0.0, 600.0, 600.0], 100, &Limits::STANDARD)?;
        assert_eq!(
            ids(&drawn),
            [
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
                "drop"
            ]
        );
        let shape = |id: &str| drawn.shapes.iter().find(|shape| shape.id == id);
        // the group moves the sun's circle by (-10, 5) before the view box scales it
        let sun = extent(shape("sun").ok_or("sun")?);
        let hand = [
            2.0 * (255.0 - 10.0 - 28.0),
            100.0 + 2.0 * (48.0 + 5.0 - 28.0),
            2.0 * (255.0 - 10.0 + 28.0),
            100.0 + 2.0 * (48.0 + 5.0 + 28.0),
        ];
        assert!(sun.iter().zip(hand).all(|(a, b)| (a - b).abs() < 0.5), "{sun:?}");
        let ring = shape("ring").ok_or("ring")?;
        assert_eq!(ring.rule, Rule::EvenOdd);
        let exact = 4.0 * std::f64::consts::PI * (32.0 * 32.0 - 16.0 * 16.0);
        // pixel centres counted along 600 px of border: within half a percent
        assert!(
            (area(ring, 600) / exact - 1.0).abs() < 5e-3,
            "{} vs {exact}",
            area(ring, 600)
        );
        let pebble = area(shape("pebble").ok_or("pebble")?, 600);
        assert!((pebble / (std::f64::consts::PI * 4.0 * 20.0 * 9.0) - 1.0).abs() < 3e-3);
        // rotate(-14) and scale(0.8) bend and shrink the leaf: it stays inside the picture
        let leaf = extent(shape("leaf").ok_or("leaf")?);
        assert!(0.0 < leaf[0] && leaf[0] < leaf[2] && leaf[2] < 600.0);
        assert!(100.0 < leaf[1] && leaf[1] < leaf[3] && leaf[3] < 500.0);
        Ok(())
    }

    #[test]
    fn a_vector_drawing_reads_as_its_shapes_with_paint_opacity_and_blend_mode_ignored() -> Result<(), String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/svg/vector-drawing.svg");
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        // 1:1 in a frame of its own size
        let drawn = read(&bytes, [0.0, 0.0, 120.0, 80.0], 100, &Limits::STANDARD)?;
        assert_eq!(drawn.shapes.len(), 3);
        assert!(
            drawn
                .shapes
                .iter()
                .all(|shape| shape.id.is_empty() && shape.rule == Rule::NonZero)
        );
        // hand extents of the three absolute paths (see the file)
        assert!(close(extent(&drawn.shapes[0]), [10.0, 10.0, 70.0, 50.0]));
        assert!(close(extent(&drawn.shapes[1]), [40.0, 30.0, 110.0, 70.0]));
        assert!(close(extent(&drawn.shapes[2]), [20.0, 5.0, 100.0, 75.0]));
        Ok(())
    }

    #[test]
    fn a_document_that_declares_entities_is_refused_before_anything_is_expanded_or_fetched() {
        let billion = format!(
            r#"<?xml version="1.0"?>
<!DOCTYPE svg [
  <!ENTITY a "aaaaaaaaaa">
  <!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;">
  <!ENTITY c "&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;">
]>
<svg {NS} viewBox="0 0 10 10"><rect id="&c;" width="5" height="5"/></svg>"#
        );
        let external = format!(
            r#"<?xml version="1.0"?>
<!DOCTYPE svg [ <!ENTITY secret SYSTEM "file:///etc/passwd"> <!ENTITY remote SYSTEM "http://127.0.0.1:9/x"> ]>
<svg {NS} viewBox="0 0 10 10"><rect id="&secret;&remote;" width="5" height="5"/></svg>"#
        );
        let parameter = format!(
            r#"<!DOCTYPE svg [ <!ENTITY % x SYSTEM "http://127.0.0.1:9/x.dtd"> %x; ]><svg {NS} viewBox="0 0 10 10"><rect width="5" height="5"/></svg>"#
        );
        for (what, text) in [
            ("billion laughs", billion),
            ("external entity", external),
            ("parameter entity", parameter),
        ] {
            let refused = drawing(&text, SQUARE);
            assert!(
                matches!(&refused, Err(said) if said.contains("XML entities")),
                "{what}: {:?}",
                refused.err()
            );
        }
    }

    #[test]
    fn a_document_type_without_declarations_is_harmless_and_its_dtd_is_never_fetched() -> Result<(), String> {
        let text = format!(
            r#"<?xml version="1.0"?><!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://127.0.0.1:9/DTD/svg11.dtd"><svg {NS} viewBox="0 0 10 10"><rect width="5" height="5"/></svg>"#
        );
        assert_eq!(drawing(&text, SQUARE)?.shapes.len(), 1);
        Ok(())
    }

    #[test]
    fn a_file_that_is_not_utf_8_is_refused_even_when_it_would_declare_entities() {
        let text = format!(r#"<?xml version="1.0"?><svg {NS} viewBox="0 0 10 10"><rect width="5" height="5"/></svg>"#);
        let utf16: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        let refused = read(&utf16, SQUARE, 100, &Limits::STANDARD).err();
        assert!(matches!(&refused, Some(said) if said.contains("UTF-8")), "{refused:?}");
    }

    #[test]
    fn scripts_links_and_images_draw_nothing_and_a_link_around_a_shape_draws_the_shape() -> Result<(), String> {
        let inside = r#"
          <script>fetch('http://127.0.0.1:9/')</script>
          <image href="http://127.0.0.1:9/x.png" width="5" height="5"/>
          <image xlink:href="file:///etc/passwd" width="5" height="5"/>
          <a href="http://127.0.0.1:9/"><rect id="inside-a-link" width="5" height="5"/></a>
          <use href="http://127.0.0.1:9/other.svg#shape"/>
          <style>@import url(http://127.0.0.1:9/x.css);</style>"#;
        let text = format!(
            r#"<svg {NS} xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 10 10" onload="alert(1)">{inside}</svg>"#
        );
        assert_eq!(ids(&drawing(&text, SQUARE)?), ["inside-a-link"]);
        Ok(())
    }

    #[test]
    fn more_shapes_than_the_limit_are_refused_naming_the_limit_and_the_count_and_the_limit_itself_is_allowed()
    -> Result<(), String> {
        let text = svg(r#"viewBox="0 0 10 10""#, &r#"<rect width="1" height="1"/>"#.repeat(7));
        let refused = read(text.as_bytes(), SQUARE, 6, &Limits::STANDARD).err();
        assert!(matches!(&refused, Some(said) if said.contains("max_shapes = 6") && said.contains("7 shapes")));
        assert_eq!(read(text.as_bytes(), SQUARE, 7, &Limits::STANDARD)?.shapes.len(), 7);
        Ok(())
    }

    /// Ten groups each using the one before ten times: 10^10 shapes from a tiny file.
    fn multiplying() -> String {
        let mut levels = String::new();
        for level in 1..=10 {
            let uses = format!(r##"<use href="#g{}"/>"##, level - 1).repeat(10);
            let _ = write!(levels, r#"<g id="g{level}">{uses}</g>"#);
        }
        svg(
            r#"viewBox="0 0 10 10""#,
            &format!(r##"<defs><rect id="g0" width="1" height="1"/>{levels}</defs><use href="#g10"/>"##),
        )
    }

    #[test]
    fn uses_that_multiply_shapes_are_counted_before_they_are_expanded() {
        let refused = read(multiplying().as_bytes(), SQUARE, 1000, &Limits::STANDARD).err();
        assert!(
            matches!(&refused, Some(said) if said.contains("max_shapes = 1000")),
            "{refused:?}"
        );
    }

    #[test]
    fn a_use_that_contains_itself_is_refused() {
        let group = r##"<g id="a"><rect width="1" height="1"/><use href="#a"/></g>"##;
        let text = svg(r#"viewBox="0 0 10 10""#, &format!(r##"{group}<use href="#a"/>"##));
        let refused = drawing(&text, SQUARE).err();
        assert!(
            matches!(&refused, Some(said) if said.contains("contains itself")),
            "{refused:?}"
        );
    }

    #[test]
    fn elements_nest_at_most_as_deep_as_the_limit_and_a_use_is_the_parent_of_what_it_uses() {
        let nested = |levels: usize| {
            // the root is level 1, the groups 2 to levels - 1, the rect the last
            svg(
                r#"viewBox="0 0 10 10""#,
                &format!(
                    "{}{}{}",
                    "<g>".repeat(levels - 2),
                    r#"<rect width="1" height="1"/>"#,
                    "</g>".repeat(levels - 2)
                ),
            )
        };
        assert_eq!(drawing(&nested(64), SQUARE).map(|drawn| drawn.shapes.len()), Ok(1));
        let refused = drawing(&nested(65), SQUARE).err();
        assert!(
            matches!(&refused, Some(said) if said.contains("deeper than 64")),
            "{refused:?}"
        );
        assert!(drawing(&nested(1000), SQUARE).is_err());
        // a chain of n uses, each of the one before, ending at a rect: the svg is level 1, the drawn use 2, the rect n + 3
        let chain = |n: usize| {
            let mut links = r#"<rect id="u0" width="1" height="1"/>"#.to_owned();
            for link in 1..=n {
                let _ = write!(links, r##"<use id="u{link}" href="#u{}"/>"##, link - 1);
            }
            svg(
                r#"viewBox="0 0 10 10""#,
                &format!(r##"<defs>{links}</defs><use href="#u{n}"/>"##),
            )
        };
        assert_eq!(drawing(&chain(61), SQUARE).map(|drawn| drawn.shapes.len()), Ok(1));
        let refused = drawing(&chain(62), SQUARE).err();
        assert!(
            matches!(&refused, Some(said) if said.contains("deeper than 64")),
            "{refused:?}"
        );
    }

    #[test]
    fn a_coordinate_far_outside_the_frame_and_a_curve_needing_too_many_chords_are_refused() {
        let far = |d: &str| drawing(&svg(r#"viewBox="0 0 10 10""#, &format!(r#"<path d="{d}"/>"#)), SQUARE);
        // 10^7 px is allowed, a little more is not (the view box scales by 20)
        assert!(far("M 0 0 L 500000 0").is_ok());
        assert!(matches!(far("M 0 0 L 500001 0"), Err(said) if said.contains("too far")));
        assert!(matches!(far("M 0 0 C 0 0 1e30 1e30 0 0"), Err(said) if said.contains("too far")));
        assert!(matches!(far("M 0 0 L 1e999 0"), Err(said) if said.contains("cannot be drawn")));
        // a transform that makes a finite coordinate overflow
        let text = svg(
            r#"viewBox="0 0 10 10""#,
            r#"<g transform="scale(1e300)"><rect width="1e300" height="1"/></g>"#,
        );
        assert!(matches!(drawing(&text, SQUARE), Err(said) if said.contains("too far")));
    }

    #[test]
    fn corners_nodes_and_pixels_are_limited_each_at_its_value_and_one_past_it() -> Result<(), String> {
        let square = |count: usize| {
            svg(
                r#"viewBox="0 0 10 10""#,
                &r#"<path d="M 0 0 L 1 0 L 1 1 L 0 1 Z"/>"#.repeat(count),
            )
        };
        // the corners of all the shapes together: 4 shapes of 4 corners
        let few = Limits {
            corners: 16,
            ..Limits::STANDARD
        };
        assert_eq!(read(square(4).as_bytes(), SQUARE, 100, &few)?.shapes.len(), 4);
        let refused = read(square(5).as_bytes(), SQUARE, 100, &few).err();
        assert!(
            matches!(&refused, Some(said) if said.contains("more than 16 corners")),
            "{refused:?}"
        );
        // nodes: the svg, its 3 rects and (the `Document` counts) the root node: 5 nodes
        let text = square(3);
        let nodes = |nodes| {
            read(
                text.as_bytes(),
                SQUARE,
                100,
                &Limits {
                    nodes,
                    ..Limits::STANDARD
                },
            )
        };
        assert!(nodes(5).is_ok());
        assert!(matches!(nodes(4), Err(said) if said.contains("more than 4 nodes")));
        Ok(())
    }
}
