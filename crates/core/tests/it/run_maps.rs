//! The maps of a photo, through the public API alone: handed in with `Photo::with_map`, found by a `Finder` (once, and
//! only when a step uses the map), or made from the picture (`background`, `light`), and held to the picture's size.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::common::Outcome;
use crate::common::run::{Finds, Paint, Recorder, Registered};
use bullterrier_image::region::{Given, Svg};
use bullterrier_image::{Each, Error, ErrorKind, Image, Layer, Options, Photo, Picture, Presets, Recipe, Step};

/// The values of a 4 x 6 picture, a different one in every channel (up to 71 / 80).
fn values() -> Vec<f32> {
    (0_u8..72).map(|index| f32::from(index) / 80.0).collect()
}

/// A map of the 4 x 6 picture whose every value is `level`.
fn flat(level: f32) -> Result<Layer, Error> {
    Layer::new(4, 6, vec![level; 24])
}

/// A `paint` and a `recorder` with the field parameter `near`, which logs the map it receives.
struct Rig {
    paint: Registered<Paint>,
    recorder: Registered<Recorder>,
}

fn rig() -> Result<Rig, Error> {
    Ok(Rig {
        paint: Registered::new(Paint::new())?,
        recorder: Registered::new(Recorder::new(&[], &["near"]))?,
    })
}

/// `recipe` rendered on `photo`.
fn render(photo: &Photo<'_>, recipe: &Recipe) -> Result<Image, Error> {
    photo.render(recipe, &Presets::new(), Options::default())
}

impl Rig {
    /// The maps the recorder received as its `near`, in order.
    fn seen(&self) -> Vec<Vec<f32>> {
        self.recorder.seen("near")
    }

    /// A step of `paint`, which needs no map.
    fn paint(&self) -> Step {
        Step::new(self.paint.name()).param("level", 0.5)
    }

    /// A step of the recorder that works from the map `map`.
    fn uses(&self, map: &str) -> Step {
        Step::new(self.recorder.name()).param("near", map)
    }
}

/// `steps` as a recipe that declares a `given` region for each of `regions`.
fn recipe(steps: impl IntoIterator<Item = Step>, regions: &[&str]) -> Recipe {
    let mut recipe = Recipe::new(steps);
    for name in regions {
        recipe.regions.insert(*name, Given {});
    }
    recipe
}

/// `result` is the error of `kind` whose message contains every one of `said`.
fn assert_refused<T>(result: &Result<T, Error>, kind: ErrorKind, said: &[&str], row: &str) {
    let shown = result.as_ref().map(|_| ());
    assert!(
        result
            .as_ref()
            .err()
            .is_some_and(|error| error.kind() == kind && said.iter().all(|word| error.to_string().contains(word))),
        "{row}: {shown:?}"
    );
}

/// Every map name a recipe can be given, with the kind of error its problems are.
const NAMES: [(&str, ErrorKind); 3] = [
    ("subject", ErrorKind::SubjectNotFound),
    ("depth", ErrorKind::DepthNotFound),
    ("star", ErrorKind::MapNotFound),
];

#[test]
fn a_finder_is_asked_once_for_a_map_however_many_steps_and_renders_use_it() -> Outcome {
    let rig = rig()?;
    let values = values();
    let finder = Arc::new(Finds::flat(0.25));
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", finder.clone());
    let twice = recipe([rig.uses("subject"), rig.uses("subject").mask("subject")], &[]);
    let once = recipe([rig.uses("subject")], &[]);
    render(&photo, &twice)?;
    render(&photo, &twice)?;
    render(&photo, &once)?;
    assert_eq!(finder.calls(), 1);
    // every use got the answer
    assert_eq!(rig.seen(), vec![vec![0.25; 24]; 5]);
    Ok(())
}

#[test]
fn a_map_handed_in_is_used_and_its_finder_is_never_asked() -> Outcome {
    let rig = rig()?;
    let values = values();
    let finder = Arc::new(Finds::flat(0.25));
    let photo = Photo::new(Picture::new(4, 6, &values)?)
        .with_finder("subject", finder.clone())
        .with_map("subject", flat(0.5)?)?;
    render(&photo, &recipe([rig.uses("subject")], &[]))?;
    assert_eq!(finder.calls(), 0);
    assert_eq!(rig.seen(), vec![vec![0.5; 24]]);
    Ok(())
}

