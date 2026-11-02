//! What an `svg` region makes of the drawing in a file, seen through the pieces it makes: the pixels of the pieces each `id`
//! names (a `Pick` of the region) are where the shape with that id shows. Placement and geometry are checked against the
//! SVG specification worked by hand: view boxes and `preserveAspectRatio` (SVG 1.1, 7.8), transforms, the fill rules and
//! the areas of circles, ellipses and rounded rectangles. (`limits.rs` has the files that are refused for their size or
//! what they declare; `svg.rs` the numbering of the pieces.)

use std::collections::BTreeSet;

use crate::common::Outcome;
use crate::common::ops::{Folder, float, probe, total};
use bullterrier_image::region::{Pick, Svg};
use bullterrier_image::{Error, Options, Region};

const NS: &str = r#"xmlns="http://www.w3.org/2000/svg""#;

/// `inside` in an svg with `attributes`.
fn svg(attributes: &str, inside: &str) -> String {
    format!("<svg {NS} {attributes}>{inside}</svg>")
}

/// The svg region of the file `file`, and a pick of the shapes with the ids `ids`, rendered on a grey photo of `height` x
/// `width` pixels (with the map `subject` if given): what the shapes with those ids choose, or the refusal.
fn pick(
    (height, width): (usize, usize),
    subject: Option<Vec<f32>>,
    region: Svg,
    ids: &[&str],
) -> Result<Vec<f32>, Error> {
    let pick = Pick {
        of: "s".to_owned(),
        // piece 0 when no id is asked for
        pieces: if ids.is_empty() {
            vec![0.into()]
        } else {
            ids.iter().map(|&id| id.into()).collect()
        },
    };
    let seen = probe(
        (height, width),
        subject,
        Options::default(),
        vec![("s", Region::from(region)), ("p", Region::from(pick))],
        (Some("s"), Some("p")),
    )?;
    seen.near
        .ok_or_else(|| Error::Recipe("the spy was handed no selection".to_owned()))
}

/// Writes `text` to a file of `folder` and gives the svg region that reads it.
fn region(folder: &Folder, text: &str) -> Result<Svg, std::io::Error> {
    Ok(Svg {
        file: folder.write("drawing.svg", text)?,
        ..Svg::default()
    })
}

/// The pixels that the shapes with the ids `ids` of the drawing `text` choose, on a square picture of `size`.
fn shown(text: &str, size: usize, ids: &[&str]) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    let folder = Folder::new("svg-drawing")?;
    Ok(pick((size, size), None, region(&folder, text)?, ids)?)
}

/// `(left, top, right, bottom)` in px of the pixels that `chosen` (`width` across) chooses.
fn extent(chosen: &[f32], width: usize) -> Option<[f64; 4]> {
    let mut found: Option<[usize; 4]> = None;
    for (at, _) in chosen.iter().enumerate().filter(|(_, value)| **value > 0.5) {
        let (row, column) = (at / width, at % width);
        found = Some(match found {
            None => [column, row, column + 1, row + 1],
            Some([left, top, right, bottom]) => [
                left.min(column),
                top.min(row),
                right.max(column + 1),
                bottom.max(row + 1),
            ],
        });
    }
    found.map(|extent| extent.map(float))
}

/// How many pixels `chosen` chooses.
fn area(chosen: &[f32]) -> f64 {
    float(chosen.iter().filter(|&&value| value > 0.5).count())
}

/// Whether the two extents agree to a thousandth of a pixel.
fn close(actual: Option<[f64; 4]>, expected: [f64; 4]) -> bool {
    actual.is_some_and(|actual| actual.iter().zip(expected).all(|(a, e)| (a - e).abs() < 1e-3))
}

/// The ids of the shapes of the drawing `text`, in order, as the refusal of an id it lacks lists them.
fn ids_of(text: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let folder = Folder::new("svg-drawing")?;
    let refused = pick((4, 4), None, region(&folder, text)?, &["#none"]);
    let message = refused.err().map(|error| error.to_string()).unwrap_or_default();
    Ok(match message.split_once("its ids: ") {
        Some((_, list)) => list.split(", ").map(str::to_owned).collect(),
        None => Vec::new(),
    })
}

