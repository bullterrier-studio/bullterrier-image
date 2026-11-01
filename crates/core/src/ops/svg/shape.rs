//! The shapes of an SVG file as path data: `rect`, `circle`, `ellipse`, `polygon` and `polyline` written as the path they
//! draw, so one reader flattens, limits and places them all.

use roxmltree::Node;
use svgtypes::{Length, LengthUnit, PointsParser};

/// The length in px the attribute `name` of `node` holds, if it has it.
pub(super) fn length(node: Node<'_, '_>, name: &str) -> Result<Option<f64>, String> {
    let Some(text) = node.attribute(name) else {
        return Ok(None);
    };
    match text.parse::<Length>() {
        Ok(Length {
            number,
            unit: LengthUnit::None | LengthUnit::Px,
        }) => Ok(Some(number)),
        _ => Err(format!("has a {name} ({text:?}) that is no number of px")),
    }
}

/// The path data of the shape `node` (an element called `name`), none if it draws nothing (no data, no size).
pub(super) fn path_data(node: Node<'_, '_>, name: &str) -> Result<Option<String>, String> {
    let number = |attribute: &str| length(node, attribute).map(|value| value.unwrap_or(0.0));
    let arcs = |rx: f64, ry: f64, x: f64, y: f64, large: u8| format!("A {rx} {ry} 0 {large} 0 {x} {y}");
    Ok(match name {
        "path" => node.attribute("d").map(str::to_owned),
        "rect" => {
            let (x, y, width, height) = (number("x")?, number("y")?, number("width")?, number("height")?);
            if !(width > 0.0 && height > 0.0) {
                return Ok(None);
            }
            // a corner takes the other's radius when it has none, and neither is more than half the side
            let (rx, ry) = match (length(node, "rx")?, length(node, "ry")?) {
                (Some(rx), Some(ry)) => (rx, ry),
                (Some(r), None) | (None, Some(r)) => (r, r),
                (None, None) => (0.0, 0.0),
            };
            let (rx, ry) = (rx.clamp(0.0, width / 2.0), ry.clamp(0.0, height / 2.0));
            if rx > 0.0 && ry > 0.0 {
                let (right, bottom) = (x + width, y + height);
                // clockwise on a screen: each corner is an arc of sweep 1
                let corner = |to_x: f64, to_y: f64| format!("A {rx} {ry} 0 0 1 {to_x} {to_y}");
                Some(format!(
                    "M {} {y} H {} {} V {} {} H {} {} V {} {} Z",
                    x + rx,
                    right - rx,
                    corner(right, y + ry),
                    bottom - ry,
                    corner(right - rx, bottom),
                    x + rx,
                    corner(x, bottom - ry),
                    y + ry,
                    corner(x + rx, y),
                ))
            } else {
                Some(format!("M {x} {y} H {} V {} H {x} Z", x + width, y + height))
            }
        }
        "circle" | "ellipse" => {
            let (cx, cy) = (number("cx")?, number("cy")?);
            let (rx, ry) = if name == "circle" {
                (number("r")?, number("r")?)
            } else {
                (number("rx")?, number("ry")?)
            };
            if !(rx > 0.0 && ry > 0.0) {
                return Ok(None);
            }
            // two half turns, the second back to the start
            Some(format!(
                "M {} {cy} {} {} Z",
                cx - rx,
                arcs(rx, ry, cx + rx, cy, 1),
                arcs(rx, ry, cx - rx, cy, 1)
            ))
        }
        _ => {
            // polygon and polyline: a corner for each pair of numbers
            let mut corners = PointsParser::from(node.attribute("points").unwrap_or(""));
            corners.next().map(|(x, y)| {
                let rest = corners.map(|(x, y)| format!("L {x} {y}"));
                std::iter::once(format!("M {x} {y}"))
                    .chain(rest)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        }
    })
}
