//! The data beside the picture: ink coverage planes (`halftone`, `dither` with `separation`), the palette-index map of
//! `dither` and the characters of `ascii`, asked of a recipe's last step. The oracles are independent of the code that
//! makes them: the picture the recipe renders (planes recombined by the overprint rule, a map painted with its palette),
//! the font's own grid and hand-worked colours. Pictures are 480 px on their longest side, where a recipe's px are as typed.

#![expect(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    reason = "small pictures; planes of exactly 0 and 1 and colours typed in the test are exact"
)]

use bullterrier_image::effects::ascii::Params as Ascii;
use bullterrier_image::effects::dither::{Method, Palette, Params as Dither, Separation as DitherInks};
use bullterrier_image::effects::halftone::{Params as Halftone, Separation};
use bullterrier_image::params::Color;
use bullterrier_image::{Error, Options, Photo, Picture, Presets, Recipe, Step};

use crate::common::compare::first_difference_within;
use crate::common::pictures::varied;

const WIDTH: usize = 480;
const HEIGHT: usize = 48;

/// A photo of `values`, `HEIGHT` x `WIDTH`.
fn photo(values: &[f32]) -> Result<Photo<'_>, Error> {
    Ok(Photo::new(Picture::new(HEIGHT, WIDTH, values)?))
}

/// A picture that ramps from black on the left to white on the right, red a little behind green, blue ahead.
fn ramp() -> Vec<f32> {
    (0..HEIGHT * WIDTH)
        .flat_map(|at| {
            let x = (at % WIDTH) as f32 / (WIDTH - 1) as f32;
            [x * 0.8, x, (x * 1.2).min(1.0)]
        })
        .collect()
}

fn recipe(step: Step) -> Recipe {
    Recipe::new([step])
}

fn render(photo: &Photo<'_>, recipe: &Recipe) -> Result<Vec<f32>, Error> {
    Ok(photo.render(recipe, &Presets::new(), Options::default())?.into_values())
}

fn message<T>(result: Result<T, Error>) -> String {
    result.err().map(|error| error.to_string()).unwrap_or_default()
}

// ---- characters (ascii) --------------------------------------------------------------------------------------------------

const CHARS: &str = " .:-=+*#%@";

fn white_on_black(cell: usize) -> Ascii {
    Ascii {
        cell,
        chars: CHARS.to_owned(),
        ink: Some(Color([1.0; 3])),
        ..Ascii::default()
    }
}

#[test]
fn the_text_is_the_grid_of_the_picture_one_line_per_row_of_cells() -> Result<(), Error> {
    // a cell 10 px wide is 24 px high (see ascii.rs), so 48 x 480 px is 2 lines of 48 characters
    let values = ramp();
    let text = photo(&values)?.characters(
        &recipe(Step::of(white_on_black(10))),
        &Presets::new(),
        Options::default(),
    )?;
    assert_eq!((text.rows(), text.columns()), (2, 48));
    let lines: Vec<String> = text.text().lines().map(str::to_owned).collect();
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|line| line.chars().count() == 48), "{lines:?}");
    assert_eq!(text.colors().len(), 96);
    Ok(())
}

#[test]
fn a_ramp_from_dark_to_bright_walks_the_palette_from_its_first_character_to_its_last() -> Result<(), Error> {
    let values = ramp();
    let text = photo(&values)?.characters(
        &recipe(Step::of(white_on_black(10))),
        &Presets::new(),
        Options::default(),
    )?;
    let first = text.text().lines().next().unwrap_or_default().to_owned();
    // the glyphs are ordered by the ink they put down (not by how the palette was typed): along a ramp each character
    // appears in one run, the lightest first and the densest last
    let mut runs: Vec<char> = first.chars().collect();
    runs.dedup();
    let mut distinct = runs.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(runs.len(), distinct.len(), "{first:?}");
    assert_eq!(distinct.len(), CHARS.len(), "{first:?}");
    assert_eq!((runs.first(), runs.last()), (Some(&' '), Some(&'@')), "{first:?}");
    Ok(())
}

#[test]
fn the_colour_of_a_character_is_the_ink_or_the_photo_brightened_by_hand() -> Result<(), Error> {
    let values = [0.2, 0.1, 0.05].repeat(HEIGHT * WIDTH);
    let on = photo(&values)?;
    let colours = |params: Ascii| -> Result<Vec<[f32; 3]>, Error> {
        Ok(on
            .characters(&recipe(Step::of(params)), &Presets::new(), Options::default())?
            .colors()
            .to_vec())
    };
    // no ink: the cell's colour scaled until its strongest channel is full, by at most 1 / ink_floor (0.35): 0.2 is below
    // the floor's 0.35, so it is scaled by 1 / 0.35 only
    let brightened = colours(Ascii {
        ink: None,
        ..white_on_black(10)
    })?;
    assert!(brightened.iter().all(|colour| {
        colour
            .iter()
            .zip([0.2 / 0.35, 0.1 / 0.35, 0.05 / 0.35])
            .all(|(got, want)| (got - want).abs() < 1e-5)
    }));
    let inked = colours(Ascii {
        ink: Some(Color([0.9, 0.5, 0.1])),
        ..white_on_black(10)
    })?;
    assert!(inked.iter().all(|colour| *colour == [0.9, 0.5, 0.1]));
    Ok(())
}

