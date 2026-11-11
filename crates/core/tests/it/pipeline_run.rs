//! Running a project through the public API: every photo with every recipe, remembered between runs. The finder of the subject
//! is the one external boundary, so a stand-in answers (and counts how often it is asked).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bullterrier_image::pipeline::{Found, Given, Item, Project, RecipeSource, RunOptions, Settings, Status, run};
use bullterrier_image::{Error, Finder, Image, Layer, Picture, Presets};

use crate::common::Outcome;

/// A subject finder that says the left half is the subject, and counts how often it is asked.
struct Halves {
    asked: Arc<AtomicUsize>,
    nothing: bool,
}

impl Finder for Halves {
    fn find(&self, picture: &Picture<'_>) -> Result<Layer, Error> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        if self.nothing {
            return Err(Error::map_not_found("subject", "nothing stands out"));
        }
        let (height, width) = (picture.height(), picture.width());
        Layer::new(
            height,
            width,
            (0..height * width)
                .map(|i| f32::from(u8::from(i % width < width / 2)))
                .collect(),
        )
    }
}

fn found(asked: &Arc<AtomicUsize>, identity: &str, nothing: bool) -> BTreeMap<String, Found> {
    let finder: Arc<dyn Finder> = Arc::new(Halves {
        asked: Arc::clone(asked),
        nothing,
    });
    BTreeMap::from([(
        "subject".to_owned(),
        Found {
            finder,
            identity: identity.to_owned(),
        },
    )])
}

struct Scene {
    root: PathBuf,
}

impl Scene {
    fn new(name: &str, photos: &[&str]) -> Result<Self, Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("bt-image-run-{name}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&root));
        std::fs::create_dir_all(root.join("photos"))?;
        for (index, photo) in photos.iter().enumerate() {
            let values: Vec<f32> = (0..24 * 32 * 3)
                .map(|i| f32::from(u8::try_from((i * (index + 3)) % 200).unwrap_or(0)) / 200.0)
                .collect();
            bullterrier_image::write(&root.join("photos").join(photo), &Image::new(24, 32, &values)?)?;
        }
        Ok(Self { root })
    }

    fn recipe(&self, name: &str, steps: &str) -> Result<PathBuf, std::io::Error> {
        std::fs::create_dir_all(self.root.join("recipes"))?;
        let path = self.root.join("recipes").join(format!("{name}.toml"));
        std::fs::write(&path, format!("name = \"{name}\"\nsize = 32\n\n[[step]]\n{steps}\n"))?;
        Ok(path)
    }

    fn project(&self, photos: &[&str], recipes: &[PathBuf], settings: Settings) -> Result<Project, Error> {
        Project::new(
            photos
                .iter()
                .map(|photo| self.root.join("photos").join(photo))
                .collect(),
            recipes.iter().cloned().map(RecipeSource::File).collect(),
            self.root.join("out"),
            settings,
        )
    }
}

const GRADE: &str = "effect = \"grade\"\nexposure = 0.3";
const CUT: &str = "effect = \"gradient\"\ncolors = [\"#000000\"]\nmask = \"background\"";

fn go(project: &Project, options: &RunOptions<'_>, given: &Given) -> Result<Vec<Item>, Error> {
    run(project, &Presets::new(), options, given, &mut |_| {})
}

fn statuses(items: &[Item]) -> Vec<(&str, &str, &str)> {
    items
        .iter()
        .map(|item| (item.photo.as_str(), item.recipe.as_str(), item.status.name()))
        .collect()
}

const NOW: &str = "2026-11-11T09:00:00";
fn plain() -> RunOptions<'static> {
    RunOptions {
        generated: NOW,
        ..RunOptions::default()
    }
}