#[test]
fn a_shape_is_placed_by_the_view_box_inside_the_frame_centred_by_default() -> Outcome {
    // a 2:1 view box fitted inside a square frame of 20 px: 20 wide, 10 tall, centred
    let text = svg(
        r#"viewBox="0 0 10 5""#,
        r#"<rect id="r" x="0" y="0" width="10" height="5"/>"#,
    );
    assert!(close(extent(&shown(&text, 20, &["r"])?, 20), [0.0, 5.0, 20.0, 15.0]));
    assert_eq!(ids_of(&text)?, ["r"]);
    Ok(())
}

/// The side of the square frame, in px: large enough for every edge of the shapes below to lie on a whole pixel.
const FRAME: f64 = 40.0;

/// Where the middle half of a `vw` x `vh` view box lands in a 40 px square frame under `preserveAspectRatio` with the
/// alignment shares `fx` and `fy` (0 for min, a half for mid, 1 for max), by the specification's rule: the scale is the
/// smaller of the frame's ratios to the view box for `meet`, the larger for `slice`, and the free room is shared by the
/// alignment. The part of it inside the frame, which is all a picture shows; none if there is none.
fn middle_half(vw: f64, vh: f64, (fx, fy): (f64, f64), slice: bool) -> Option<[f64; 4]> {
    let (ratio_x, ratio_y) = (FRAME / vw, FRAME / vh);
    let scale = if slice {
        ratio_x.max(ratio_y)
    } else {
        ratio_x.min(ratio_y)
    };
    let (left, top) = (fx * (FRAME - scale * vw), fy * (FRAME - scale * vh));
    let corners = [
        left + scale * vw / 4.0,
        top + scale * vh / 4.0,
        left + scale * vw * 0.75,
        top + scale * vh * 0.75,
    ];
    let clipped = corners.map(|value| value.clamp(0.0, FRAME));
    (clipped[0] < clipped[2] && clipped[1] < clipped[3]).then_some(clipped)
}