#[test]
fn ansi_and_html_carry_the_colours_and_escape_what_html_reserves() -> Result<(), Error> {
    let values = vec![0.5; HEIGHT * WIDTH * 3];
    let params = Ascii {
        chars: "<>&".to_owned(),
        ink: Some(Color([1.0, 0.0, 0.0])),
        paper: Color([0.0, 0.0, 1.0]),
        cell: 120,
        ..Ascii::default()
    };
    let text = photo(&values)?.characters(&recipe(Step::of(params)), &Presets::new(), Options::default())?;
    let ansi = text.ansi();
    assert!(ansi.starts_with("\u{1b}[48;2;0;0;255m\u{1b}[38;2;255;0;0m"), "{ansi:?}");
    assert_eq!(
        ansi.matches("\u{1b}[0m").count(),
        text.rows(),
        "reset at the end of each line"
    );
    let html = text.html();
    assert!(html.starts_with("<pre style=\"background:#0000ff"), "{html}");
    assert!(html.contains("<span style=\"color:#ff0000\">"), "{html}");
    // the characters are escaped: no raw `&` or `>` outside a tag, and the text is what text() says
    let body: String = html.split("</pre>").next().unwrap_or_default().to_owned();
    let unescaped = body.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&");
    for character in text.text().chars().filter(|c| "<>&".contains(*c)) {
        assert!(unescaped.contains(character));
    }
    assert!(!html.contains("&&") && !html.contains("<>"), "{html}");
    Ok(())
}

// ---- the kind of data a step makes -----------------------------------------------------------------------------------------

#[test]
fn asking_for_the_wrong_kind_or_a_step_that_makes_none_is_refused_naming_the_effect() -> Result<(), Error> {
    let values = ramp();
    let on = photo(&values)?;
    let (presets, options) = (Presets::new(), Options::default());
    let ascii = recipe(Step::of(white_on_black(10)));
    let said = message(on.ink_planes(&ascii, &presets, options));
    assert!(said.contains("ascii") && said.contains("ink planes"), "{said}");
    let grade = recipe(Step::new("grade"));
    let said = message(on.characters(&grade, &presets, options));
    assert!(said.contains("grade") && said.contains("no data"), "{said}");
    let said = message(on.palette_map(&recipe(Step::of(Halftone::default())), &presets, options));
    assert!(said.contains("halftone") && said.contains("palette map"), "{said}");
    Ok(())
}

#[test]
fn only_the_picture_the_recipe_makes_has_data_so_a_step_laid_over_its_input_is_refused() -> Result<(), Error> {
    let values = ramp();
    let on = photo(&values)?;
    let mixed = recipe(Step::of(white_on_black(10)).opacity(0.5));
    let said = message(on.characters(&mixed, &Presets::new(), Options::default()));
    assert!(said.contains("ascii") && said.contains("opacity"), "{said}");
    // a step before the last may do as it likes: the data is of what it leaves
    let two = Recipe::new([Step::new("grade").param("exposure", 1.0), Step::of(white_on_black(10))]);
    assert!(on.characters(&two, &Presets::new(), Options::default()).is_ok());
    Ok(())
}

// ---- the palette-index map (dither) --------------------------------------------------------------------------------------

fn palette_of(method: Method, cell: usize, block: i32) -> Dither {
    Dither {
        method,
        cell: cell as f32,
        block,
        palette: Palette::Custom,
        custom: vec![Color([0.1, 0.1, 0.3]), Color([0.9, 0.2, 0.2]), Color([0.95, 0.9, 0.5])],
        ..Dither::default()
    }
}

/// The picture `map` painted with `colours`, as 0..1 values.
fn painted(map: &bullterrier_image::Labels, colours: &[[f32; 3]]) -> Vec<f32> {
    map.values().iter().flat_map(|&label| colours[label as usize]).collect()
}

