//! The pieces regions cut a picture into, seen the way an effect sees them: through the public API alone, with an effect
//! of the test's own that logs the partition and the map it is handed.

use std::sync::{Arc, Mutex, PoisonError};

use bullterrier_image::params::Table;
use bullterrier_image::region::{Bands, Given, Grid, Pick, Shares, Subject, Svg};
use bullterrier_image::{
    Category, Effect, Error, ErrorKind, FieldParam, Finder, Image, Input, Inputs, Layer, Options, Photo, Picture,
    Presets, Recipe, Step, register,
};
use proptest::prelude::*;
use proptest::test_runner::TestRunner;

type Outcome = Result<(), Box<dyn std::error::Error>>;

/// What a [`Spy`] was handed by one `apply`: the labels and number of pieces of its partition, the values of its map.
#[derive(Clone, Debug, PartialEq)]
struct Seen {
    labels: Option<(Vec<i32>, usize)>,
    near: Option<Vec<f32>>,
}

/// An effect that logs what it is handed and returns the picture as it is.
struct Spy {
    name: String,
    inputs: &'static [Input],
    fields: Vec<FieldParam>,
    log: Mutex<Vec<Seen>>,
}

impl Spy {
    /// A spy called `name` that takes a partition (`partitioned`) or a map `near`.
    fn register(name: &str, partitioned: bool) -> Result<Arc<Self>, Error> {
        let spy = Arc::new(Self {
            name: name.to_owned(),
            inputs: if partitioned { &[Input::Partition] } else { &[] },
            fields: if partitioned {
                vec![]
            } else {
                vec![FieldParam::new("near", "NEAR")]
            },
            log: Mutex::new(Vec::new()),
        });
        register(spy.clone())?;
        Ok(spy)
    }

    fn seen(&self) -> Vec<Seen> {
        self.log.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn clear(&self) {
        self.log.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }
}

impl Effect for Spy {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn inputs(&self) -> &[Input] {
        self.inputs
    }

    fn fields(&self) -> &[FieldParam] {
        &self.fields
    }

    fn params(&self) -> Table {
        crate::common::run::table_of_fields(&self.fields)
    }