#[test]
fn a_run_writes_every_pair_and_the_record_and_the_report_and_a_second_run_skips_what_is_up_to_date() -> Outcome {
    let scene = Scene::new("basic", &["a.png", "b.png"])?;
    let recipes = [
        scene.recipe("warm", GRADE)?,
        scene.recipe("cool", "effect = \"grade\"\nexposure = -0.3")?,
    ];
    let project = scene.project(&["a.png", "b.png"], &recipes, Settings::default())?;
    let first = go(&project, &plain(), &Given::default())?;
    assert_eq!(
        statuses(&first),
        [
            ("a.png", "warm", "done"),
            ("a.png", "cool", "done"),
            ("b.png", "warm", "done"),
            ("b.png", "cool", "done")
        ]
    );
    for photo in ["a", "b"] {
        for recipe in ["warm", "cool"] {
            assert!(
                scene
                    .root
                    .join("out")
                    .join(photo)
                    .join(format!("{recipe}.jpg"))
                    .is_file(),
                "{photo}/{recipe}"
            );
        }
    }
    assert!(scene.root.join("out/manifest.json").is_file() && scene.root.join("out/index.html").is_file());
    assert_eq!(first[0].outputs, ["a/warm.jpg"]);
    let second = go(&project, &plain(), &Given::default())?;
    assert!(second.iter().all(|item| item.status == Status::Skipped));
    let forced = go(&project, &RunOptions { force: true, ..plain() }, &Given::default())?;
    assert!(forced.iter().all(|item| item.status == Status::Done));
    Ok(())
}

#[test]
fn a_changed_recipe_a_removed_output_or_new_settings_redo_only_what_they_shape() -> Outcome {
    let scene = Scene::new("redo", &["a.png", "b.png"])?;
    let recipes = [
        scene.recipe("warm", GRADE)?,
        scene.recipe("cool", "effect = \"grade\"\nexposure = -0.3")?,
    ];
    let project = scene.project(&["a.png", "b.png"], &recipes, Settings::default())?;
    go(&project, &plain(), &Given::default())?;
    scene.recipe("warm", "effect = \"grade\"\nexposure = 0.4")?;
    let after_recipe = go(&project, &plain(), &Given::default())?;
    assert_eq!(
        statuses(&after_recipe).iter().filter(|row| row.2 == "done").count(),
        2,
        "warm for both photos: {after_recipe:?}"
    );
    assert!(
        after_recipe
            .iter()
            .filter(|item| item.recipe == "cool")
            .all(|item| item.status == Status::Skipped)
    );
    std::fs::remove_file(scene.root.join("out/b/cool.jpg"))?;
    let after_file = go(&project, &plain(), &Given::default())?;
    assert_eq!(
        statuses(&after_file)
            .iter()
            .filter(|row| row.2 == "done")
            .collect::<Vec<_>>(),
        [&("b.png", "cool", "done")]
    );
    std::fs::write(
        scene.root.join("photos/a.png"),
        std::fs::read(scene.root.join("photos/b.png"))?,
    )?;
    let after_photo = go(&project, &plain(), &Given::default())?;
    assert!(
        after_photo
            .iter()
            .filter(|item| item.photo == "a.png")
            .all(|item| item.status == Status::Done)
    );
    let changed = Settings {
        quality: 70,
        ..Settings::default()
    };
    let other = scene.project(&["a.png", "b.png"], &recipes, changed)?;
    assert!(
        go(&other, &plain(), &Given::default())?
            .iter()
            .all(|item| item.status == Status::Done),
        "quality is a setting of the key"
    );
    Ok(())
}

#[test]
fn a_dry_run_says_what_would_run_and_writes_nothing_and_only_picks_photos() -> Outcome {
    let scene = Scene::new("dry", &["a.png", "b.png"])?;
    let recipes = [scene.recipe("warm", GRADE)?];
    let project = scene.project(&["a.png", "b.png"], &recipes, Settings::default())?;
    let dry = go(
        &project,
        &RunOptions {
            dry_run: true,
            ..plain()
        },
        &Given::default(),
    )?;
    assert!(dry.iter().all(|item| item.status == Status::WouldRun));
    assert!(!scene.root.join("out").exists());
    let only = go(
        &project,
        &RunOptions {
            only: Some("b.*"),
            ..plain()
        },
        &Given::default(),
    )?;
    assert_eq!(statuses(&only), [("b.png", "warm", "done")]);
    let none = go(
        &project,
        &RunOptions {
            only: Some("z*"),
            ..plain()
        },
        &Given::default(),
    );
    assert!(
        matches!(&none, Err(Error::Project(message)) if message.contains("--only") && message.contains("z*")),
        "{none:?}"
    );
    Ok(())
}