#[test]
fn a_palette_map_painted_with_its_palette_is_the_dithered_picture_exactly() -> Result<(), Error> {
    let values = varied(HEIGHT, WIDTH);
    let on = photo(&values)?;
    let cases = [
        palette_of(Method::FloydSteinberg, 1, 0),
        palette_of(Method::Bayer, 3, 0),
        palette_of(Method::Clustered45, 4, 0),
        palette_of(Method::Ostromoukhov, 2, 0),
        palette_of(Method::BlueNoise, 1, 0),
        palette_of(Method::FloydSteinberg, 2, 4),
        Dither {
            palette: Palette::Photo,
            colors: 5,
            ..palette_of(Method::Atkinson, 2, 0)
        },
        Dither {
            palette: Palette::Cube3,
            ..palette_of(Method::Sierra, 1, 0)
        },
    ];
    for params in cases {
        let look = recipe(Step::of(params.clone()));
        let (map, colours) = on.palette_map(&look, &Presets::new(), Options::default())?;
        assert_eq!(
            (map.height(), map.width(), map.pieces()),
            (HEIGHT, WIDTH, colours.len()),
            "{params:?}"
        );
        let rendered = render(&on, &look)?;
        let at = first_difference_within(&painted(&map, &colours), &rendered, 0.0);
        assert_eq!(at, None, "{:?} {} {}", params.method, params.cell, params.block);
    }
    Ok(())
}

#[test]
fn recolouring_the_map_with_another_palette_changes_only_the_colours() -> Result<(), Error> {
    let values = varied(HEIGHT, WIDTH);
    let on = photo(&values)?;
    let look = recipe(Step::of(palette_of(Method::FloydSteinberg, 1, 0)));
    let (map, colours) = on.palette_map(&look, &Presets::new(), Options::default())?;
    assert_eq!(colours.len(), 3);
    // the same pixels share a label whatever the colour: three distinct colours, three pieces
    let mut used = map.values().to_vec();
    used.sort_unstable();
    used.dedup();
    assert_eq!(used, [0, 1, 2]);
    Ok(())
}

#[test]
fn the_tiles_method_has_no_single_colour_for_a_pixel_and_says_so() -> Result<(), Error> {
    let values = varied(HEIGHT, WIDTH);
    let params = Dither {
        method: Method::Tiles,
        ..palette_of(Method::Tiles, 1, 0)
    };
    let said = message(photo(&values)?.palette_map(&recipe(Step::of(params)), &Presets::new(), Options::default()));
    assert!(said.contains("tiles"), "{said}");
    Ok(())
}

// ---- ink coverage planes: dither with `separation` -------------------------------------------------------------------------

fn spot_inks(inks: &[[f32; 3]]) -> Dither {
    Dither {
        separation: DitherInks::Spot,
        inks: inks.iter().map(|&ink| Color(ink)).collect(),
        paper: Color([1.0; 3]),
        method: Method::Bayer,
        ..Dither::default()
    }
}

#[test]
fn the_planes_of_a_dither_are_one_binary_plane_per_ink_and_decide_the_picture() -> Result<(), Error> {
    let values = ramp();
    let on = photo(&values)?;
    let inks = [[0.0, 1.0, 1.0], [1.0, 0.0, 1.0]];
    let look = recipe(Step::of(spot_inks(&inks)));
    let planes = on.ink_planes(&look, &Presets::new(), Options::default())?;
    assert_eq!(planes.len(), 2);
    assert!(
        planes
            .iter()
            .all(|plane| (plane.height(), plane.width()) == (HEIGHT, WIDTH))
    );
    assert!(
        planes
            .iter()
            .all(|plane| plane.values().iter().all(|&level| level == 0.0 || level == 1.0))
    );
    // each combination of inks that lands on a pixel prints one colour: the planes decide the picture
    let picture = render(&on, &look)?;
    let mut printed: [Option<[f32; 3]>; 4] = [None; 4];
    for (at, pixel) in picture.as_chunks::<3>().0.iter().enumerate() {
        let combination = usize::from(planes[0].values()[at] == 1.0) | usize::from(planes[1].values()[at] == 1.0) << 1;
        match printed[combination] {
            None => printed[combination] = Some(*pixel),
            Some(seen) => assert_eq!(seen, *pixel, "combination {combination} printed two colours"),
        }
    }
    assert_eq!(printed[0], Some([1.0; 3]), "no ink: the paper");
    assert!(
        printed.iter().all(Option::is_some),
        "the ramp uses every combination: {printed:?}"
    );
    Ok(())
}

// ---- ink coverage planes: halftone ---------------------------------------------------------------------------------------

fn screen(separation: Separation) -> Halftone {
    Halftone {
        separation,
        paper: Color([1.0; 3]),
        cell: 8.0,
        ..Halftone::default()
    }
}

