//! `svg` regions, seen the way an effect sees them: through the public API alone, with an effect of the test's own that logs
//! the partition and the selection it is handed. The files are written to a folder of the test's own.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use bullterrier_image::region::{Edge, EdgeStyle, Pick, Svg};
use bullterrier_image::{
    Category, Effect, Error, FieldParam, Image, Input, Inputs, Layer, Options, Photo, Picture, Presets, Recipe, Step,
    register,
};

const NS: &str = r#"xmlns="http://www.w3.org/2000/svg""#;

/// What a [`Spy`] was handed: the labels and number of pieces of its partition, the values of its selection `near`.
#[derive(Clone, Debug)]
struct Seen {
    labels: Vec<i32>,
    pieces: usize,
    near: Option<Vec<f32>>,
}

/// An effect that logs what it is handed and returns the picture as it is.
struct Spy {
    name: String,
    fields: Vec<FieldParam>,
    log: Mutex<Vec<Seen>>,
}

impl Effect for Spy {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn inputs(&self) -> &[Input] {
        &[Input::Partition]
    }

    fn fields(&self) -> &[FieldParam] {
        &self.fields
    }

    fn apply(&self, picture: &Picture<'_>, _params: serde_json::Value, inputs: &Inputs<'_>) -> Result<Image, Error> {
        if let Some(labels) = inputs.partition() {
            self.log.lock().unwrap_or_else(PoisonError::into_inner).push(Seen {
                labels: labels.values().to_vec(),
                pieces: labels.pieces(),
                near: inputs.field("near").map(|layer| layer.values().to_vec()),
            });
        }
        Ok(picture.map(|pixel| pixel))
    }
}

/// A folder of the test's own, removed when it goes out of scope.
struct Folder(PathBuf);