#[test]
fn a_pair_that_fails_is_its_own_item_and_the_others_run() -> Outcome {
    let scene = Scene::new("fails", &["a.png"])?;
    let recipes = [scene.recipe("cut", CUT)?, scene.recipe("warm", GRADE)?];
    let project = scene.project(&["a.png"], &recipes, Settings::default())?;
    let asked = Arc::new(AtomicUsize::new(0));
    let given = Given {
        finders: found(&asked, "stand-in", true),
        ..Given::default()
    };
    let items = go(&project, &plain(), &given)?;
    assert_eq!(
        statuses(&items),
        [("a.png", "cut", "failed"), ("a.png", "warm", "done")]
    );
    let error = items[0].error.clone().unwrap_or_default();
    assert!(
        error.starts_with("SubjectNotFoundError: ") && error.contains("nothing stands out"),
        "{error}"
    );
    Ok(())
}

#[test]
fn a_map_is_asked_once_per_photo_and_remembered_by_its_finders_identity() -> Outcome {
    let scene = Scene::new("maps", &["a.png", "b.png"])?;
    let recipes = [
        scene.recipe("cut1", CUT)?,
        scene.recipe("cut2", &format!("{CUT}\nopacity = 0.5"))?,
    ];
    let project = scene.project(&["a.png", "b.png"], &recipes, Settings::default())?;
    let asked = Arc::new(AtomicUsize::new(0));
    let given = Given {
        finders: found(&asked, "one", false),
        ..Given::default()
    };
    go(&project, &plain(), &given)?;
    assert_eq!(
        asked.load(Ordering::SeqCst),
        2,
        "once for each photo, however many recipes use it (the same finder serves every photo)"
    );
    go(&project, &RunOptions { force: true, ..plain() }, &given)?;
    assert_eq!(
        asked.load(Ordering::SeqCst),
        2,
        "remembered on disk: a forced run asks nothing"
    );
    let other = Given {
        finders: found(&asked, "two", false),
        ..Given::default()
    };
    let redone = go(&project, &plain(), &other)?;
    assert!(
        redone.iter().all(|item| item.status == Status::Done),
        "another identity redoes the recipes that use the map"
    );
    assert_eq!(asked.load(Ordering::SeqCst), 4);
    Ok(())
}

#[test]
fn a_cutout_is_flagged_when_it_covers_an_implausible_share_and_still_exported() -> Outcome {
    let scene = Scene::new("share", &["a.png"])?;
    let recipes = [scene.recipe("cut", CUT)?];
    let project = scene.project(
        &["a.png"],
        &recipes,
        Settings {
            subject_share: (0.6, 0.9),
            ..Settings::default()
        },
    )?;
    let asked = Arc::new(AtomicUsize::new(0));
    let items = go(
        &project,
        &plain(),
        &Given {
            finders: found(&asked, "x", false),
            ..Given::default()
        },
    )?;
    assert_eq!(items[0].status, Status::Done);
    assert!(
        items[0].warnings.iter().any(|warning| warning.contains("only 50.0%")),
        "{:?}",
        items[0].warnings
    );
    Ok(())
}

#[test]
fn the_steps_of_a_recipe_are_saved_with_a_sheet_when_asked() -> Outcome {
    let scene = Scene::new("steps", &["a.png"])?;
    let recipes = [scene.recipe("warm", GRADE)?];
    let project = scene.project(
        &["a.png"],
        &recipes,
        Settings {
            steps: true,
            ..Settings::default()
        },
    )?;
    let items = go(&project, &plain(), &Given::default())?;
    assert_eq!(items[0].steps.as_deref(), Some("a/steps/warm/_sheet.jpg"));
    assert!(items[0].outputs.contains(&"a/steps/warm/_sheet.jpg".to_owned()));
    let folder = scene.root.join("out/a/steps/warm");
    let mut names: Vec<String> = std::fs::read_dir(&folder)?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    names.sort();
    assert_eq!(names, ["00-input.jpg", "01-grade.jpg", "_sheet.jpg"]);
    Ok(())
}