#[test]
fn a_finder_is_not_asked_for_a_map_that_no_step_uses() -> Outcome {
    let rig = rig()?;
    let values = values();
    let (subject, depth) = (Arc::new(Finds::flat(0.25)), Arc::new(Finds::flat(0.75)));
    let photo = Photo::new(Picture::new(4, 6, &values)?)
        .with_finder("subject", subject.clone())
        .with_finder("depth", depth.clone());
    render(&photo, &recipe([rig.uses("depth")], &[]))?;
    assert_eq!((subject.calls(), depth.calls()), (0, 1));
    render(&photo, &recipe([rig.paint()], &[]))?;
    assert_eq!((subject.calls(), depth.calls()), (0, 1));
    Ok(())
}

#[test]
fn every_map_a_recipe_needs_is_found_before_its_first_step_runs() -> Outcome {
    let rig = rig()?;
    let values = values();
    // the finder notes how many steps had run when it was asked
    let (recorder, ran) = (rig.recorder.shared(), Arc::new(AtomicUsize::new(usize::MAX)));
    let noted = ran.clone();
    let finder = Finds::new(move |picture| {
        noted.store(recorder.received().len(), Ordering::SeqCst);
        Layer::new(picture.height(), picture.width(), vec![0.5; 24])
    });
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("depth", Arc::new(finder));
    render(&photo, &recipe([rig.paint(), rig.uses("depth")], &[]))?;
    assert_eq!(ran.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn a_finder_is_given_the_photo_as_it_is_and_its_map_is_what_the_steps_get() -> Outcome {
    let rig = rig()?;
    let values = values();
    // the map is the photo's red channel
    let finder = Finds::new(|picture| {
        let red = picture.values().chunks(3).map(|pixel| pixel[0]).collect();
        Layer::new(picture.height(), picture.width(), red)
    });
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("star", Arc::new(finder));
    render(
        &photo,
        &recipe(
            [
                Step::new(rig.paint.name()).param("level", 0.9).opacity(1.0),
                rig.uses("star"),
            ],
            &["star"],
        ),
    )?;
    let red: Vec<f32> = values.chunks(3).map(|pixel| pixel[0]).collect();
    assert_eq!(rig.seen(), vec![red]);
    Ok(())
}

#[test]
fn a_photo_shared_between_threads_still_asks_its_finder_once() -> Outcome {
    let rig = rig()?;
    let values = values();
    // the finder lingers, giving any thread that was let in beside it time to ask too
    let inside = Arc::new(AtomicUsize::new(0));
    let entered = inside.clone();
    let finder = Arc::new(Finds::new(move |picture| {
        entered.fetch_add(1, Ordering::SeqCst);
        for _ in 0..20_000 {
            if entered.load(Ordering::SeqCst) > 1 {
                break;
            }
            std::thread::yield_now();
        }
        Layer::new(picture.height(), picture.width(), vec![0.25; 24])
    }));
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", finder.clone());
    let look = recipe([rig.uses("subject")], &[]);
    let results: Vec<Result<Image, Error>> = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..8).map(|_| scope.spawn(|| render(&photo, &look))).collect();
        threads.into_iter().filter_map(|thread| thread.join().ok()).collect()
    });
    assert_eq!(results.len(), 8);
    assert!(results.iter().all(Result::is_ok));
    assert_eq!(finder.calls(), 1);
    assert_eq!(inside.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn the_background_is_one_minus_the_subject() -> Outcome {
    let rig = rig()?;
    let values = values();
    // quarters, exact in f32: a subject of 0, 1/4, 1/2, 3/4, 1 and 1/4 again, repeated
    let subject: Vec<f32> = (0..24)
        .map(|index| [0.0, 0.25, 0.5, 0.75, 1.0, 0.25][index % 6])
        .collect();
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_map("subject", Layer::new(4, 6, subject.clone())?)?;
    render(&photo, &recipe([rig.uses("background"), rig.uses("subject")], &[]))?;
    let seen = rig.seen();
    let background: Vec<f32> = subject.iter().map(|value| 1.0 - value).collect();
    assert_eq!(seen[0], background);
    assert_eq!(seen[1], subject);
    assert_eq!(&seen[0][..6], [1.0, 0.75, 0.5, 0.25, 0.0, 0.75]);
    Ok(())
}

#[test]
fn the_light_is_the_rec_709_brightness_of_the_picture_as_it_was_given() -> Outcome {
    let rig = rig()?;
    // red, green, blue, white and a mixed pixel: brightness 0.2126 r + 0.7152 g + 0.0722 b, worked by hand from the
    // Rec. 709 definition (the last: 0.1063 + 0.1788 + 0.0722)
    let pixels = [
        1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.5, 0.25, 1.0,
    ];
    let photo = Photo::new(Picture::new(1, 5, &pixels)?);
    // a step before the one that asks changes the picture; the light is still the photo's
    let recipe = recipe(
        [
            Step::new(rig.paint.name()).param("level", 0.9).opacity(1.0),
            rig.uses("light"),
        ],
        &[],
    );
    render(&photo, &recipe)?;
    let expected = [0.2126, 0.7152, 0.0722, 1.0, 0.3573];
    let seen = &rig.seen()[0];
    assert!(
        seen.iter()
            .zip(expected)
            .all(|(light, hand)| (light - hand).abs() < 1e-6),
        "{seen:?}"
    );
    Ok(())
}

#[test]
fn a_picture_that_is_no_picture_has_no_light() -> Outcome {
    let rig = rig()?;
    for (pixel, row) in [([1.5, 1.5, 1.5], "above 1"), ([-0.5, 0.0, 0.0], "below 0")] {
        let photo = Photo::new(Picture::new(1, 1, &pixel)?);
        let result = render(&photo, &recipe([rig.uses("light")], &[]));
        assert_refused(&result, ErrorKind::InvalidImage, &["light"], row);
    }
    assert_eq!(rig.recorder.received().len(), 0);
    Ok(())
}

#[test]
fn a_map_nobody_gave_is_refused_as_that_maps_error_naming_the_recipe_and_nothing_runs() -> Outcome {
    let rig = rig()?;
    let values = values();
    let photo = Photo::new(Picture::new(4, 6, &values)?);
    // (what the step works from, the map that is missing, the error the package raises it as)
    let rows = [
        ("subject", "subject", ErrorKind::SubjectNotFound),
        ("background", "subject", ErrorKind::SubjectNotFound),
        ("depth", "depth", ErrorKind::DepthNotFound),
        ("star", "star", ErrorKind::MapNotFound),
    ];
    for (map, missing, kind) in rows {
        let mut look = recipe([rig.paint(), rig.uses(map)], &["star"]);
        look.name = Some("dusk".to_owned());
        assert_refused(&render(&photo, &look), kind, &[&format!("'{missing}'"), "'dusk'"], map);
        // a mask and a drive ask for it the same way
        let masked = recipe([rig.paint().mask(map)], &["star"]);
        assert_refused(&render(&photo, &masked), kind, &[&format!("'{missing}'")], map);
    }
    assert_eq!(rig.recorder.received().len(), 0);
    Ok(())
}

#[test]
fn a_map_of_another_size_than_the_picture_is_refused_when_it_is_handed_in_used_or_not() -> Outcome {
    let values = values();
    for (name, kind) in NAMES {
        // too small, and the picture turned on its side
        for (height, width) in [(2, 2), (6, 4), (4, 3)] {
            let result = Photo::new(Picture::new(4, 6, &values)?)
                .with_map(name, Layer::new(height, width, vec![0.5; height * width])?);
            let said = [format!("'{name}'"), format!("{height} x {width}"), "4 x 6".to_owned()];
            assert_refused(&result, kind, &said.each_ref().map(String::as_str), name);
        }
        // the right size is accepted
        assert!(
            Photo::new(Picture::new(4, 6, &values)?)
                .with_map(name, flat(0.5)?)
                .is_ok()
        );
    }
    Ok(())
}

#[test]
fn a_finder_answering_a_map_of_another_size_than_the_picture_is_refused_and_nothing_runs() -> Outcome {
    let rig = rig()?;
    let values = values();
    for (name, kind) in NAMES {
        for (height, width) in [(2, 2), (6, 4), (4, 3)] {
            let finder = Finds::new(move |_| Layer::new(height, width, vec![0.5; height * width]));
            let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder(name, Arc::new(finder));
            let result = render(&photo, &recipe([rig.uses(name)], &["star"]));
            assert_refused(
                &result,
                kind,
                &[&format!("'{name}'"), &format!("{height} x {width}")],
                name,
            );
        }
    }
    assert_eq!(rig.recorder.received().len(), 0);
    Ok(())
}

#[test]
fn what_a_finder_fails_with_is_the_renders_error_and_a_failure_is_not_remembered() -> Outcome {
    let rig = rig()?;
    let values = values();
    let finder = Arc::new(Finds::new(|_| {
        Err(Error::map_not_found("subject", "nothing stands out"))
    }));
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("subject", finder.clone());
    let look = recipe([rig.uses("subject")], &[]);
    for _ in 0..2 {
        assert_refused(
            &render(&photo, &look),
            ErrorKind::SubjectNotFound,
            &["nothing stands out"],
            "subject",
        );
    }
    assert_eq!(finder.calls(), 2);
    // a map with values outside 0..1 cannot be made at all, so a finder that builds one fails as `Layer::new` does
    let finder = Finds::new(|picture| Layer::new(picture.height(), picture.width(), vec![f32::NAN; 24]));
    let photo = Photo::new(Picture::new(4, 6, &values)?).with_finder("depth", Arc::new(finder));
    assert_refused(
        &render(&photo, &recipe([rig.uses("depth")], &[])),
        ErrorKind::Other,
        &["NaN"],
        "NaN",
    );
    Ok(())
}

#[test]
fn a_name_that_is_no_map_of_the_recipe_is_refused_listing_the_names_there_are() -> Outcome {
    let rig = rig()?;
    let values = values();
    let picture = Picture::new(4, 6, &values)?;
    // the recipe uses no map at all: the names given are checked all the same
    let look = recipe([rig.paint()], &["star"]);
    let lists = ["depth", "star", "subject"];
    let given = |name: &str| Photo::new(picture).with_map(name, flat(0.5)?);
    let found = |name: &str| -> Result<Photo<'_>, Error> {
        Ok(Photo::new(picture).with_finder(name, Arc::new(Finds::flat(0.5))))
    };
    for name in ["nope", "light", "background", "cut"] {
        assert_refused(
            &render(&given(name)?, &look),
            ErrorKind::Recipe,
            &[&format!("a map '{name}'"), "subject, depth, star"],
            name,
        );
        assert_refused(
            &render(&found(name)?, &look),
            ErrorKind::Recipe,
            &[&format!("a finder '{name}'"), "subject, depth, star"],
            name,
        );
    }
    // the built-in maps and the recipe's own given region are maps of it, used or not
    for name in lists {
        render(&given(name)?, &look)?;
        render(&found(name)?, &look)?;
    }
    Ok(())
}

#[test]
fn a_region_that_cannot_be_made_is_refused_by_name_when_a_step_uses_it_and_costs_nothing_unused() -> Outcome {
    let rig = rig()?;
    let values = values();
    let mut look = Recipe::new([rig.paint()]);
    look.steps.push(Each::new("cut", [None]).into());
    // an svg region with no file
    look.regions.insert("cut", Svg::default());
    let photo = Photo::new(Picture::new(4, 6, &values)?);
    assert_refused(
        &render(&photo, &look),
        ErrorKind::Recipe,
        &["'cut'", "`file` is not set"],
        "cuts",
    );
    // declared and not used, it costs nothing and the recipe renders; nothing makes a map of it, so one handed in is
    // no map of the recipe
    let mut unused = Recipe::new([rig.paint()]);
    unused.regions.insert("cut", Svg::default());
    render(&photo, &unused)?;
    assert_refused(
        &render(
            &Photo::new(Picture::new(4, 6, &values)?).with_map("cut", flat(0.5)?)?,
            &unused,
        ),
        ErrorKind::Recipe,
        &["'cut'", "no map of the recipe"],
        "handed in",
    );
    Ok(())
}