impl Folder {
    fn new() -> std::io::Result<Self> {
        static FOLDERS: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "bt-image-svg-{}-{}",
            std::process::id(),
            FOLDERS.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    /// A folder inside this one, named `a-folder`, and its path.
    fn directory(&self) -> std::io::Result<String> {
        let path = self.0.join("a-folder");
        std::fs::create_dir_all(&path)?;
        Ok(path.to_string_lossy().into_owned())
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

/// `inside` in an svg of the view box `0 0 width height`.
fn drawing(width: usize, height: usize, inside: &str) -> String {
    format!(r#"<svg {NS} viewBox="0 0 {width} {height}">{inside}</svg>"#)
}

fn rect(id: &str, x: usize, y: usize, width: usize, height: usize) -> String {
    format!(r#"<rect id="{id}" x="{x}" y="{y}" width="{width}" height="{height}"/>"#)
}

/// The step of the spy: it takes the partition `s` and the selection `p` as `near`.
fn step(spy: &str) -> Step {
    let mut step = Step::new(spy).param("near", "p");
    step.partition = Some("s".to_owned());
    step
}

fn spy() -> Result<Arc<Spy>, Error> {
    static SPIES: AtomicUsize = AtomicUsize::new(0);
    let spy = Arc::new(Spy {
        name: format!("svg_spy_{}", SPIES.fetch_add(1, Ordering::Relaxed)),
        fields: vec![FieldParam::new("near", "NEAR")],
        log: Mutex::new(Vec::new()),
    });
    register(spy.clone())?;
    Ok(spy)
}

/// What the spy is handed when the svg region `svg` (as `s`) is rendered on a grey photo of `height` x `width` pixels, with
/// the pieces of the ids `ids` picked (as `p`) if there are some, and the map `subject` if given.
fn render(height: usize, width: usize, subject: Option<Vec<f32>>, svg: Svg, ids: &[&str]) -> Result<Seen, Error> {
    let spy = spy()?;
    let pixels = vec![0.5; height * width * 3];
    let mut photo = Photo::new(Picture::new(height, width, &pixels)?);
    if let Some(values) = subject {
        photo = photo.with_map("subject", Layer::new(height, width, values)?)?;
    }
    let mut recipe = Recipe::new([step(&spy.name)]).with_region("s", svg);
    recipe = recipe.with_region(
        "p",
        Pick {
            of: "s".to_owned(),
            pieces: if ids.is_empty() {
                vec![0.into()]
            } else {
                ids.iter().map(|&id| id.into()).collect()
            },
        },
    );
    photo.render(&recipe, &Presets::new(), Options::default())?;
    let mut seen = spy.log.lock().unwrap_or_else(PoisonError::into_inner);
    seen.pop()
        .ok_or_else(|| Error::Recipe("the spy was handed no partition".to_owned()))
}

fn svg_of(file: String) -> Svg {
    Svg { file, ..Svg::default() }
}

/// The pieces of a picture as one line of digits for each row, rows joined by `/`: labels below 10 only.
fn drawn(labels: &[i32], width: usize) -> String {
    labels
        .chunks_exact(width)
        .map(|row| row.iter().map(ToString::to_string).collect::<String>())
        .collect::<Vec<_>>()
        .join("/")
}

/// The pixels a selection chooses, as one line of `#` and `.` for each row, rows joined by `/`.
fn chosen(near: &[f32], width: usize) -> String {
    near.chunks_exact(width)
        .map(|row| {
            row.iter()
                .map(|&value| if value > 0.5 { '#' } else { '.' })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/")
}

type Outcome = Result<(), Box<dyn std::error::Error>>;

#[test]
fn the_area_outside_every_shape_is_one_more_piece_and_pieces_are_numbered_by_their_first_pixel() -> Outcome {
    let folder = Folder::new()?;
    // a shape at the top left is the first pixel, so it is piece 0 and the outside piece 1
    let corner = folder.write("corner.svg", drawing(8, 6, &rect("a", 0, 0, 3, 2)))?;
    let seen = render(6, 8, None, svg_of(corner), &[])?;
    assert_eq!(seen.pieces, 2);
    assert_eq!(
        drawn(&seen.labels, 8),
        "00011111/00011111/11111111/11111111/11111111/11111111"
    );
    // a shape elsewhere leaves the first pixel outside: the outside is piece 0
    let middle = folder.write("middle.svg", drawing(8, 6, &rect("a", 2, 1, 3, 2)))?;
    let seen = render(6, 8, None, svg_of(middle), &[])?;
    assert_eq!(
        drawn(&seen.labels, 8),
        "00000000/00111000/00111000/00000000/00000000/00000000"
    );
    Ok(())
}

#[test]
fn a_later_shape_lies_on_top_of_an_earlier_one_and_each_id_names_the_pieces_it_shows() -> Outcome {
    let folder = Folder::new()?;
    // a is 4 x 4 at the origin, b is 4 x 4 at (2, 2): where they overlap b shows
    let file = folder.write(
        "overlap.svg",
        drawing(6, 6, &format!("{}{}", rect("a", 0, 0, 4, 4), rect("b", 2, 2, 4, 4))),
    )?;
    let seen = render(6, 6, None, svg_of(file.clone()), &["a"])?;
    assert_eq!(seen.pieces, 3);
    assert_eq!(drawn(&seen.labels, 6), "000011/000011/002222/002222/112222/112222");
    // a shows its 16 - 4 pixels
    assert_eq!(
        chosen(seen.near.as_deref().unwrap_or_default(), 6),
        "####../####../##..../##..../....../......"
    );
    let b = render(6, 6, None, svg_of(file), &["b"])?;
    assert_eq!(
        chosen(b.near.as_deref().unwrap_or_default(), 6),
        "....../....../..####/..####/..####/..####"
    );
    Ok(())
}

#[test]
fn a_shape_covered_by_a_later_one_keeps_its_id_with_no_piece_and_picking_it_chooses_nothing() -> Outcome {
    let folder = Folder::new()?;
    let file = folder.write(
        "covered.svg",
        drawing(
            6,
            4,
            &format!("{}{}", rect("under", 1, 1, 2, 2), rect("over", 0, 0, 6, 4)),
        ),
    )?;
    let under = render(4, 6, None, svg_of(file.clone()), &["under"])?;
    // the later shape covers the picture, so it is the one piece
    assert_eq!(under.pieces, 1);
    assert_eq!(under.near.as_deref().unwrap_or_default(), [0.0; 24]);
    let over = render(4, 6, None, svg_of(file), &["over"])?;
    assert_eq!(over.near.as_deref().unwrap_or_default(), [1.0; 24]);
    Ok(())
}

#[test]
fn an_id_the_file_lacks_is_refused_listing_the_ids_it_has() -> Outcome {
    let folder = Folder::new()?;
    let named = folder.write(
        "named.svg",
        drawing(
            6,
            4,
            &format!("{}{}", rect("roof", 0, 0, 2, 2), rect("door", 3, 0, 2, 2)),
        ),
    )?;
    let refused = render(4, 6, None, svg_of(named), &["window"]);
    assert!(
        matches!(&refused, Err(error) if ["region 'p'", "'window'", "'s'", "roof, door"].iter().all(|word| error.to_string().contains(word))),
        "{refused:?}"
    );
    let anonymous = folder.write("anonymous.svg", drawing(6, 4, r#"<rect width="2" height="2"/>"#))?;
    let refused = render(4, 6, None, svg_of(anonymous), &["window"]);
    assert!(
        matches!(&refused, Err(error) if error.to_string().contains("no ids")),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_shape_painted_on_whole_pixels_has_exactly_its_area() -> Outcome {
    let folder = Folder::new()?;
    let file = folder.write("box.svg", drawing(12, 8, &rect("box", 1, 1, 7, 4)))?;
    let seen = render(8, 12, None, svg_of(file), &["box"])?;
    let near = seen.near.unwrap_or_default();
    assert_eq!(near.iter().filter(|&&value| value > 0.5).count(), 28);
    let band = ".#######....";
    let rows = [
        "............",
        band,
        band,
        band,
        band,
        "............",
        "............",
        "............",
    ];
    assert_eq!(chosen(&near, 12), rows.join("/"));
    Ok(())
}

#[test]
fn the_ids_of_a_realistic_file_name_disjoint_pieces_that_with_the_outside_cover_the_picture() -> Outcome {
    let path = format!("{}/../../examples/assets/mosaic.svg", env!("CARGO_MANIFEST_DIR"));
    let ids = [
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
    ];
    let (height, width) = (200, 300);
    let mut taken = vec![0_u8; height * width];
    let mut counts = Vec::new();
    for id in ids {
        let near = render(height, width, None, svg_of(path.clone()), &[id])?
            .near
            .unwrap_or_default();
        counts.push(near.iter().filter(|&&value| value > 0.5).count());
        for (count, value) in taken.iter_mut().zip(&near) {
            *count += u8::from(*value > 0.5);
        }
    }
    // no pixel is in two shapes' pieces
    assert!(taken.iter().all(|&count| count <= 1));
    // the outside (piece 0: the top left pixel is outside every shape) is what is left
    let outside = render(height, width, None, svg_of(path.clone()), &[])?
        .near
        .unwrap_or_default();
    let outside = outside.iter().filter(|&&value| value > 0.5).count();
    assert_eq!(counts.iter().sum::<usize>() + outside, height * width);
    // every shape shows something, and the pebble (an ellipse of radii 20 and 9, under nothing) has its area to 1.5 %
    assert!(counts.iter().all(|&count| count > 0), "{counts:?}");
    let pebble = f64::from(u32::try_from(counts[8])?);
    assert!(
        (pebble / (std::f64::consts::PI * 20.0 * 9.0) - 1.0).abs() < 1.5e-2,
        "{pebble}"
    );
    // the hole of the ring (a pixel at its centre) belongs to no shape of the ring, the band round it does
    let ring = render(height, width, None, svg_of(path), &["ring"])?
        .near
        .unwrap_or_default();
    assert!(ring[105 * width + 62] < 0.5 && ring[81 * width + 62] > 0.5);
    Ok(())
}

#[test]
fn the_drawing_is_placed_in_the_box_of_the_selection_it_is_fitted_to() -> Outcome {
    let folder = Folder::new()?;
    // a subject covering rows 2..6 and columns 4..12 of an 8 x 16 picture: a frame of 8 x 4, as the view box
    let mut subject = vec![0.0_f32; 8 * 16];
    for row in 2..6 {
        subject[row * 16 + 4..row * 16 + 12].fill(1.0);
    }
    let file = folder.write("fit.svg", drawing(8, 4, &rect("r", 0, 0, 4, 2)))?;
    let svg = Svg {
        fit: Some("subject".to_owned()),
        ..svg_of(file)
    };
    let near = render(8, 16, Some(subject), svg, &["r"])?.near.unwrap_or_default();
    let (empty, shown) = ("................", "....####........");
    let rows = [empty, empty, shown, shown, empty, empty, empty, empty];
    assert_eq!(chosen(&near, 16), rows.join("/"));
    Ok(())
}

#[test]
fn a_torn_edge_wanders_by_seed_and_tears_by_nothing_at_amount_zero() -> Outcome {
    let folder = Folder::new()?;
    let file = folder.write("tear.svg", drawing(128, 96, &rect("r", 30, 20, 60, 50)))?;
    let torn = |seed: u32, amount: f64| Svg {
        edge: Edge {
            style: EdgeStyle::Torn,
            amount,
            seed,
            ..Edge::default()
        },
        ..svg_of(file.clone())
    };
    let straight = render(96, 128, None, svg_of(file.clone()), &[])?;
    let nothing = render(96, 128, None, torn(1, 0.0), &[])?;
    let first = render(96, 128, None, torn(1, 12.0), &[])?;
    let again = render(96, 128, None, torn(1, 12.0), &[])?;
    let other = render(96, 128, None, torn(2, 12.0), &[])?;
    assert_eq!(straight.labels, nothing.labels);
    assert_eq!(first.labels, again.labels);
    assert_ne!(first.labels, straight.labels);
    assert_ne!(first.labels, other.labels);
    // the pieces still partition the picture and are numbered by first pixel
    assert!(first.labels.iter().all(|&label| (0..2).contains(&label)) && first.labels[0] == 0);
    Ok(())
}

#[test]
fn the_drawing_of_a_vector_program_renders_as_its_shapes_and_the_outside() -> Outcome {
    let path = format!("{}/tests/svg/vector-drawing.svg", env!("CARGO_MANIFEST_DIR"));
    // placed 1:1 in a picture of its own size (120 x 80): the three paths, all showing, and the outside
    let seen = render(80, 120, None, svg_of(path), &[])?;
    assert_eq!(seen.pieces, 4);
    Ok(())
}

#[test]
fn a_file_that_cannot_be_used_is_refused_naming_the_region_and_the_file_but_not_the_folder() -> Outcome {
    let folder = Folder::new()?;
    let broken = folder.write("broken.svg", "<svg")?;
    let rows = [
        (Svg::default().file, "`file` is not set"),
        (
            format!("{}/nowhere.svg", folder.0.display()),
            "svg file 'nowhere.svg' cannot be read",
        ),
        (broken, "svg file 'broken.svg' is not well-formed"),
        (folder.directory()?, "svg file 'a-folder' cannot be read"),
        ("a\0b.svg".to_owned(), "svg file 'a\0b.svg' cannot be read"),
    ];
    for (file, said) in rows {
        let refused = render(4, 6, None, svg_of(file), &[]);
        assert!(
            matches!(&refused, Err(error) if error.to_string().contains("region 's'") && error.to_string().contains(said)
                && !error.to_string().contains("bt-image-svg")),
            "{said}: {refused:?}"
        );
    }
    Ok(())
}