#[test]
fn recipes_that_cannot_run_together_are_refused_before_anything_is_written() -> Outcome {
    let scene = Scene::new("refused", &["a.png"])?;
    let one = scene.recipe("my look", GRADE)?;
    let two = scene.recipe("my-look", GRADE)?;
    let same = go(
        &scene.project(&["a.png"], &[one, two], Settings::default())?,
        &plain(),
        &Given::default(),
    );
    assert!(
        matches!(&same, Err(Error::Project(message)) if message.contains("same")),
        "{same:?}"
    );
    let cut = scene.recipe("cut", "effect = \"grade\"")?;
    std::fs::write(
        &cut,
        "name = \"cut\"\n[frame]\ntransparent = \"circle\"\n[[step]]\neffect = \"grade\"\n",
    )?;
    let jpg = go(
        &scene.project(&["a.png"], &[cut], Settings::default())?,
        &plain(),
        &Given::default(),
    );
    assert!(
        matches!(&jpg, Err(Error::Project(message)) if message.contains("jpg") && message.contains("cut")),
        "{jpg:?}"
    );
    let nameless = Project::new(
        vec![scene.root.join("photos/a.png")],
        vec![RecipeSource::Built(Box::default())],
        scene.root.join("out"),
        Settings::default(),
    )?;
    let unnamed = go(&nameless, &plain(), &Given::default());
    assert!(
        matches!(&unnamed, Err(Error::Recipe(message)) if message.contains("name=")),
        "{unnamed:?}"
    );
    assert!(!scene.root.join("out").exists());
    Ok(())
}

#[test]
fn a_finder_for_no_map_and_a_recipe_without_a_finder_for_a_map_it_uses_are_refused() -> Outcome {
    let scene = Scene::new("finders", &["a.png"])?;
    let recipes = [scene.recipe("cut", CUT)?];
    let project = scene.project(&["a.png"], &recipes, Settings::default())?;
    let asked = Arc::new(AtomicUsize::new(0));
    let missing = go(&project, &plain(), &Given::default());
    assert!(
        matches!(&missing, Err(Error::Project(message)) if message.contains("'subject'") && message.contains("finder")),
        "{missing:?}"
    );
    let plain_recipes = [scene.recipe("warm", GRADE)?];
    let project = scene.project(&["a.png"], &plain_recipes, Settings::default())?;
    let strange = Given {
        finders: BTreeMap::from([(
            "strar".to_owned(),
            found(&asked, "x", false).remove("subject").ok_or("subject")?,
        )]),
        ..Given::default()
    };
    let unknown = go(&project, &plain(), &strange);
    assert!(
        matches!(&unknown, Err(Error::Project(message)) if message.contains("'strar'") && message.contains("subject")),
        "{unknown:?}"
    );
    Ok(())
}

#[test]
fn a_recipe_name_gives_its_files_a_safe_name_and_a_partition_is_shown_with_its_pieces_numbered() -> Outcome {
    let scene = Scene::new("safe", &["a.png"])?;
    let path = scene.recipe("x", GRADE)?;
    std::fs::write(
        &path,
        "format = 2\nname = \"my warm look!\"\nsize = 32\n\n[region.panes]\nkind = \"grid\"\ncolumns = 2\nrows = 2\n\n[[step]]\neach = \"panes\"\nlooks = [{ effect = \"grade\", exposure = 0.4 }, \"none\"]\n",
    )?;
    let project = scene.project(
        &["a.png"],
        &[path],
        Settings {
            steps: true,
            ..Settings::default()
        },
    )?;
    let items = go(&project, &plain(), &Given::default())?;
    assert_eq!(items[0].status, Status::Done, "{items:?}");
    assert_eq!(items[0].recipe, "my-warm-look");
    assert_eq!(items[0].outputs[0], "a/my-warm-look.jpg");
    let folder = scene.root.join("out/a/steps/my-warm-look");
    let mut names: Vec<String> = std::fs::read_dir(&folder)?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "00-input.jpg",
            "01-region-panes-4-pieces-numbered.jpg",
            "02-each.jpg",
            "_sheet.jpg"
        ]
    );
    Ok(())
}
