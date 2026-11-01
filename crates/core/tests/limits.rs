//! Every number a user can set that drives allocation or work has a maximum, and this file holds each at its maximum
//! (accepted, and the render finishes) and one past it (refused, naming the field and the value). The files an svg region
//! reads are hostile ones, written to a folder of the test's own. Limits whose real value cannot be reached in a test
//! (crossings, painted pixels) are tested at a small value in the unit tests of `ops/svg/paint.rs`; the real constants are
//! reviewed, not run.
//!
//! Not run, and why: the 100 MB file of `max_kilobytes` and the 2^21 nodes of `Limits::nodes` are rows only where they
//! finish in a few seconds (see `a_file_of_the_most_kilobytes_is_read` and `a_file_of_the_most_nodes_is_parsed`).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use bullterrier_image::region::{
    Bands, Curve, Cuts, Edge, EdgeStyle, Grid, Line, Path, Pick, Region, Shards, Shares, Svg,
};
use bullterrier_image::{Each, Entry, Error, ErrorKind, Layer, Options, Photo, Picture, Presets, Recipe, Step};

type Outcome = Result<(), Box<dyn std::error::Error>>;

/// A folder of the test's own, removed when it goes out of scope.
struct Folder(PathBuf);

impl Folder {
    fn new() -> std::io::Result<Self> {
        static FOLDERS: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "bt-image-limits-{}-{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    /// Writes `content` to the file `name` and gives its path.
    fn write(&self, name: &str, content: impl AsRef<[u8]>) -> std::io::Result<String> {
        let path = self.0.join(name);
        std::fs::write(&path, content)?;
        Ok(path.to_string_lossy().into_owned())
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// Renders `region` (as `r`) on a grey photo of `height` x `width` pixels: a partition by an `each` step, a selection (a
/// `path`) as the mask of a step; with the map `subject` if given.
fn render(height: usize, width: usize, subject: Option<Vec<f32>>, region: impl Into<Region>) -> Result<(), Error> {
    let region = region.into();
    let step: Entry = if matches!(region, Region::Path(_)) {
        Step::new("grade").param("exposure", 0.5).mask("r").into()
    } else {
        Each::new("r", [None]).into()
    };
    let pixels = vec![0.5; height * width * 3];
    let values = subject.unwrap_or_else(|| vec![0.5; height * width]);
    let photo =
        Photo::new(Picture::new(height, width, &pixels)?).with_map("subject", Layer::new(height, width, values)?)?;
    photo
        .render(
            &Recipe::new([step]).with_region("r", region),
            &Presets::new(),
            Options::default(),
        )
        .map(drop)
}

/// The refusal of `result`: a recipe error containing every word of `says`.
fn refused(result: &Result<(), Error>, says: &[&str]) -> bool {
    matches!(result, Err(error) if error.kind() == ErrorKind::Recipe && says.iter().all(|word| error.to_string().contains(word)))
}

fn torn(octaves: u32, scale: f64, amount: f64) -> Edge {
    Edge {
        style: EdgeStyle::Torn,
        octaves,
        scale,
        amount,
        seed: 7,
        ..Edge::default()
    }
}

/// A zigzag path of `edges` edges across the unit square.
fn zigzag(edges: usize) -> String {
    let mut d = String::from("M 0 0");
    for edge in 0..edges {
        d.push_str(if edge % 2 == 0 { " L 1 0.5" } else { " L 0 0.5" });
    }
    d
}

/// One row of the table of limits: the `field` as the checks of `Region::check` name it, a region at its maximum (none
/// where the maximum is a file, which has rows of its own below), one past it, and what the refusal says; on a picture of
/// `size` (height, width).
struct Row {
    field: &'static str,
    size: (usize, usize),
    most: Option<Region>,
    past: Region,
    says: &'static [&'static str],
}

/// The fields whose maximum bounds an allocation or work, one row each.
#[expect(clippy::too_many_lines, reason = "a table of rows, one limit each")]
fn rows() -> Vec<Row> {
    let cuts = |count: u32| {
        Region::from(Cuts {
            lines: (0..count)
                .map(|at| {
                    let at = f64::from(at);
                    if at % 2.0 == 0.0 {
                        Line {
                            x: 0.5,
                            y: 0.5,
                            angle: 5.0 * at,
                        }
                        .into()
                    } else {
                        Curve {
                            path: format!("M {} 0 L {} 1", 0.03 * at, 0.03 * at + 0.02),
                        }
                        .into()
                    }
                })
                .collect(),
            ..Cuts::default()
        })
    };
    let grid = |edge: Edge| {
        Region::from(Grid {
            edge,
            ..Grid::default()
        })
    };
    vec![
        Row {
            field: "count",
            size: (48, 64),
            most: Some(
                Shards {
                    count: 500,
                    ..Shards::default()
                }
                .into(),
            ),
            past: Shards {
                count: 501,
                ..Shards::default()
            }
            .into(),
            says: &["`count` must be <= 500", "501"],
        },
        Row {
            field: "lines",
            size: (64, 96),
            most: Some(cuts(32)),
            past: cuts(33),
            says: &["`lines`", "33"],
        },
        Row {
            field: "samples",
            size: (96, 128),
            most: Some(
                Path {
                    d: zigzag(5000),
                    samples: 16,
                    ..Path::default()
                }
                .into(),
            ),
            past: Path {
                d: zigzag(5000),
                samples: 17,
                ..Path::default()
            }
            .into(),
            says: &["`samples` must be <= 16", "17"],
        },
        Row {
            field: "edge.octaves",
            size: (150, 200),
            most: Some(grid(torn(8, 1.0, 48.0))),
            past: grid(torn(9, 1.0, 48.0)),
            says: &["`edge.octaves` must be <= 8", "9"],
        },
        Row {
            field: "edge.scale",
            size: (150, 200),
            most: Some(grid(torn(8, 480.0, 48.0))),
            past: grid(torn(8, 481.0, 48.0)),
            says: &["`edge.scale` must be <= 480", "481"],
        },
        Row {
            field: "edge.amount",
            size: (150, 200),
            most: Some(grid(torn(8, 24.0, 48.0))),
            past: grid(torn(8, 24.0, 48.5)),
            says: &["`edge.amount` must be <= 48", "48.5"],
        },
        Row {
            field: "columns",
            size: (24, 64),
            most: Some(
                Grid {
                    columns: Shares(vec![1.0; 1000]),
                    ..Grid::default()
                }
                .into(),
            ),
            past: Grid {
                columns: Shares(vec![1.0; 1001]),
                ..Grid::default()
            }
            .into(),
            says: &["`columns`", "1000"],
        },
        Row {
            field: "rows",
            size: (64, 24),
            most: Some(
                Grid {
                    rows: Shares(vec![1.0; 1000]),
                    ..Grid::default()
                }
                .into(),
            ),
            past: Grid {
                rows: Shares(vec![1.0; 1001]),
                ..Grid::default()
            }
            .into(),
            says: &["`rows`", "1000"],
        },
        Row {
            field: "count",
            size: (24, 64),
            most: Some(
                Bands {
                    count: 100,
                    of: "subject".to_owned(),
                }
                .into(),
            ),
            past: Bands {
                count: 101,
                of: "subject".to_owned(),
            }
            .into(),
            says: &["`count` must be <= 100", "101"],
        },
        Row {
            field: "max_shapes",
            size: (24, 32),
            most: None,
            past: Svg {
                max_shapes: 100_001,
                ..Svg::default()
            }
            .into(),
            says: &["`max_shapes` must be <= 100000", "100001"],
        },
        Row {
            field: "max_kilobytes",
            size: (24, 32),
            most: None,
            past: Svg {
                max_kilobytes: 100_001,
                ..Svg::default()
            }
            .into(),
            says: &["`max_kilobytes` must be <= 100000", "100001"],
        },
    ]
}

#[test]
fn every_limit_is_accepted_at_its_maximum_and_refused_one_past_it_naming_the_field_and_the_value() {
    for row in rows() {
        let (height, width) = row.size;
        if let Some(most) = row.most.clone() {
            let done = render(height, width, None, most);
            assert!(done.is_ok(), "{} at its maximum: {done:?}", row.field);
        }
        let past = render(height, width, None, row.past.clone());
        assert!(refused(&past, row.says), "{} one past: {past:?}", row.field);
    }
}

#[test]
fn the_maxima_of_a_pick_and_of_the_edge_fields_that_cost_nothing_are_checked_the_same_way() {
    // pieces picked: 10000 numbers of a grid of 100 x 100 cells, and one more
    let pick = |count: u32| {
        let recipe = Recipe::new([Step::new("grade").param("exposure", 0.5).mask("p")])
            .with_region(
                "g",
                Grid {
                    columns: Shares(vec![1.0; 100]),
                    rows: Shares(vec![1.0; 100]),
                    ..Grid::default()
                },
            )
            .with_region(
                "p",
                Pick {
                    of: "g".to_owned(),
                    pieces: (0..count).map(Into::into).collect(),
                },
            );
        let pixels = vec![0.5; 100 * 100 * 3];
        Picture::new(100, 100, &pixels).and_then(|picture| {
            Photo::new(picture)
                .render(&recipe, &Presets::new(), Options::default())
                .map(drop)
        })
    };
    assert!(pick(10_000).is_ok());
    assert!(refused(&pick(10_001), &["`pieces`", "10001"]));
}

/// A grey photo whose map `subject` chooses two pixels at opposite corners: the box round them is the whole picture, the
/// chosen pixels are two.
fn corners_only(height: usize, width: usize) -> Vec<f32> {
    let mut values = vec![0.0; height * width];
    values[0] = 1.0;
    values[height * width - 1] = 1.0;
    values
}

#[test]
fn five_hundred_shards_in_two_pixels_of_a_big_picture_finish() {
    // a honeycomb fine enough to put 500 sites in two pixels of the 800 x 600 box would hold 120 million points (2 GB); it is
    // capped, and the rest drawn at random. (A picture of 2000 x 1500 takes 45 s in the debug profile: 3 million pixels
    // looked up among the sites)
    let shards = Shards {
        count: 500,
        fit: Some("subject".to_owned()),
        ..Shards::default()
    };
    let done = render(600, 800, Some(corners_only(600, 800)), shards);
    assert!(done.is_ok(), "{done:?}");
}

#[test]
fn a_torn_edge_at_its_largest_on_a_thin_picture_finishes() {
    let thin = Grid {
        edge: torn(8, 1.0, 48.0),
        columns: Shares(vec![1.0; 3]),
        ..Grid::default()
    };
    let done = render(3, 2000, None, thin);
    assert!(done.is_ok(), "{done:?}");
}

// ---- the files an svg region reads --------------------------------------------------------------------------------

const NS: &str = r#"xmlns="http://www.w3.org/2000/svg""#;

/// `inside` in an svg that fits a 100 x 100 picture as it is.
fn drawing(inside: &str) -> String {
    format!(r#"<svg {NS} viewBox="0 0 100 100">{inside}</svg>"#)
}

/// Renders the svg file `content` on a grey 100 x 100 photo, with `limits` (`max_kilobytes`, `max_shapes`) as given.
fn read(folder: &Folder, content: impl AsRef<[u8]>, max_shapes: u32, max_kilobytes: u32) -> Result<(), Error> {
    let file = folder
        .write("drawing.svg", content)
        .map_err(|error| Error::Recipe(error.to_string()))?;
    render(
        100,
        100,
        None,
        Svg {
            file,
            max_shapes,
            max_kilobytes,
            ..Svg::default()
        },
    )
}

fn read_standard(folder: &Folder, content: impl AsRef<[u8]>) -> Result<(), Error> {
    read(folder, content, 1000, 1024)
}

const RECT: &str = r#"<rect width="5" height="5"/>"#;

#[test]
fn files_that_declare_entities_are_refused_before_anything_is_expanded_or_fetched() -> Outcome {
    let folder = Folder::new()?;
    let billion = format!(
        r#"<?xml version="1.0"?><!DOCTYPE svg [<!ENTITY a "aaaaaaaaaa"><!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;"><!ENTITY c "&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;">]>
           <svg {NS} viewBox="0 0 10 10"><rect id="&c;" width="5" height="5"/></svg>"#
    );
    let system = format!(
        r#"<!DOCTYPE svg [<!ENTITY secret SYSTEM "file:///etc/passwd"><!ENTITY remote SYSTEM "http://127.0.0.1:9/x">]><svg {NS} viewBox="0 0 10 10"><rect id="&secret;" width="5" height="5"/></svg>"#
    );
    let public = format!(
        r#"<!DOCTYPE svg [<!ENTITY remote PUBLIC "-//X//Y" "http://127.0.0.1:9/x">]><svg {NS} viewBox="0 0 10 10">{RECT}</svg>"#
    );
    let parameter = format!(
        r#"<!DOCTYPE svg [<!ENTITY % x SYSTEM "http://127.0.0.1:9/x.dtd"> %x;]><svg {NS} viewBox="0 0 10 10">{RECT}</svg>"#
    );
    for (what, text) in [
        ("billion laughs", billion),
        ("SYSTEM", system),
        ("PUBLIC", public),
        ("parameter", parameter),
    ] {
        let result = read_standard(&folder, text);
        assert!(refused(&result, &["XML entities"]), "{what}: {result:?}");
    }
    // a document type that declares nothing is harmless: exporters write one, and its DTD is never fetched
    let harmless = format!(
        r#"<?xml version="1.0"?><!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://127.0.0.1:9/svg11.dtd"><svg {NS} viewBox="0 0 100 100">{RECT}</svg>"#
    );
    assert!(read_standard(&folder, harmless).is_ok());
    Ok(())
}

#[test]
fn a_file_that_is_not_utf_8_is_refused() -> Outcome {
    let folder = Folder::new()?;
    let text = drawing(RECT);
    let utf16: Vec<u8> = [0xFF, 0xFE]
        .into_iter()
        .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    let result = read_standard(&folder, utf16);
    assert!(refused(&result, &["not UTF-8"]), "{result:?}");
    assert!(read_standard(&folder, text).is_ok());
    Ok(())
}

#[test]
fn shapes_are_counted_before_uses_are_expanded_and_the_count_may_be_the_limit_but_no_more() -> Outcome {
    let folder = Folder::new()?;
    let rects = |count: usize| drawing(&RECT.repeat(count));
    assert!(read(&folder, rects(3), 3, 1024).is_ok());
    let result = read(&folder, rects(4), 3, 1024);
    assert!(refused(&result, &["max_shapes = 3", "4 shapes"]), "{result:?}");
    // ten groups each using the one before ten times: 10^10 shapes from a small file, refused without being expanded
    let mut levels = String::new();
    for level in 1..=10 {
        let uses = format!(r##"<use href="#g{}"/>"##, level - 1).repeat(10);
        let _ = write!(levels, r#"<g id="g{level}">{uses}</g>"#);
    }
    let multiplying = drawing(&format!(
        r##"<defs><rect id="g0" width="1" height="1"/>{levels}</defs><use href="#g10"/>"##
    ));
    let result = read(&folder, multiplying, 1000, 1024);
    assert!(refused(&result, &["max_shapes = 1000"]), "{result:?}");
    Ok(())
}

#[test]
fn a_use_that_contains_itself_is_refused() -> Outcome {
    let folder = Folder::new()?;
    let result = read_standard(
        &folder,
        drawing(r##"<g id="a"><rect width="1" height="1"/><use href="#a"/></g><use href="#a"/>"##),
    );
    assert!(refused(&result, &["contains itself"]), "{result:?}");
    Ok(())
}

#[test]
fn nesting_is_limited_to_64_levels_and_a_use_is_the_parent_of_what_it_uses() -> Outcome {
    let folder = Folder::new()?;
    // the svg is level 1 and the rect the last: `levels` in all
    let nested = |levels: usize| {
        drawing(&format!(
            "{}{RECT}{}",
            "<g>".repeat(levels - 2),
            "</g>".repeat(levels - 2)
        ))
    };
    assert!(read_standard(&folder, nested(64)).is_ok());
    for levels in [65, 1000] {
        let result = read_standard(&folder, nested(levels));
        assert!(refused(&result, &["deeper than 64"]), "{levels}: {result:?}");
    }
    // a chain of uses, each of the one before, ends at a rect: the svg is level 1, the use drawn 2, the rect n + 3
    let chain = |links: usize| {
        let mut defs = String::from(r#"<rect id="u0" width="1" height="1"/>"#);
        for link in 1..=links {
            let _ = write!(defs, r##"<use id="u{link}" href="#u{}"/>"##, link - 1);
        }
        drawing(&format!(r##"<defs>{defs}</defs><use href="#u{links}"/>"##))
    };
    assert!(read_standard(&folder, chain(61)).is_ok());
    let result = read_standard(&folder, chain(62));
    assert!(refused(&result, &["deeper than 64"]), "{result:?}");
    Ok(())
}

#[test]
fn coordinates_chords_and_corners_are_limited_each_at_its_value_and_one_past_it() -> Outcome {
    let folder = Folder::new()?;
    let path = |d: &str| drawing(&format!(r#"<path d="{d}"/>"#));
    // no corner or control point farther than 10^7 px from the origin
    assert!(read_standard(&folder, path("M 0 0 L 10000000 0")).is_ok());
    for d in ["M 0 0 L 10000001 0", "M 0 0 L 1e999 0"] {
        let result = read_standard(&folder, path(d));
        assert!(refused(&result, &["cannot be drawn"]), "{d}: {result:?}");
    }
    // a cubic from (0,0) with control points (0,0), (x,0) and the end (0,0) needs ceil(sqrt(30 x)) chords: 1024 up to
    // x = 1024^2 / 30 = 34952.53, 1025 from there
    assert!(read_standard(&folder, path("M 0 0 C 0 0 34952 0 0 0")).is_ok());
    let result = read_standard(&folder, path("M 0 0 C 0 0 34953 0 0 0"));
    assert!(refused(&result, &["1024 chords"]), "{result:?}");
    // 2^20 corners in all, a path of 6 MB
    let corners = |count: usize| {
        let mut d = String::from("M 5 5");
        for corner in 1..count {
            d.push_str(if corner % 2 == 1 { " L 6 5" } else { " L 5 6" });
        }
        path(&d)
    };
    let most = 1_usize << 20;
    assert!(read(&folder, corners(most), 1000, 100_000).is_ok());
    let result = read(&folder, corners(most + 1), 1000, 100_000);
    assert!(refused(&result, &["more than 1048576 corners"]), "{result:?}");
    Ok(())
}

#[test]
fn a_file_of_the_most_bytes_is_read_and_one_byte_more_is_refused() -> Outcome {
    let folder = Folder::new()?;
    let text = drawing(RECT);
    let padded = |size: usize| format!("{text}{}", " ".repeat(size - text.len()));
    assert!(read(&folder, padded(1024), 1000, 1).is_ok());
    let result = read(&folder, padded(1025), 1000, 1);
    assert!(refused(&result, &["max_kilobytes = 1"]), "{result:?}");
    Ok(())
}

#[test]
fn a_file_of_a_hundred_thousand_kilobytes_is_read() -> Outcome {
    let folder = Folder::new()?;
    let text = drawing(RECT);
    let size = 100_000 * 1024;
    let started = std::time::Instant::now();
    let done = read(
        &folder,
        format!("{text}{}", " ".repeat(size - text.len())),
        1000,
        100_000,
    );
    assert!(done.is_ok(), "{done:?} after {:?}", started.elapsed());
    let result = read(
        &folder,
        format!("{text}{}", " ".repeat(size + 1 - text.len())),
        1000,
        100_000,
    );
    assert!(refused(&result, &["max_kilobytes = 100000"]), "{result:?}");
    Ok(())
}

#[test]
fn a_hundred_thousand_shapes_are_drawn_and_one_more_is_refused() -> Outcome {
    let folder = Folder::new()?;
    let rects = |count: usize| drawing(&r#"<rect width="1" height="1"/>"#.repeat(count));
    let started = std::time::Instant::now();
    let done = read(&folder, rects(100_000), 100_000, 100_000);
    assert!(done.is_ok(), "{done:?} after {:?}", started.elapsed());
    let result = read(&folder, rects(100_001), 100_000, 100_000);
    assert!(
        refused(&result, &["max_shapes = 100000", "100001 shapes"]),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn a_file_of_the_most_nodes_is_parsed_and_one_more_is_refused() -> Outcome {
    let folder = Folder::new()?;
    // the document's own node, the svg and a rect are 3 of the 2^21
    let groups = |count: usize| drawing(&format!("{RECT}{}", "<g/>".repeat(count)));
    let most = (1_usize << 21) - 3;
    let started = std::time::Instant::now();
    let done = read(&folder, groups(most), 1000, 100_000);
    assert!(done.is_ok(), "{done:?} after {:?}", started.elapsed());
    let result = read(&folder, groups(most + 1), 1000, 100_000);
    assert!(refused(&result, &["more than 2097152 nodes"]), "{result:?}");
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_file_that_never_ends_is_refused_after_reading_only_the_limit() {
    let result = render(
        100,
        100,
        None,
        Svg {
            file: "/dev/zero".to_owned(),
            max_kilobytes: 4,
            ..Svg::default()
        },
    );
    assert!(refused(&result, &["max_kilobytes = 4"]), "{result:?}");
}

#[cfg(unix)]
#[test]
fn nothing_in_a_file_is_opened_not_an_image_a_link_or_a_use_of_another_file() -> Outcome {
    // a named pipe blocks whoever opens it for reading until something writes: if the reader followed a link to it, this
    // test would hang (and be killed by its deadline) instead of passing
    let folder = Folder::new()?;
    let pipe = folder.0.join("pipe");
    let made = std::process::Command::new("mkfifo").arg(&pipe).status()?;
    assert!(made.success());
    let path = pipe.to_string_lossy();
    let links = drawing(&format!(
        r#"<image href="{path}" width="5" height="5"/><image href="file://{path}" width="5" height="5"/>
           <use href="{path}#shape"/><a href="{path}">{RECT}</a><style>@import url({path});</style><script href="{path}"/>"#
    ));
    assert!(read_standard(&folder, links).is_ok());
    Ok(())
}

// ---- completeness ------------------------------------------------------------------------------------------------

/// The fields of the numeric bounds of `Region::check` that bound nothing allocated or run, with why. (The table above has
/// a row for every other.)
const COSTLESS: &[(&str, &str)] = &[
    ("jitter", "a share of a cell"),
    ("impact[0]", "a place in the frame"),
    ("impact[1]", "a place in the frame"),
    ("focus", "an exponent on a distance"),
    ("context", "a share of a piece's own size, within the picture"),
    ("threshold", "a comparison"),
    ("low", "a comparison"),
    ("high", "a comparison"),
    ("feather", "a comparison"),
    ("angle", "a direction"),
    ("x", "a place in the frame"),
    ("y", "a place in the frame"),
    ("radius", "a distance"),
    (
        "edge.persistence",
        "the weight of each octave, which costs the same whatever it is",
    ),
    (
        "of",
        "up to 1000 regions put together, each already declared and made once",
    ),
    (
        "pieces",
        "one flag for each piece of a partition of at most 10000 pieces: the pick test above",
    ),
];

/// The fields of `scanned` that `known` does not list.
fn missing<'a>(scanned: &[&'a str], known: &[&str]) -> Vec<&'a str> {
    scanned.iter().copied().filter(|field| !known.contains(field)).collect()
}

/// The names of the fields with a numeric bound in the source `code` of the checks: the first argument of `within`,
/// `count`, `shares` and `finite` when it is written out.
fn bounded_fields(code: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    for call in ["within(\"", "count(\"", "shares(\"", "finite(\""] {
        for (start, _) in code.match_indices(call) {
            fields.push(code[start + call.len()..].split('"').next().unwrap_or_default());
        }
    }
    fields.sort_unstable();
    fields.dedup();
    fields
}

#[test]
fn the_completeness_check_finds_a_bound_nobody_listed() {
    let code = r#"at.within("count", 1, 2, 3)?; at.count("lines", n, 1, 32)?; at.within("brand_new", 1, 2, 3)"#;
    let scanned = bounded_fields(code);
    assert_eq!(scanned, ["brand_new", "count", "lines"]);
    assert_eq!(missing(&scanned, &["count", "lines"]), ["brand_new"]);
    assert_eq!(missing(&scanned, &["count", "lines", "brand_new"]), Vec::<&str>::new());
}

/// An architecture test: it reads the source of the checks (`recipe/region/check.rs`) to find every field with a numeric
/// bound, as no public API lists them, and fails for one with neither a row of the table nor a reason in `COSTLESS`.
#[test]
fn every_numeric_bound_of_a_region_has_a_row_or_a_reason() -> Outcome {
    let source = std::fs::read_to_string(format!("{}/src/recipe/region/check.rs", env!("CARGO_MANIFEST_DIR")))?;
    let code = source.split("#[cfg(test)]").next().unwrap_or_default();
    let scanned = bounded_fields(code);
    let rows = rows();
    let known: Vec<&str> = rows
        .iter()
        .map(|row| row.field)
        .chain(COSTLESS.iter().map(|(field, _)| *field))
        .collect();
    assert_eq!(
        missing(&scanned, &known),
        Vec::<&str>::new(),
        "bounds with no row and no reason"
    );
    // and nothing listed is gone from the checks
    let gone: Vec<&str> = known.iter().copied().filter(|field| !scanned.contains(field)).collect();
    assert_eq!(
        gone,
        Vec::<&str>::new(),
        "rows or reasons for bounds that are not in check.rs any more"
    );
    Ok(())
}