    fn apply(&self, picture: &Picture<'_>, _params: serde_json::Value, inputs: &Inputs<'_>) -> Result<Image, Error> {
        self.log.lock().unwrap_or_else(PoisonError::into_inner).push(Seen {
            labels: inputs
                .partition()
                .map(|labels| (labels.values().to_vec(), labels.pieces())),
            near: inputs.field("near").map(|layer| layer.values().to_vec()),
        });
        Ok(picture.map(|pixel| pixel))
    }
}

/// A grey photo of `height` x `width` pixels.
fn grey(height: usize, width: usize) -> Vec<f32> {
    vec![0.5; height * width * 3]
}

/// The labels and number of pieces a `step` of the spy `spy` is handed when `recipe` is rendered on `photo`.
fn pieces_of(spy: &Spy, photo: &Photo<'_>, recipe: &Recipe, options: Options) -> Result<(Vec<i32>, usize), Error> {
    spy.clear();
    photo.render(recipe, &Presets::new(), options)?;
    spy.seen()
        .remove(0)
        .labels
        .ok_or_else(|| Error::Recipe("the spy was handed no partition".to_owned()))
}

/// A step of the partitioned spy `spy` that takes the region `region`.
fn partitioned(spy: &str, region: &str) -> Step {
    let mut step = Step::new(spy);
    step.partition = Some(region.to_owned());
    step
}

/// The grid-recipe of `spy` over `grid`, declared as `g`.
fn gridded(spy: &str, grid: Grid) -> Recipe {
    Recipe::new([partitioned(spy, "g")]).with_region("g", grid)
}

fn shares(list: &[f64]) -> Shares {
    Shares(list.to_vec())
}

/// The rectangle `fit` chooses is the frame; here the oracle for the pieces of a grid on `height` x `width` pixels, by
/// integer arithmetic: a pixel is past a border when its centre `(2 c + 1) / 2` is at or beyond it, and the border
/// after the first `k` of the shares is at `left + (right - left) * cum / total`.
fn brute_force(
    (height, width): (usize, usize),
    frame: (usize, usize, usize, usize),
    (columns, rows): (&[usize], &[usize]),
) -> (Vec<i32>, usize) {
    let (left, top, right, bottom) = frame;
    let place = |at: usize, low: usize, high: usize, shares: &[usize]| -> usize {
        let total: usize = shares.iter().sum();
        (1..shares.len())
            .filter(|&k| {
                let cum: usize = shares[..k].iter().sum();
                (2 * at + 1) * total >= 2 * (low * total + (high - low) * cum)
            })
            .count()
    };
    let mut seen: Vec<usize> = Vec::new();
    let mut labels = Vec::new();
    for row in 0..height {
        for column in 0..width {
            let cell = place(row, top, bottom, rows) * columns.len() + place(column, left, right, columns);
            let number = seen.iter().position(|&known| known == cell).unwrap_or_else(|| {
                seen.push(cell);
                seen.len() - 1
            });
            labels.push(i32::try_from(number).unwrap_or(i32::MAX));
        }
    }
    (labels, seen.len())
}

#[test]
fn a_grid_numbers_its_cells_row_after_row_and_a_centre_on_a_border_goes_to_the_later_one() -> Outcome {
    let spy = Spy::register("grid_hand", true)?;
    let pixels = grey(4, 6);
    let photo = Photo::new(Picture::new(4, 6, &pixels)?);
    let at = |columns: &[f64], rows: &[f64]| {
        let grid = Grid {
            columns: shares(columns),
            rows: shares(rows),
            ..Grid::default()
        };
        pieces_of(&spy, &photo, &gridded("grid_hand", grid), Options::default())
    };
    // 2 x 2 equal cells on 4 rows of 6 pixels: the border across is at 3 and the one down at 2, worked by hand
    let quarters = [
        [0, 0, 0, 1, 1, 1],
        [0, 0, 0, 1, 1, 1],
        [2, 2, 2, 3, 3, 3],
        [2, 2, 2, 3, 3, 3],
    ]
    .concat();
    assert_eq!(at(&[1.0, 1.0], &[1.0, 1.0])?, (quarters, 4));
    // shares 1 : 2 put the border across at 2
    let uneven = [[0, 0, 1, 1, 1, 1]; 4].concat();
    assert_eq!(at(&[1.0, 2.0], &[1.0])?, (uneven, 2));
    // three pixels across in two equal columns: the border is at 1.5, the centre of the middle pixel: it is past it
    let pixels = grey(1, 3);
    let thin = Photo::new(Picture::new(1, 3, &pixels[..9])?);
    let grid = Grid {
        columns: shares(&[1.0, 1.0]),
        ..Grid::default()
    };
    assert_eq!(
        pieces_of(&spy, &thin, &gridded("grid_hand", grid), Options::default())?,
        (vec![0, 1, 1], 2)
    );
    Ok(())
}

#[test]
fn more_cells_than_pixels_leave_only_the_cells_that_have_a_pixel_numbered_without_a_gap() -> Outcome {
    let spy = Spy::register("grid_small", true)?;
    let pixels = grey(1, 2);
    let photo = Photo::new(Picture::new(1, 2, &pixels)?);
    let grid = Grid {
        columns: Shares::equal(5),
        rows: Shares::equal(3),
        ..Grid::default()
    };
    // a 1 x 2 picture: centres 0.5 and 1.5 against borders at 0.4, 0.8, 1.2 and 1.6: columns 1 and 3 of five hold a pixel
    assert_eq!(
        pieces_of(&spy, &photo, &gridded("grid_small", grid), Options::default())?,
        (vec![0, 1], 2)
    );
    Ok(())
}

#[test]
fn a_grid_laid_over_a_selection_divides_its_bounding_box_and_the_outer_pieces_run_to_the_edge() -> Outcome {
    let spy = Spy::register("grid_fit", true)?;
    let pixels = grey(6, 8);
    // rows 2 to 3 and columns 3 to 5 chosen, one pixel of the box a little under half
    let mut chosen = vec![0.0_f32; 48];
    for row in 2..4 {
        for column in 3..6 {
            chosen[row * 8 + column] = 1.0;
        }
    }
    chosen[2 * 8 + 3] = 0.5;
    chosen[4 * 8] = 0.5; // exactly half is not chosen
    let photo = Photo::new(Picture::new(6, 8, &pixels)?).with_map("box", Layer::new(6, 8, chosen)?)?;
    let grid = Grid {
        fit: Some("box".to_owned()),
        columns: Shares::equal(2),
        rows: Shares::equal(2),
        ..Grid::default()
    };
    let recipe = gridded("grid_fit", grid).with_region("box", Given {});
    // the box is columns 3 to 6 (the corner pixel at 0.5 is not chosen, so its row still starts at 2 through the others)
    // and rows 2 to 4: the borders are at 4.5 across (centre of column 4: later) and 3 down; hand worked
    let top = [0, 0, 0, 0, 1, 1, 1, 1];
    let bottom = [2, 2, 2, 2, 3, 3, 3, 3];
    let expected: Vec<i32> = [top, top, top, bottom, bottom, bottom].concat();
    assert_eq!(pieces_of(&spy, &photo, &recipe, Options::default())?, (expected, 4));
    Ok(())
}

#[test]
fn a_fit_that_chooses_nothing_is_a_recipe_error_naming_the_region_and_the_fit_while_a_mask_of_nothing_is_none()
-> Outcome {
    let spy = Spy::register("grid_none", true)?;
    let pixels = grey(2, 3);
    // every value 0.5: not more than half, so nothing is chosen
    let photo = Photo::new(Picture::new(2, 3, &pixels)?).with_map("box", Layer::new(2, 3, vec![0.5; 6])?)?;
    let grid = Grid {
        fit: Some("box".to_owned()),
        ..Grid::default()
    };
    let recipe = gridded("grid_none", grid).with_region("box", Given {});
    let result = photo.render(&recipe, &Presets::new(), Options::default());
    assert!(
        matches!(&result, Err(error) if error.kind() == ErrorKind::Recipe
            && error.to_string().contains("region 'g'") && error.to_string().contains("`fit` 'box'")),
        "{result:?}"
    );
    assert_eq!(spy.seen().len(), 0);
    // an empty subject masks a step to nothing and is no error
    let masked = Recipe::new([Step::new("grade").mask("subject")]);
    let empty = Photo::new(Picture::new(2, 3, &pixels)?).with_map("subject", Layer::new(2, 3, vec![0.0; 6])?)?;
    assert_eq!(
        empty
            .render(&masked, &Presets::new(), Options::default())?
            .into_values(),
        pixels
    );
    Ok(())
}

#[test]
fn shares_of_any_size_divide_a_picture_as_their_proportions_say_and_a_border_a_hair_beyond_a_centre_leaves_it_before()
-> Outcome {
    let spy = Spy::register("grid_proportions", true)?;
    let pixels = grey(1, 1000);
    let photo = Photo::new(Picture::new(1, 1000, &pixels)?);
    let labels = |list: &[f64]| {
        let grid = Grid {
            columns: shares(list),
            ..Grid::default()
        };
        pieces_of(&spy, &photo, &gridded("grid_proportions", grid), Options::default())
    };
    // two equal shares are half and half whether they are 1 or 5e99 (the most they may add up to is 1e100)
    let halves = labels(&[1.0, 1.0])?;
    assert_eq!(halves.0.iter().filter(|&&label| label == 0).count(), 500);
    assert!(labels(&[5e99, 5e99])? == halves, "shares of 5e99");
    assert!(labels(&[1e-300, 1e-300])? == halves, "shares of 1e-300");
    // shares 4 : 1 : 4 : 5 on 21 pixels put a border at 21 x 9 / 14 = 13.5 exactly, a centre: the later piece (a share of the whole
    // taken first, 21 x (9 / 14), would be a hair off and put it in the earlier one)
    let narrow = grey(1, 21);
    let photo21 = Photo::new(Picture::new(1, 21, &narrow)?);
    let grid = Grid {
        columns: shares(&[4.0, 1.0, 4.0, 5.0]),
        ..Grid::default()
    };
    let by_21 = pieces_of(&spy, &photo21, &gridded("grid_proportions", grid), Options::default())?;
    assert!(
        by_21 == brute_force((1, 21), (0, 0, 21, 1), (&[4, 1, 4, 5], &[1])),
        "21 pixels in 4 : 1 : 4 : 5"
    );
    // shares 10,050,010 : 89,949,990 put the border at 100.5001, a hair beyond the centre of pixel 100 (100.5): that pixel
    // is still in the first column, by exact integer arithmetic on the shares (101 pixels)
    let (expected, _) = brute_force((1, 1000), (0, 0, 1000, 1), (&[10_050_010, 89_949_990], &[1]));
    assert_eq!(expected.iter().filter(|&&label| label == 0).count(), 101);
    assert!(
        labels(&[10_050_010.0, 89_949_990.0])?.0 == expected,
        "a border a hair beyond a centre"
    );
    Ok(())
}

#[test]
fn a_grid_whose_shares_divide_both_sizes_has_the_same_pieces_at_twice_the_size() -> Outcome {
    let spy = Spy::register("grid_sized", true)?;
    let pixels = grey(4, 6);
    let photo = Photo::new(Picture::new(4, 6, &pixels)?);
    let grid = Grid {
        columns: shares(&[1.0, 2.0, 3.0]),
        rows: shares(&[1.0, 1.0]),
        ..Grid::default()
    };
    let recipe = gridded("grid_sized", grid);
    let (small, count) = pieces_of(&spy, &photo, &recipe, Options::default().with_size(12))?;
    let (large, larger) = pieces_of(&spy, &photo, &recipe, Options::default().with_size(24))?;
    assert_eq!((small.len(), large.len(), count, larger), (8 * 12, 16 * 24, 6, 6));
    for row in 0..16 {
        for column in 0..24 {
            assert_eq!(
                large[row * 24 + column],
                small[(row / 2) * 12 + column / 2],
                "{row}, {column}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_grid_is_what_a_brute_force_scan_with_whole_numbers_gives_and_numbers_its_pieces_in_reading_order() -> Outcome {
    let spy = Spy::register("grid_brute", true)?;
    let shares_of = || prop::collection::vec(1_usize..=5, 1..=6);
    let index = || any::<prop::sample::Index>();
    let strategy = (
        (1_usize..=24, 1_usize..=24),
        shares_of(),
        shares_of(),
        any::<bool>(),
        (index(), index(), index(), index()),
    );
    TestRunner::default().run(&strategy, |((height, width), columns, rows, fitted, corners)| {
        // a rectangle of the picture, `left..right` across and `top..bottom` down, that a map chooses when the grid is
        // fitted to it
        let (a, b) = (corners.0.index(width), corners.1.index(width));
        let (c, d) = (corners.2.index(height), corners.3.index(height));
        let (left, right) = (a.min(b), a.max(b) + 1);
        let (top, bottom) = (c.min(d), c.max(d) + 1);
        let frame = if fitted {
            (left, top, right, bottom)
        } else {
            (0, 0, width, height)
        };
        let pixels = grey(height, width);
        let whole = |shares: &[usize]| {
            Shares(
                shares
                    .iter()
                    .map(|&share| f64::from(u32::try_from(share).unwrap_or(1)))
                    .collect(),
            )
        };
        let mut grid = Grid {
            columns: whole(&columns),
            rows: whole(&rows),
            ..Grid::default()
        };
        let fail = |error: Error| TestCaseError::fail(error.to_string());
        let mut photo = Photo::new(Picture::new(height, width, &pixels).map_err(fail)?);
        let mut recipe = Recipe::new([partitioned("grid_brute", "g")]);
        if fitted {
            let map = (0..height * width)
                .map(|at| {
                    f32::from(u8::from(
                        (left..right).contains(&(at % width)) && (top..bottom).contains(&(at / width)),
                    ))
                })
                .collect();
            photo = photo
                .with_map("box", Layer::new(height, width, map).map_err(fail)?)
                .map_err(fail)?;
            grid.fit = Some("box".to_owned());
            recipe = recipe.with_region("box", Given {});
        }
        let recipe = recipe.with_region("g", grid);
        let (labels, count) = pieces_of(&spy, &photo, &recipe, Options::default()).map_err(fail)?;
        let (expected, expected_count) = brute_force((height, width), frame, (&columns, &rows));
        prop_assert!(
            labels == expected && count == expected_count,
            "{}x{} frame {:?}",
            height,
            width,
            frame
        );
        // and the promises of any partition: at most as many pieces as asked for or pixels, the first pixel in piece 0,
        // every number used, and each number met for the first time after the one below it
        prop_assert!(count <= (columns.len() * rows.len()).min(height * width));
        prop_assert_eq!(labels[0], 0);
        let mut next = 0;
        for &label in &labels {
            if label == next {
                next += 1;
            } else {
                prop_assert!((0..next).contains(&label), "{} met before {}", label, next);
            }
        }
        prop_assert_eq!(usize::try_from(next).unwrap_or(0), count);
        Ok(())
    })?;
    Ok(())
}

/// The values a step's map `near` has when it names the region `name` of `recipe`, rendered on `photo`.
fn near_of(spy: &Spy, photo: &Photo<'_>, recipe: &Recipe) -> Result<Vec<f32>, Error> {
    spy.clear();
    photo.render(recipe, &Presets::new(), Options::default())?;
    spy.seen()
        .remove(0)
        .near
        .ok_or_else(|| Error::Recipe("the spy was handed no map".to_owned()))
}

/// A recipe of one step of `spy` with `near` set to `name`.
fn asking(spy: &str, name: &str) -> Recipe {
    Recipe::new([Step::new(spy).param("near", name)])
}

#[test]
fn bands_number_the_ranges_of_a_selection_from_the_highest_and_a_value_on_a_border_goes_to_the_lower() -> Outcome {
    let spy = Spy::register("bands_hand", true)?;
    let pixels = grey(1, 6);
    // 4 ranges: 0.75 to 1 is piece 0, 0.5 to 0.75 piece 1, 0.25 to 0.5 piece 2, 0 to 0.25 piece 3; 0.75, 0.5 and 0.25 are
    // on borders and belong to the range below them
    let depth = Layer::new(1, 6, vec![0.0, 0.25, 0.5, 0.75, 1.0, 0.3])?;
    let photo = Photo::new(Picture::new(1, 6, &pixels)?).with_map("depth", depth)?;
    let at = |count: u32, field: &str| {
        let recipe = Recipe::new([partitioned("bands_hand", "b")]).with_region(
            "b",
            Bands {
                of: field.to_owned(),
                count,
            },
        );
        pieces_of(&spy, &photo, &recipe, Options::default())
    };
    assert_eq!(at(4, "depth")?, (vec![3, 3, 2, 1, 0, 2], 4));
    // fewer ranges than there are: 2 ranges, 0.5 on the border goes to the lower
    assert_eq!(at(2, "depth")?, (vec![1, 1, 1, 0, 0, 1], 2));
    // one range is one piece
    assert_eq!(at(1, "depth")?, (vec![0; 6], 1));
    // a range no value lies in leaves no piece and the pieces below move up: 10 ranges, four used
    let sparse = Layer::new(1, 6, vec![1.0, 0.95, 0.0, 0.05, 0.5, 0.55])?;
    let photo = Photo::new(Picture::new(1, 6, &pixels)?).with_map("depth", sparse)?;
    let recipe = Recipe::new([partitioned("bands_hand", "b")]).with_region(
        "b",
        Bands {
            of: "depth".to_owned(),
            count: 10,
        },
    );
    assert_eq!(
        pieces_of(&spy, &photo, &recipe, Options::default())?,
        (vec![0, 0, 3, 3, 2, 1], 4)
    );
    // the ranges are worked out in double precision: of a hundred ranges 0.23000003 and 0.235 are both in the 77th from
    // the top (the 76th by number), though in single precision (1 - 0.23000003) * 100 rounds up to 77
    let hundred = Recipe::new([partitioned("bands_hand", "b")]).with_region(
        "b",
        Bands {
            of: "depth".to_owned(),
            count: 100,
        },
    );
    let close = Layer::new(
        1,
        6,
        vec![0.230_000_03, 0.235, 0.230_000_03, 0.235, 0.230_000_03, 0.235],
    )?;
    let photo = Photo::new(Picture::new(1, 6, &pixels)?).with_map("depth", close)?;
    assert_eq!(pieces_of(&spy, &photo, &hundred, Options::default())?, (vec![0; 6], 1));
    // and 1 - 0.15 is taken in double precision too, where single precision rounds it up before the product (85th)
    let close = Layer::new(1, 6, vec![0.15, 0.155, 0.15, 0.155, 0.15, 0.155])?;
    let photo = Photo::new(Picture::new(1, 6, &pixels)?).with_map("depth", close)?;
    assert_eq!(pieces_of(&spy, &photo, &hundred, Options::default())?, (vec![0; 6], 1));
    Ok(())
}

#[test]
fn a_pick_chooses_the_pixels_of_the_pieces_it_numbers_and_a_number_beyond_the_pieces_chooses_nothing() -> Outcome {
    let spy = Spy::register("pick_hand", false)?;
    let pixels = grey(2, 6);
    let photo = Photo::new(Picture::new(2, 6, &pixels)?);
    let grid = Grid {
        columns: Shares::equal(3),
        rows: Shares::equal(2),
        ..Grid::default()
    };
    let pick = |pieces: &[u32]| {
        let region = Pick {
            of: "g".to_owned(),
            pieces: pieces.iter().map(|&number| number.into()).collect(),
        };
        near_of(
            &spy,
            &photo,
            &asking("pick_hand", "p")
                .with_region("g", grid.clone())
                .with_region("p", region),
        )
    };
    // 3 x 2 cells of 2 x 1 pixels; piece 4 is the middle one of the bottom row
    let (on, off) = (1.0_f32, 0.0_f32);
    assert_eq!(pick(&[4])?, [[off; 6], [off, off, on, on, off, off]].concat());
    assert_eq!(
        pick(&[0, 5])?,
        [[on, on, off, off, off, off], [off, off, off, off, on, on]].concat()
    );
    // the last piece and the first, and a number the grid has no piece for
    assert_eq!(pick(&[5])?, [[off; 6], [off, off, off, off, on, on]].concat());
    assert_eq!(pick(&[6])?, vec![off; 12]);
    assert_eq!(pick(&[1000])?, vec![off; 12]);
    // an id names a shape of an svg region; a grid has none, and the recipe is refused before step 1 runs
    let by_id = Pick {
        of: "g".to_owned(),
        pieces: vec![0.into(), "roof".into()],
    };
    let photo = photo.with_map("subject", Layer::new(2, 6, vec![1.0; 12])?)?;
    let recipe = Recipe::new([
        Step::new("pick_hand").param("near", "subject"),
        Step::new("pick_hand").param("near", "subject"),
        Step::new("pick_hand").param("near", "subject"),
        Step::new("pick_hand").param("near", "p"),
    ])
    .with_region("g", grid)
    .with_region("p", by_id);
    spy.clear();
    let result = photo.render(&recipe, &Presets::new(), Options::default());
    assert!(
        matches!(&result, Err(error) if error.kind() == ErrorKind::Recipe
            && ["'p'", "'roof'", "'g'", "not an svg"].iter().all(|word| error.to_string().contains(word))),
        "{result:?}"
    );
    assert_eq!(spy.seen().len(), 0, "a step ran before the recipe was refused");
    // of an svg region an id passes the check; this svg has no file, so making it is what refuses the render
    let of_svg = Pick {
        of: "s".to_owned(),
        pieces: vec!["roof".into()],
    };
    let recipe = asking("pick_hand", "p")
        .with_region("s", Svg::default())
        .with_region("p", of_svg);
    let result = photo.render(&recipe, &Presets::new(), Options::default());
    assert!(
        matches!(&result, Err(error) if error.to_string().contains("region 's'") && error.to_string().contains("`file` is not set")),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn a_selection_used_as_a_partition_is_two_pieces_numbered_by_the_first_pixel_or_one_when_it_chooses_all_or_none()
-> Outcome {
    let spy = Spy::register("split_hand", true)?;
    let pixels = grey(1, 5);
    let photo_with = |values: Vec<f32>| -> Result<Photo<'_>, Error> {
        Photo::new(Picture::new(1, 5, &pixels)?).with_map("subject", Layer::new(1, 5, values)?)
    };
    let at = |values: Vec<f32>| {
        let recipe = Recipe::new([partitioned("split_hand", "subject")]);
        pieces_of(&spy, &photo_with(values)?, &recipe, Options::default())
    };
    // the chosen pixels are those above one half: 0.5 is not
    assert_eq!(at(vec![1.0, 0.0, 0.0, 0.7, 0.5])?, (vec![0, 1, 1, 0, 1], 2));
    // numbered by the first pixel, not by what is chosen
    assert_eq!(at(vec![0.0, 1.0, 1.0, 0.0, 0.2])?, (vec![0, 1, 1, 0, 0], 2));
    assert_eq!(at(vec![1.0; 5])?, (vec![0; 5], 1));
    assert_eq!(at(vec![0.0; 5])?, (vec![0; 5], 1));
    assert_eq!(at(vec![0.5; 5])?, (vec![0; 5], 1));
    Ok(())
}

#[test]
fn a_partition_is_refused_where_a_selection_is_needed_and_the_reverse_before_anything_runs() -> Outcome {
    let spy = Spy::register("typed", false)?;
    let pixels = grey(2, 2);
    let photo = Photo::new(Picture::new(2, 2, &pixels)?);
    let recipe = asking("typed", "g").with_region("g", Grid::default());
    let result = photo.render(&recipe, &Presets::new(), Options::default());
    assert!(
        matches!(&result, Err(error) if error.kind() == ErrorKind::Recipe && error.to_string().contains("partition")),
        "{result:?}"
    );
    assert_eq!(spy.seen().len(), 0);
    Ok(())
}

#[test]
fn a_pick_of_bands_chooses_a_range_of_the_selection_and_the_highest_is_piece_0() -> Outcome {
    let spy = Spy::register("pick_bands", false)?;
    let pixels = grey(1, 4);
    let depth = Layer::new(1, 4, vec![0.9, 0.2, 0.6, 0.3])?;
    let photo = Photo::new(Picture::new(1, 4, &pixels)?).with_map("depth", depth)?;
    let recipe = |piece: u32| {
        asking("pick_bands", "p")
            .with_region(
                "b",
                Bands {
                    of: "depth".to_owned(),
                    count: 2,
                },
            )
            .with_region(
                "p",
                Pick {
                    of: "b".to_owned(),
                    pieces: vec![piece.into()],
                },
            )
    };
    assert_eq!(near_of(&spy, &photo, &recipe(0))?, [1.0, 0.0, 1.0, 0.0]);
    assert_eq!(near_of(&spy, &photo, &recipe(1))?, [0.0, 1.0, 0.0, 1.0]);
    Ok(())
}

#[test]
fn a_grid_is_laid_over_the_bounding_box_of_a_selection_that_is_no_rectangle() -> Outcome {
    let spy = Spy::register("grid_box", true)?;
    let pixels = grey(6, 8);
    let at = |chosen: &[(usize, usize)], columns: usize, rows: usize| {
        let mut values = vec![0.0_f32; 48];
        for (row, column) in chosen {
            values[row * 8 + column] = 1.0;
        }
        let photo = Photo::new(Picture::new(6, 8, &pixels)?).with_map("box", Layer::new(6, 8, values)?)?;
        let grid = Grid {
            fit: Some("box".to_owned()),
            columns: Shares::equal(columns),
            rows: Shares::equal(rows),
            ..Grid::default()
        };
        pieces_of(
            &spy,
            &photo,
            &gridded("grid_box", grid).with_region("box", Given {}),
            Options::default(),
        )
    };
    // the widest row first, then a narrower one lower down: the box is columns 2 to 5 (6 across), so the border between
    // two columns is at 4 and the pixel at column 3, centre 3.5, is in the first
    let (labels, count) = at(&[(1, 2), (1, 5), (4, 3)], 2, 1)?;
    assert_eq!((labels[3], labels[4], count), (0, 1, 2));
    // the narrow row first, the wide one lower down: the border is the same
    let (labels, _) = at(&[(1, 3), (4, 2), (4, 5)], 2, 1)?;
    assert_eq!((labels[3], labels[4]), (0, 1));
    // one row: the box is that row, so the borders of three rows are at 2 1/3 and 2 2/3, and row 2 (centre 2.5) is
    // between them
    let (labels, count) = at(&[(2, 1), (2, 5)], 1, 3)?;
    let down: Vec<i32> = labels.iter().step_by(8).copied().collect();
    assert_eq!((down, count), (vec![0, 0, 1, 2, 2, 2], 3));
    // one column: the box is that column, 4 to 5, so the borders of three columns are at 4 1/3 and 4 2/3 and the pixel of the
    // column (centre 4.5) is the middle one
    let (labels, count) = at(&[(1, 4), (4, 4)], 3, 1)?;
    assert_eq!((labels[..8].to_vec(), count), (vec![0, 0, 0, 0, 1, 2, 2, 2], 3));
    Ok(())
}

/// What a [`Looks`] finder is asked and answers.
type Answer = Box<dyn Fn(&Picture<'_>) -> Result<Layer, Error> + Send + Sync>;

/// A subject finder that logs the size of each crop it is asked about, in order, and answers with `answer` of it.
struct Looks {
    asked: Mutex<Vec<(usize, usize)>>,
    answer: Answer,
}

impl Looks {
    fn new(answer: impl Fn(&Picture<'_>) -> Result<Layer, Error> + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self {
            asked: Mutex::new(Vec::new()),
            answer: Box::new(answer),
        })
    }

    /// A finder that knows the truth: 0 where the crop's red is up to one half, else the red.
    fn truth() -> Arc<Self> {
        Self::new(|picture| {
            Layer::new(
                picture.height(),
                picture.width(),
                picture
                    .values()
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|pixel| if pixel[0] > 0.5 { pixel[0] } else { 0.0 })
                    .collect(),
            )
        })
    }

    fn asked(&self) -> Vec<(usize, usize)> {
        self.asked.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

impl Finder for Looks {
    fn find(&self, picture: &Picture<'_>) -> Result<Layer, Error> {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((picture.height(), picture.width()));
        (self.answer)(picture)
    }
}

/// `grid` as `g` and the subject of each of its pieces as `found`, which a step of `spy` works from.
fn per_piece(spy: &str, grid: Grid, context: f64, threshold: f64) -> Recipe {
    asking(spy, "found").with_region("g", grid).with_region(
        "found",
        Subject {
            per: Some("g".to_owned()),
            context,
            threshold,
        },
    )
}

fn columns(across: &[f64]) -> Grid {
    Grid {
        columns: shares(across),
        ..Grid::default()
    }
}

/// A picture of `height` x `width` pixels whose red channel takes the values `reds` in turn, the others 0.5.
fn reddish(height: usize, width: usize, reds: &[f32]) -> Vec<f32> {
    (0..height * width)
        .flat_map(|at| [reds[at % reds.len()], 0.5, 0.5])
        .collect()
}

/// The truth about `pixels` (a finder that knows it): the red where it is above one half, else 0.
fn truth(pixels: &[f32]) -> Vec<f32> {
    pixels
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| if pixel[0] > 0.5 { pixel[0] } else { 0.0 })
        .collect()
}

#[test]
fn the_finds_of_all_the_pieces_are_united_into_what_a_finder_that_knows_the_truth_finds_in_the_whole() -> Outcome {
    let spy = Spy::register("per_truth", false)?;
    let reds = [0.9, 0.1, 0.55, 0.5, 0.3, 0.7, 0.2, 0.95, 0.5001];
    let pixels = reddish(9, 11, &reds);
    let mut runner = TestRunner::default();
    runner.run(
        &(
            1_usize..=4,
            1_usize..=3,
            prop::sample::select(vec![0.0, 0.1, 0.25, 0.5, 1.0]),
        ),
        |(across, down, context)| {
            let fail = |error: Error| TestCaseError::fail(error.to_string());
            let photo = Photo::new(Picture::new(9, 11, &pixels).map_err(fail)?).with_finder("subject", Looks::truth());
            let grid = Grid {
                columns: Shares::equal(across),
                rows: Shares::equal(down),
                ..Grid::default()
            };
            let made = near_of(&spy, &photo, &per_piece("per_truth", grid, context, 0.0)).map_err(fail)?;
            prop_assert!(made == truth(&pixels), "{across} x {down} pieces, context {context}");
            Ok(())
        },
    )?;
    Ok(())
}

#[test]
fn a_piece_is_looked_at_in_its_box_grown_by_the_context_rounded_to_the_even_pixel_and_clipped() -> Outcome {
    // (the spy is the effect of the steps, which the test does not look into)
    Spy::register("per_crops", false)?;
    let pixels = grey(4, 10);
    let asked = |across: &[f64], context: f64, options: Options| -> Result<Vec<(usize, usize)>, Error> {
        let finder = Looks::truth();
        let photo = Photo::new(Picture::new(4, 10, &pixels)?).with_finder("subject", finder.clone());
        photo.render(
            &per_piece("per_crops", columns(across), context, 0.0),
            &Presets::new(),
            options,
        )?;
        Ok(finder.asked())
    };
    let plain = Options::default();
    // two pieces of 4 x 5: the context is 2.5 pixels across, which goes to 2 (the even one, not 3), and 2 pixels down,
    // clipped at both ends of the 4 rows
    assert_eq!(asked(&[1.0, 1.0], 0.5, plain)?, [(4, 7), (4, 7)]);
    assert_eq!(asked(&[1.0, 1.0], 0.0, plain)?, [(4, 5), (4, 5)]);
    assert_eq!(asked(&[1.0, 1.0], 1.0, plain)?, [(4, 10), (4, 10)]);
    // the pieces are asked about in order, whatever their size: 3 wide, then 7
    assert_eq!(asked(&[1.0, 2.0], 0.0, plain)?, [(4, 3), (4, 7)]);
    // 0.25 of 3 is 0.75, which goes to 1; of 7 it is 1.75, which goes to 2 (clipped at the right edge)
    assert_eq!(asked(&[1.0, 2.0], 0.25, plain)?, [(4, 4), (4, 9)]);
    // the crop is of the working picture: at a size of 20 wide it is 8 high and the pieces are 10 wide each
    assert_eq!(
        asked(&[1.0, 1.0], 0.0, Options::default().with_size(20))?,
        [(8, 10), (8, 10)]
    );
    // down the picture it is the pieces' height that counts, not their width: a 10 x 6 picture in two rows of 5, context
    // 0.5 is 2.5 down (2) and 3 across (clipped to the 6)
    let tall = grey(10, 6);
    let finder = Looks::truth();
    let photo = Photo::new(Picture::new(10, 6, &tall)?).with_finder("subject", finder.clone());
    let rows = Grid {
        rows: shares(&[1.0, 1.0]),
        ..Grid::default()
    };
    photo.render(&per_piece("per_crops", rows, 0.5, 0.0), &Presets::new(), plain)?;
    assert_eq!(finder.asked(), [(7, 6), (7, 6)]);
    Ok(())
}

#[test]
fn every_piece_is_asked_once_however_often_the_region_is_used_and_a_handed_in_subject_does_not_stop_it() -> Outcome {
    let spy = Spy::register("per_once", false)?;
    let pixels = reddish(4, 6, &[0.9, 0.2, 0.7]);
    let finder = Looks::truth();
    let photo = Photo::new(Picture::new(4, 6, &pixels)?)
        .with_finder("subject", finder.clone())
        .with_map("subject", Layer::new(4, 6, vec![0.25; 24])?)?;
    let mut recipe = per_piece("per_once", columns(&[1.0, 1.0, 1.0]), 0.0, 0.0);
    recipe.steps.extend([
        Step::new("per_once").param("near", "found").into(),
        Step::new("per_once").param("near", "found").mask("found").into(),
    ]);
    photo.render(&recipe, &Presets::new(), Options::default())?;
    assert_eq!(finder.asked(), [(4, 2); 3]);
    // all three steps got the same map, which is the finds, not the handed in subject
    let seen = spy.seen();
    assert_eq!(seen.len(), 3);
    assert!(
        seen.iter()
            .all(|seen| seen.near.as_deref() == Some(truth(&pixels).as_slice()))
    );
    Ok(())
}

#[test]
fn only_the_pixels_of_the_piece_itself_count_whatever_else_its_view_shows() -> Outcome {
    let spy = Spy::register("per_own", false)?;
    let pixels = grey(4, 9);
    // every crop is answered with 1 over its width: 0.2 for the 5 wide crops, 1/7 for the middle one
    let finder = Looks::new(|picture| {
        Layer::new(
            picture.height(),
            picture.width(),
            vec![1.0 / 7.0; picture.height() * picture.width()],
        )
    });
    let photo = Photo::new(Picture::new(4, 9, &pixels)?).with_finder("subject", finder);
    let made = near_of(&spy, &photo, &per_piece("per_own", columns(&[1.0, 1.0, 1.0]), 0.5, 0.0))?;
    assert!(made.iter().all(|value| (value - 1.0 / 7.0).abs() < 1e-7), "{made:?}");
    // answered by the width of the crop (0.2, 1/7, 0.2), each piece's own columns carry its own answer
    let wide = Looks::new(|picture| {
        #[expect(clippy::cast_precision_loss, reason = "a test width")]
        let width = picture.width() as f32;
        Layer::new(
            picture.height(),
            picture.width(),
            vec![1.0 / width; picture.height() * picture.width()],
        )
    });
    let photo = Photo::new(Picture::new(4, 9, &pixels)?).with_finder("subject", wide);
    let made = near_of(&spy, &photo, &per_piece("per_own", columns(&[1.0, 1.0, 1.0]), 0.5, 0.0))?;
    let row: Vec<f32> = [vec![0.2; 3], vec![1.0 / 7.0; 3], vec![0.2; 3]].concat();
    for line in made.as_chunks::<9>().0 {
        assert!(line.iter().zip(&row).all(|(a, b)| (a - b).abs() < 1e-7), "{line:?}");
    }
    Ok(())
}

#[test]
fn a_piece_the_finder_finds_nothing_in_adds_nothing_and_the_others_are_unaffected() -> Outcome {
    let spy = Spy::register("per_none", false)?;
    let pixels = reddish(4, 9, &[0.9, 0.8, 0.7]);
    // nothing in the narrow piece (the first, 4 wide with its context), the truth in the others
    let finder = Looks::new(|picture| {
        if picture.width() == 4 {
            Err(Error::map_not_found("subject", "nothing stands out"))
        } else {
            Layer::new(
                picture.height(),
                picture.width(),
                picture
                    .values()
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|pixel| pixel[0])
                    .collect(),
            )
        }
    });
    let photo = Photo::new(Picture::new(4, 9, &pixels)?).with_finder("subject", finder.clone());
    // pieces of 2, 3 and 4 columns; context 0 so the crops are 2, 3 and 4 wide: only the last is 4
    let made = near_of(
        &spy,
        &photo,
        &per_piece("per_none", columns(&[2.0, 3.0, 4.0]), 0.0, 0.0),
    )?;
    assert_eq!(finder.asked(), [(4, 2), (4, 3), (4, 4)]);
    for line in made.as_chunks::<9>().0 {
        assert_eq!(line[..5], [0.9, 0.8, 0.7, 0.9, 0.8]);
        assert_eq!(line[5..], [0.0; 4]);
    }
    Ok(())
}

#[test]
fn an_answer_of_the_wrong_size_and_any_other_failure_stop_the_render() -> Outcome {
    let spy = Spy::register("per_bad", false)?;
    let pixels = grey(4, 6);
    let photo_with = |finder: Arc<Looks>| -> Result<Photo<'_>, Error> {
        Ok(Photo::new(Picture::new(4, 6, &pixels)?).with_finder("subject", finder))
    };
    let recipe = per_piece("per_bad", columns(&[1.0, 1.0]), 0.0, 0.0);
    // a map of 1 x 1 for a view of 4 x 3: the subject's error, naming the piece and both sizes
    let small = photo_with(Looks::new(|_| Layer::new(1, 1, vec![0.5])))?;
    let result = small.render(&recipe, &Presets::new(), Options::default());
    assert!(
        matches!(&result, Err(error) if error.kind() == ErrorKind::SubjectNotFound
            && ["piece 0", "1 x 1", "4 x 3"].iter().all(|word| error.to_string().contains(word))),
        "{result:?}"
    );
    // what is not "nothing found" is the render's error, as it is
    let broken = photo_with(Looks::new(|_| Err(Error::foreign("segmenter", "the model failed"))))?;
    let result = broken.render(&recipe, &Presets::new(), Options::default());
    assert!(
        matches!(&result, Err(error) if error.kind() == ErrorKind::Other && error.to_string().contains("the model failed")),
        "{result:?}"
    );
    assert_eq!(spy.seen().len(), 0);
    Ok(())
}

#[test]
fn a_pixel_counts_only_when_the_finder_is_surer_of_it_than_the_threshold_and_keeps_its_value() -> Outcome {
    let spy = Spy::register("per_threshold", false)?;
    let pixels = grey(1, 6);
    let answer = vec![0.0, 0.3, 0.31, 0.5, 0.8, 1.0];
    let at = |threshold: f64| {
        let answered = answer.clone();
        let finder = Looks::new(move |_| Layer::new(1, 6, answered.clone()));
        let photo = Photo::new(Picture::new(1, 6, &pixels)?).with_finder("subject", finder);
        near_of(
            &spy,
            &photo,
            &per_piece("per_threshold", Grid::default(), 0.0, threshold),
        )
    };
    // above the threshold, as found; a value equal to it is dropped (0.3 as the maps hold it is the threshold 0.3)
    assert_eq!(at(0.3)?, [0.0, 0.0, 0.31, 0.5, 0.8, 1.0]);
    assert_eq!(at(0.0)?, [0.0, 0.3, 0.31, 0.5, 0.8, 1.0]);
    assert_eq!(at(0.8)?, [0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    assert_eq!(at(1.0)?, [0.0; 6]);
    Ok(())
}

#[test]
fn a_photo_without_a_subject_finder_is_refused_before_anything_runs_even_with_a_subject_map() -> Outcome {
    let spy = Spy::register("per_nofinder", false)?;
    let pixels = grey(2, 2);
    let mut recipe = per_piece("per_nofinder", Grid::default(), 0.0, 0.0);
    recipe
        .steps
        .insert(0, Step::new("per_nofinder").param("near", "subject").into());
    for photo in [
        Photo::new(Picture::new(2, 2, &pixels)?),
        Photo::new(Picture::new(2, 2, &pixels)?).with_map("subject", Layer::new(2, 2, vec![1.0; 4])?)?,
    ] {
        let result = photo.render(&recipe, &Presets::new(), Options::default());
        assert!(
            matches!(&result, Err(error) if error.kind() == ErrorKind::SubjectNotFound
                && ["'found'", "'g'", "no subject finder"].iter().all(|word| error.to_string().contains(word))),
            "{result:?}"
        );
    }
    assert_eq!(spy.seen().len(), 0);
    // a region that is declared and not used needs no finder
    let unused = Recipe::new([Step::new("per_nofinder").param("near", "subject")])
        .with_region("g", Grid::default())
        .with_region(
            "found",
            Subject {
                per: Some("g".to_owned()),
                ..Subject::default()
            },
        );
    Photo::new(Picture::new(2, 2, &pixels)?)
        .with_map("subject", Layer::new(2, 2, vec![1.0; 4])?)?
        .render(&unused, &Presets::new(), Options::default())?;
    Ok(())
}

#[test]
fn the_subject_can_be_found_in_the_pieces_of_bands_whose_boxes_are_not_rectangles_of_equal_size() -> Outcome {
    let spy = Spy::register("per_bands", false)?;
    let pixels = reddish(1, 5, &[0.9, 0.2, 0.7, 0.3, 0.6]);
    let depth = Layer::new(1, 5, vec![0.9, 0.9, 0.2, 0.2, 0.9])?;
    let finder = Looks::truth();
    let photo = Photo::new(Picture::new(1, 5, &pixels)?)
        .with_finder("subject", finder.clone())
        .with_map("depth", depth)?;
    let recipe = asking("per_bands", "found")
        .with_region(
            "b",
            Bands {
                of: "depth".to_owned(),
                count: 2,
            },
        )
        .with_region(
            "found",
            Subject {
                per: Some("b".to_owned()),
                context: 0.0,
                threshold: 0.0,
            },
        );
    let made = near_of(&spy, &photo, &recipe)?;
    // the near band holds pixels 0, 1 and 4: its box is the whole row; the far band holds 2 and 3
    assert_eq!(finder.asked(), [(1, 5), (1, 2)]);
    assert_eq!(made, truth(&pixels));
    Ok(())
}