#[test]
fn preserve_aspect_ratio_places_the_view_box_as_the_svg_specification_says() -> Outcome {
    // a wide (10 x 5) and a tall (5 x 10) view box, every alignment, meet and slice; the shape is the middle half of the
    // view box, so that it shows where the view box lies even when slice makes the view box run past the frame
    for (vw, vh) in [(10.0, 5.0), (5.0, 10.0)] {
        for (x, fx) in [("xMin", 0.0), ("xMid", 0.5), ("xMax", 1.0)] {
            for (y, fy) in [("YMin", 0.0), ("YMid", 0.5), ("YMax", 1.0)] {
                for how in ["meet", "slice"] {
                    let text = svg(
                        &format!(r#"viewBox="0 0 {vw} {vh}" preserveAspectRatio="{x}{y} {how}""#),
                        &format!(
                            r#"<rect id="m" x="{}" y="{}" width="{}" height="{}"/>"#,
                            vw / 4.0,
                            vh / 4.0,
                            vw / 2.0,
                            vh / 2.0
                        ),
                    );
                    let expected = middle_half(vw, vh, (fx, fy), how == "slice");
                    let chosen = shown(&text, 40, &["m"])?;
                    let placed = extent(&chosen, 40);
                    assert_eq!(
                        placed.is_some(),
                        expected.is_some(),
                        "{x}{y} {how} {vw}x{vh}: {placed:?}"
                    );
                    if let Some(expected) = expected {
                        assert!(
                            close(placed, expected),
                            "{x}{y} {how} {vw}x{vh}: {placed:?} not {expected:?}"
                        );
                    }
                }
            }
        }
        // none: the view box is stretched to the frame, the middle half of it is the middle half of the frame
        let text = svg(
            &format!(r#"viewBox="0 0 {vw} {vh}" preserveAspectRatio="none""#),
            &format!(
                r#"<rect id="m" x="{}" y="{}" width="{}" height="{}"/>"#,
                vw / 4.0,
                vh / 4.0,
                vw / 2.0,
                vh / 2.0
            ),
        );
        assert!(
            close(extent(&shown(&text, 40, &["m"])?, 40), [10.0, 10.0, 30.0, 30.0]),
            "none {vw}x{vh}"
        );
    }
    Ok(())
}

#[test]
fn the_view_box_origin_and_a_frame_that_does_not_start_at_zero_are_honoured() -> Outcome {
    // a subject covering columns 10..20 and rows 4..14 of a 30 x 20 picture is a frame of 10 x 10 at (10, 4) for a 5 x 5
    // view box from (-1, -2): scale 2, so the rect from (0, 0) to (1, 1) lies from (10 + 2, 4 + 4) to (14, 10)
    let mut subject = vec![0.0_f32; 20 * 30];
    for row in 4..14 {
        subject[row * 30 + 10..row * 30 + 20].fill(1.0);
    }
    let folder = Folder::new("svg-drawing")?;
    let text = svg(
        r#"viewBox="-1 -2 5 5""#,
        r#"<rect id="r" x="0" y="0" width="1" height="1"/>"#,
    );
    let fitted = Svg {
        fit: Some("subject".to_owned()),
        ..region(&folder, &text)?
    };
    let chosen = pick((20, 30), Some(subject), fitted, &["r"])?;
    assert!(
        close(extent(&chosen, 30), [12.0, 8.0, 14.0, 10.0]),
        "{:?}",
        extent(&chosen, 30)
    );
    Ok(())
}

#[test]
fn without_a_view_box_the_width_and_height_are_the_view_box() -> Outcome {
    // a 4 x 2 drawing in a frame of 8 px: scale 2, centred, 8 wide and 4 tall
    for sized in [r#"width="4" height="2""#, r#"width="4px" height="2px""#] {
        let text = svg(sized, r#"<rect id="r" width="4" height="2"/>"#);
        assert!(
            close(extent(&shown(&text, 8, &["r"])?, 8), [0.0, 2.0, 8.0, 6.0]),
            "{sized}"
        );
    }
    Ok(())
}

#[test]
fn a_file_that_is_no_usable_svg_is_refused_naming_the_problem() -> Outcome {
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
    let folder = Folder::new("svg-drawing")?;
    for (what, text, mentions) in cases {
        let refused = pick((20, 20), None, region(&folder, &text)?, &[]);
        assert!(
            matches!(&refused, Err(error) if error.to_string().contains("region 's'") && error.to_string().contains(mentions)),
            "{what}: {refused:?}"
        );
    }
    Ok(())
}

#[test]
fn nested_groups_transforms_and_every_kind_of_shape_come_out_in_document_order() -> Outcome {
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
    let text = svg(r#"viewBox="0 0 100 100""#, inside);
    // the shapes with an id, in order; the path without one, the line and the text are no shape with an id, and what is
    // hidden is not drawn
    assert_eq!(ids_of(&text)?, ["rect", "circle", "ellipse", "polygon", "polyline"]);
    let rect = shown(&text, 100, &["rect"])?;
    assert!(close(extent(&rect, 100), [12.0, 12.0, 18.0, 20.0]));
    assert!((area(&rect) - 48.0).abs() < 1e-9);
    // pixels painted: pi r^2 and pi rx ry, within 1 % (the border pixels decide)
    let circle = area(&shown(&text, 100, &["circle"])?);
    assert!((circle / (std::f64::consts::PI * 100.0) - 1.0).abs() < 1e-2, "{circle}");
    let ellipse = area(&shown(&text, 100, &["ellipse"])?);
    assert!(
        (ellipse / (std::f64::consts::PI * 100.0) - 1.0).abs() < 2e-2,
        "{ellipse}"
    );
    // a triangle of legs 10 is 50 (pixels whose centres are inside; those on the diagonal go by the rule at edges); the
    // path after it lies over a part of it, the triangle of legs 4 (8 px), which is not the polygon's to show
    let triangle = area(&shown(&text, 100, &["polygon"])?);
    assert!((36.0..=48.0).contains(&triangle), "{triangle}");
    Ok(())
}

#[test]
fn a_rounded_rectangle_has_the_area_of_a_rectangle_less_its_four_corners() -> Outcome {
    // 80 x 60 with radii 10 x 10: 4800 - (4 - pi) * 100; a lone rx gives the same radius to ry
    for corners in [r#"rx="10" ry="10""#, r#"rx="10""#, r#"ry="10""#] {
        let text = svg(
            r#"viewBox="0 0 100 100""#,
            &format!(r#"<rect id="r" x="10" y="10" width="80" height="60" {corners}/>"#),
        );
        let painted = area(&shown(&text, 100, &["r"])?);
        let exact = 4800.0 - (4.0 - std::f64::consts::PI) * 100.0;
        assert!((painted / exact - 1.0).abs() < 4e-3, "{corners}: {painted} vs {exact}");
    }
    // radii beyond half the sides are cut to half: an ellipse of 40 x 30 radii, however large the radius asked
    let text = svg(
        r#"viewBox="0 0 100 100""#,
        r#"<rect id="r" x="10" y="10" width="80" height="60" rx="100"/>"#,
    );
    let painted = area(&shown(&text, 100, &["r"])?);
    assert!(
        (painted / (std::f64::consts::PI * 1200.0) - 1.0).abs() < 1e-2,
        "{painted}"
    );
    Ok(())
}

#[test]
fn a_rotation_turns_the_shape_about_its_centre() -> Outcome {
    let text = svg(
        r#"viewBox="0 0 100 100""#,
        r#"<g transform="rotate(90 50 50)"><rect id="r" x="10" y="40" width="30" height="10"/></g>"#,
    );
    // (x, y) goes to (100 - y, x)
    assert!(close(
        extent(&shown(&text, 100, &["r"])?, 100),
        [50.0, 10.0, 60.0, 40.0]
    ));
    Ok(())
}

#[test]
fn a_transform_applies_to_its_element_before_the_transforms_outside_it() -> Outcome {
    // the inner scale(2) first: (1, 1) to (2, 2), then the outer translate(10, 0): (12, 2). The other order would give
    // (22, 2)
    let text = svg(
        r#"viewBox="0 0 100 100""#,
        r#"<g transform="translate(10 0)"><rect id="r" transform="scale(2)" x="1" y="1" width="1" height="1"/></g>"#,
    );
    assert!(close(extent(&shown(&text, 100, &["r"])?, 100), [12.0, 2.0, 14.0, 4.0]));
    Ok(())
}

#[test]
fn shapes_in_defs_that_nothing_uses_are_not_counted_or_drawn() -> Outcome {
    let defs = format!("<defs>{}</defs>", r#"<rect width="1" height="1"/>"#.repeat(10));
    let text = svg(
        r#"viewBox="0 0 10 10""#,
        &format!(r#"{defs}<rect id="drawn" width="5" height="5"/>"#),
    );
    let folder = Folder::new("svg-drawing")?;
    let counted = Svg {
        max_shapes: 3,
        ..region(&folder, &text)?
    };
    let chosen = pick((10, 10), None, counted, &["drawn"])?;
    assert!(close(extent(&chosen, 10), [0.0, 0.0, 5.0, 5.0]));
    assert_eq!(ids_of(&text)?, ["drawn"]);
    Ok(())
}

/// Two circles of radii 40 and 20 about (50, 50), in one path: drawn the same way round.
const SAME_WAY: &str =
    "M 10 50 a 40 40 0 1 0 80 0 a 40 40 0 1 0 -80 0 Z M 30 50 a 20 20 0 1 0 40 0 a 20 20 0 1 0 -40 0 Z";
/// As above, the inner circle drawn the other way round (sweep flag 1 against 0).
const OTHER_WAY: &str =
    "M 10 50 a 40 40 0 1 0 80 0 a 40 40 0 1 0 -80 0 Z M 30 50 a 20 20 0 1 1 40 0 a 20 20 0 1 1 -40 0 Z";

#[test]
fn the_fill_rule_is_read_from_the_shape_or_its_style_and_inherited_from_the_group() -> Outcome {
    // the rule decides only a shape that crosses or encloses itself: the two circles drawn the same way round are a ring
    // under even-odd and a disc under nonzero. (shape markup, whether it is a ring)
    let ring = std::f64::consts::PI * (40.0 * 40.0 - 20.0 * 20.0);
    let disc = std::f64::consts::PI * 1600.0;
    let rows = [
        (
            format!(r#"<g fill-rule="evenodd"><path id="a" d="{SAME_WAY}"/></g>"#),
            true,
        ),
        (format!(r#"<path id="a" d="{SAME_WAY}"/>"#), false),
        (format!(r#"<path id="a" fill-rule="evenodd" d="{SAME_WAY}"/>"#), true),
        (
            format!(r#"<path id="a" style="fill-rule:evenodd" d="{SAME_WAY}"/>"#),
            true,
        ),
        (
            format!(r#"<g fill-rule="evenodd"><path id="a" fill-rule="nonzero" d="{SAME_WAY}"/></g>"#),
            false,
        ),
    ];
    for (inside, hollow) in rows {
        let painted = area(&shown(&svg(r#"viewBox="0 0 100 100""#, &inside), 100, &["a"])?);
        let expected = if hollow { ring } else { disc };
        assert!(
            (painted / expected - 1.0).abs() < 3e-3,
            "{inside}: {painted} vs {expected}"
        );
    }
    Ok(())
}

#[test]
fn a_shape_with_a_hole_leaves_the_hole_unfilled_under_even_odd_and_filled_under_nonzero() -> Outcome {
    let exact = std::f64::consts::PI * (40.0 * 40.0 - 20.0 * 20.0);
    let painted = |path: &str| -> Result<f64, Box<dyn std::error::Error>> {
        Ok(area(&shown(&svg(r#"viewBox="0 0 100 100""#, path), 100, &["a"])?))
    };
    let holed = painted(&format!(r#"<path id="a" fill-rule="evenodd" d="{OTHER_WAY}"/>"#))?;
    assert!((holed / exact - 1.0).abs() < 3e-3, "{holed} vs {exact}");
    // the inner circle is drawn the other way round, so nonzero leaves the hole as well
    let nonzero = painted(&format!(r#"<path id="a" d="{OTHER_WAY}"/>"#))?;
    assert!((nonzero / exact - 1.0).abs() < 3e-3, "{nonzero}");
    // two circles drawn the same way round: even-odd leaves a hole, nonzero fills it
    let evenodd = painted(&format!(r#"<path id="a" fill-rule="evenodd" d="{SAME_WAY}"/>"#))?;
    assert!((evenodd / exact - 1.0).abs() < 3e-3, "{evenodd}");
    let filled = painted(&format!(r#"<path id="a" fill-rule="nonzero" d="{SAME_WAY}"/>"#))?;
    assert!(
        (filled / (std::f64::consts::PI * 1600.0) - 1.0).abs() < 3e-3,
        "{filled}"
    );
    Ok(())
}

#[test]
fn a_used_shape_is_drawn_where_it_is_used_and_not_where_it_is_defined() -> Outcome {
    let text = svg(
        r#"viewBox="0 0 100 100""#,
        r##"<defs><rect id="r" width="10" height="10"/></defs><use href="#r" x="30" y="40"/><use href="#r" x="60" y="60"/>"##,
    );
    let chosen = shown(&text, 100, &["r"])?;
    // a used shape carries the id of the element it uses: both uses are the pieces of 'r', and nothing is at the origin
    let expected: Vec<f32> = (0..100 * 100)
        .map(|at| {
            let (row, column) = (at / 100, at % 100);
            f32::from(u8::from(
                ((40..50).contains(&row) && (30..40).contains(&column))
                    || ((60..70).contains(&row) && (60..70).contains(&column)),
            ))
        })
        .collect();
    assert_eq!(crate::common::ops::first_difference(&chosen, &expected), None);
    assert_eq!(ids_of(&text)?, ["r"]);
    Ok(())
}

#[test]
fn a_nested_svg_and_a_used_symbol_are_placed_by_their_own_view_boxes() -> Outcome {
    // a nested svg at (6, 4), 6 x 4, showing a 6 x 4 view box... scale 1
    let nested = svg(
        r#"viewBox="0 0 20 20""#,
        r#"<svg x="6" y="4" width="6" height="4" viewBox="0 0 3 2"><rect id="r" width="3" height="2"/></svg>"#,
    );
    assert!(close(extent(&shown(&nested, 20, &["r"])?, 20), [6.0, 4.0, 12.0, 8.0]));
    // a symbol of a 10 x 10 view box used at (2, 3) with width and height 6: scale 0.6 (the symbol is not drawn alone)
    let used = svg(
        r#"viewBox="0 0 20 20""#,
        r##"<symbol id="s" viewBox="0 0 10 10"><rect id="r" width="5" height="5"/></symbol><use href="#s" x="2" y="3" width="6" height="6"/>"##,
    );
    let chosen = shown(&used, 20, &["r"])?;
    assert!(close(extent(&chosen, 20), [2.0, 3.0, 5.0, 6.0]));
    assert!((area(&chosen) - 9.0).abs() < 1e-9);
    Ok(())
}

#[test]
fn the_shapes_of_a_realistic_file_come_in_document_order() -> Outcome {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/assets/mosaic.svg");
    let folder = Folder::new("svg-drawing")?;
    let text = std::fs::read_to_string(path)?;
    assert_eq!(
        ids_of(&text)?,
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
    // the 3:2 view box is fitted inside a 600 px square: 600 x 400 centred, scale 2, 100 px down. The group moves the sun's
    // circle by (-10, 5) before the view box scales it: its box is hand-computed, and the wave (later in the file) lies
    // over a part of it, so the sun shows within its box and keeps its leftmost, lowest and rightmost points
    let sun = pick((600, 600), None, region(&folder, &text)?, &["sun"])?;
    let hand = [
        2.0 * (255.0 - 10.0 - 28.0),
        100.0 + 2.0 * (48.0 + 5.0 - 28.0),
        2.0 * (255.0 - 10.0 + 28.0),
        100.0 + 2.0 * (48.0 + 5.0 + 28.0),
    ];
    let seen = extent(&sun, 600).ok_or("the sun shows nothing")?;
    assert!(
        seen[0] >= hand[0] - 0.5 && seen[1] >= hand[1] - 0.5 && seen[2] <= hand[2] + 0.5 && seen[3] <= hand[3] + 0.5,
        "{seen:?}"
    );
    assert!(
        (seen[0] - hand[0]).abs() < 0.5 && (seen[2] - hand[2]).abs() < 0.5 && (seen[3] - hand[3]).abs() < 0.5,
        "{seen:?} {hand:?}"
    );
    // the ring is even-odd: a disc of radius 32 less one of 16, scaled by 2, and under nothing; counted along 600 px
    let ring = area(&pick((600, 600), None, region(&folder, &text)?, &["ring"])?);
    let exact = 4.0 * std::f64::consts::PI * (32.0 * 32.0 - 16.0 * 16.0);
    assert!((ring / exact - 1.0).abs() < 5e-3, "{ring} vs {exact}");
    Ok(())
}

#[test]
fn a_vector_drawing_reads_as_its_shapes_with_paint_opacity_and_blend_mode_ignored() -> Outcome {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/svg/vector-drawing.svg");
    let text = std::fs::read_to_string(path)?;
    // none of its shapes has an id
    assert_eq!(ids_of(&text)?, Vec::<String>::new());
    // 1:1 in a frame of its own size (120 x 80): the three paths have the extents [10, 10, 70, 50], [40, 30, 110, 70] and
    // [20, 5, 100, 75] (see the file), so the pieces of the shapes together, which are all but the outside, have the box
    // of their union, whichever lies over which
    let folder = Folder::new("svg-drawing")?;
    let outside = pick((80, 120), None, region(&folder, &text)?, &[])?;
    let shapes: Vec<f32> = outside.iter().map(|value| 1.0 - value).collect();
    // (the apex of a path may lie in a row where no pixel centre is inside it: the box is within a pixel)
    let seen = extent(&shapes, 120).ok_or("the shapes show nothing")?;
    assert!(
        seen.iter()
            .zip([10.0, 5.0, 110.0, 75.0])
            .all(|(a, e)| (a - e).abs() <= 1.0),
        "{seen:?}"
    );
    assert!(total(&outside) > 0.0);
    Ok(())
}

#[test]
fn scripts_links_and_images_draw_nothing_and_a_link_around_a_shape_draws_the_shape() -> Outcome {
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
    assert_eq!(ids_of(&text)?, ["inside-a-link"]);
    Ok(())
}

#[test]
fn the_ids_are_listed_once_each_in_the_order_they_first_appear() -> Outcome {
    let text = svg(
        "viewBox='0 0 10 10'",
        r#"<rect id="b" width="1" height="1"/><rect id="a" width="1" height="1"/><rect id="b" x="2" width="1" height="1"/>"#,
    );
    let ids = ids_of(&text)?;
    assert_eq!(ids, ["b", "a"]);
    assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), ids.len());
    Ok(())
}

#[test]
fn a_transform_that_throws_a_finite_coordinate_far_away_is_refused() -> Outcome {
    // the corners of the shape are finite in the file; the scale makes them overflow, and a control point 1e30 away asks
    // for more chords than any picture needs
    let rows = [
        (
            r#"<g transform="scale(1e300)"><rect width="1e300" height="1"/></g>"#,
            "too far",
        ),
        (r#"<path d="M 0 0 C 0 0 1e30 1e30 0 0"/>"#, "too far"),
    ];
    let folder = Folder::new("svg-drawing")?;
    for (inside, mentions) in rows {
        let text = svg(r#"viewBox="0 0 10 10""#, inside);
        let refused = pick((10, 10), None, region(&folder, &text)?, &[]);
        assert!(
            matches!(&refused, Err(error) if error.to_string().contains(mentions)),
            "{inside}: {refused:?}"
        );
    }
    Ok(())
}