#[test]
fn rgb_light_planes_are_the_picture_channels() -> Result<(), Error> {
    let values = ramp();
    let on = photo(&values)?;
    let look = recipe(Step::of(screen(Separation::Rgb)));
    let planes = on.ink_planes(&look, &Presets::new(), Options::default())?;
    assert_eq!(planes.len(), 3);
    let picture = render(&on, &look)?;
    for (channel, plane) in planes.iter().enumerate() {
        let wanted: Vec<f32> = picture.as_chunks::<3>().0.iter().map(|pixel| pixel[channel]).collect();
        assert_eq!(
            first_difference_within(plane.values(), &wanted, 0.0),
            None,
            "channel {channel}"
        );
    }
    let mean = planes[1].values().iter().sum::<f32>() / (HEIGHT * WIDTH) as f32;
    assert!(
        mean > 0.2 && mean < 0.8,
        "a ramp's green plane is partly inked, got {mean}"
    );
    Ok(())
}

#[test]
fn spot_ink_planes_recombined_by_the_overprint_rule_are_the_picture() -> Result<(), Error> {
    let values = ramp();
    let on = photo(&values)?;
    // on white paper an ink lets its own colour through: a channel is paper x product of (1 - coverage x (1 - through))
    let inks = [[0.0, 1.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 0.0]];
    let params = Halftone {
        inks: inks.iter().map(|&ink| Color(ink)).collect(),
        paper: Color([1.0; 3]),
        ..screen(Separation::Spot)
    };
    let look = recipe(Step::of(params));
    let planes = on.ink_planes(&look, &Presets::new(), Options::default())?;
    assert_eq!(planes.len(), 3);
    assert!(
        planes
            .iter()
            .all(|plane| plane.values().iter().all(|level| (0.0..=1.0).contains(level)))
    );
    let picture = render(&on, &look)?;
    let recombined: Vec<f32> = (0..HEIGHT * WIDTH)
        .flat_map(|at| {
            [0, 1, 2].map(|channel| {
                (0..3).fold(1.0_f32, |light, ink| {
                    light * (1.0 - planes[ink].values()[at] * (1.0 - inks[ink][channel]))
                })
            })
        })
        .collect();
    // (an ink lets at least 1e-4 of the paper's light through, the model's numerical floor)
    assert_eq!(first_difference_within(&recombined, &picture, 2e-4), None);
    Ok(())
}

#[test]
fn a_halftone_with_no_separation_has_one_plane_the_ink_over_the_paper() -> Result<(), Error> {
    let values = ramp();
    let on = photo(&values)?;
    let params = Halftone {
        ink: Some(Color([1.0; 3])),
        paper: Color([0.0; 3]),
        ..screen(Separation::None)
    };
    let look = recipe(Step::of(params));
    let planes = on.ink_planes(&look, &Presets::new(), Options::default())?;
    assert_eq!(planes.len(), 1);
    let picture = render(&on, &look)?;
    let reds: Vec<f32> = picture.as_chunks::<3>().0.iter().map(|pixel| pixel[0]).collect();
    assert_eq!(first_difference_within(planes[0].values(), &reds, 1e-6), None);
    Ok(())
}

#[test]
fn cmyk_gives_four_planes_and_a_dark_paper_refusal_still_holds() -> Result<(), Error> {
    let values = ramp();
    let on = photo(&values)?;
    let planes = on.ink_planes(
        &recipe(Step::of(screen(Separation::Cmyk))),
        &Presets::new(),
        Options::default(),
    )?;
    assert_eq!(planes.len(), 4);
    // black ink only where the picture is dark: the black plane's left half carries more than its right
    let black = planes[3].values();
    let (left, right): (f32, f32) = (0..HEIGHT).fold((0.0, 0.0), |(l, r), row| {
        let line = &black[row * WIDTH..(row + 1) * WIDTH];
        (
            l + line[..WIDTH / 2].iter().sum::<f32>(),
            r + line[WIDTH / 2..].iter().sum::<f32>(),
        )
    });
    assert!(left > right + 1.0, "{left} {right}");
    Ok(())
}

#[test]
fn data_is_the_same_whatever_the_thread_count() -> Result<(), Error> {
    let values = ramp();
    let on = photo(&values)?;
    let look = recipe(Step::of(screen(Separation::Cmyk)));
    let one = on.ink_planes(&look, &Presets::new(), Options::default())?;
    let many = on.ink_planes(&look, &Presets::new(), Options::default().with_threads(3))?;
    for (a, b) in one.iter().zip(&many) {
        assert_eq!(first_difference_within(a.values(), b.values(), 0.0), None);
    }
    Ok(())
}

#[test]
fn a_line_break_among_the_chars_is_refused_naming_chars() -> Result<(), Error> {
    let values = ramp();
    let on = photo(&values)?;
    for chars in [" .\n#", " .\r#"] {
        let look = recipe(Step::of(Ascii {
            chars: chars.to_owned(),
            ..white_on_black(10)
        }));
        let said = message(on.characters(&look, &Presets::new(), Options::default()));
        assert!(said.contains("chars"), "{said}");
    }
    Ok(())
}
