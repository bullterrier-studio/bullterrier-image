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
//! `max_shapes`, counted before any `use` is expanded, or a drawing that visits more elements than `Limits::drawn` (a `use`
//! repeats everything it uses, groups included); a transform, length or path that cannot be read; a coordinate not
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
    let count = scan::count(root, &index, limits.depth)?;
    if count.shapes > u64::from(max_shapes) {
        return Err(format!(
            "has {} shapes, over the limit max_shapes = {max_shapes}",
            count.shapes
        ));
    }
    // a use of a group of uses of groups repeats elements, not only shapes: the work is the visits
    if count.visits > limits.drawn {
        return Err(format!(
            "would draw {} elements, over the limit of {} (a use repeats everything it uses)",
            count.visits, limits.drawn
        ));
    }
    let shapes = draw(root, base, &index, limits)?;
    if shapes.is_empty() {
        return Err("has no shapes (it needs a rect, circle, ellipse, polygon, polyline or path)".to_owned());
    }
    Ok(Drawing { shapes })
}
