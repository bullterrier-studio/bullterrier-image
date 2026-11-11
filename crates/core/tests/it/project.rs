//! A project (`bt-image.toml`) through the public API: what a file says, what the defaults are, and what is refused naming
//! the file or the setting and its value.

use std::path::{Path, PathBuf};

use bullterrier_image::Error;
use bullterrier_image::pipeline::{Format, Project, RecipeSource, Settings};

use crate::common::Outcome;

/// A folder with `files` (relative paths, empty content) and the project file `text`; returns the project file's path.
fn folder(name: &str, files: &[&str], text: &str) -> Result<PathBuf, std::io::Error> {
    let root = std::env::temp_dir().join(format!("bt-image-project-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&root));
    for file in files {
        let path = root.join(file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, b"")?;
    }
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join("bt-image.toml"), text)?;
    Ok(root.join("bt-image.toml"))
}

fn names(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .filter_map(|path| path.file_name()?.to_str().map(str::to_owned))
        .collect()
}

const BASIC: &[&str] = &["photos/b.jpg", "photos/a.png", "recipes/warm.toml", "recipes/cool.toml"];

#[test]
fn a_project_with_no_settings_has_the_defaults_and_finds_photos_and_recipes_where_they_are_expected() -> Outcome {
    let file = folder("defaults", BASIC, "")?;
    let project = Project::read(&file)?;
    assert_eq!(names(&project.photos), ["a.png", "b.jpg"], "sorted");
    let recipes: Vec<PathBuf> = project
        .recipes
        .iter()
        .filter_map(|source| {
            if let RecipeSource::File(path) = source {
                Some(path.clone())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        names(&recipes),
        ["cool.toml", "warm.toml"],
        "the folder's toml files, sorted"
    );
    assert_eq!(project.output, file.parent().ok_or("parent")?.join("out"));
    assert_eq!(project.settings, Settings::default());
    assert_eq!(project.settings.formats, [Format::Jpg]);
    assert!(project.settings.sizes.is_empty() && project.settings.quality == 92 && !project.settings.steps);
    assert_eq!(project.settings.subject_share, (0.02, 0.97));
    Ok(())
}

#[test]
fn the_settings_of_a_file_are_read_and_the_sizes_come_largest_first() -> Outcome {
    let text = "photos = [\"photos/*.png\"]\nrecipes = [\"recipes/warm.toml\"]\noutput = \"made\"\n\n[export]\nformats = [\"png\", \"webp\"]\nsizes = [1080, 2048]\nquality = 80\nsteps = true\n\n[checks]\nsubject_share = [0.1, 0.5]\n";
    let project = Project::read(&folder("settings", BASIC, text)?)?;
    assert_eq!(names(&project.photos), ["a.png"]);
    assert!(project.output.ends_with("made"));
    assert_eq!(project.settings.formats, [Format::Png, Format::Webp]);
    assert_eq!(project.settings.sizes, [2048, 1080]);
    assert_eq!((project.settings.quality, project.settings.steps), (80, true));
    assert_eq!(project.settings.subject_share, (0.1, 0.5));
    Ok(())
}

#[test]
fn globs_look_in_folders_below_and_skip_dotfiles_and_folders() -> Outcome {
    let files = [
        "photos/a.jpg",
        "photos/trip/b.jpg",
        "photos/._a.jpg",
        "photos/c.jpg/inner.txt",
        "recipes/r.toml",
    ];
    let project = Project::read(&folder("recursive", &files, "photos = [\"photos/**/*.jpg\"]\n")?)?;
    assert_eq!(
        names(&project.photos),
        ["a.jpg", "b.jpg"],
        "no ._a.jpg (a copy macOS makes), no folder called c.jpg"
    );
    Ok(())
}

fn refusal(files: &[&str], text: &str, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    match Project::read(&folder(name, files, text)?) {
        Err(Error::Project(message)) => Ok(message),
        other => Err(format!("wanted a project error, got {other:?}").into()),
    }
}

#[test]
fn a_file_that_breaks_a_rule_is_refused_naming_the_file_the_setting_and_the_value() -> Outcome {
    let rows: [(&str, &[&str], &str, &[&str]); 16] = [
        (
            "not toml",
            BASIC,
            "output = \"out\nsize = 3\n",
            &["bt-image.toml", "line"],
        ),
        ("an unknown key", BASIC, "colour = 1\n", &["bt-image.toml", "colour"]),
        (
            "an unknown key of a table",
            BASIC,
            "[export]\nsise = [1]\n",
            &["bt-image.toml", "sise", "sizes"],
        ),
        (
            "a table that is a value",
            BASIC,
            "export = 3\n",
            &["bt-image.toml", "export"],
        ),
        (
            "no photo matches",
            &["recipes/r.toml"],
            "",
            &["bt-image.toml", "no photos match", "photos/*.jpg"],
        ),
        (
            "no recipe",
            &["photos/a.jpg"],
            "",
            &["bt-image.toml", "no recipe", "recipes"],
        ),
        (
            "a quality of 0",
            BASIC,
            "[export]\nquality = 0\n",
            &["quality", "0", "1"],
        ),
        (
            "a quality of 101",
            BASIC,
            "[export]\nquality = 101\n",
            &["quality", "101"],
        ),
        (
            "a quality that is a decimal",
            BASIC,
            "[export]\nquality = 92.5\n",
            &["quality", "92.5"],
        ),
        (
            "a size of 15",
            BASIC,
            "[export]\nsizes = [15]\n",
            &["sizes", "15", "16"],
        ),
        (
            "no format",
            BASIC,
            "[export]\nformats = []\n",
            &["formats", "jpg, png, webp"],
        ),
        (
            "a format that is no format",
            BASIC,
            "[export]\nformats = [\"gif\"]\n",
            &["gif"],
        ),
        (
            "a share that is not a range",
            BASIC,
            "[checks]\nsubject_share = [0.5, 0.5]\n",
            &["subject_share", "low < high"],
        ),
        (
            "a share outside 0..1",
            BASIC,
            "[checks]\nsubject_share = [0.5, 1.5]\n",
            &["subject_share", "1.5"],
        ),
        ("steps that are text", BASIC, "[export]\nsteps = \"yes\"\n", &["steps"]),
        (
            "two photos of one name",
            &["photos/a.jpg", "photos/a.png", "recipes/r.toml"],
            "",
            &["share a file name", "a"],
        ),
    ];
    for (rule, files, text, words) in rows {
        let message = refusal(files, text, &rule.replace(' ', "-"))?;
        for word in words {
            assert!(message.contains(word), "{rule}: {word:?} not in {message:?}");
        }
    }
    Ok(())
}

#[test]
fn a_project_made_in_code_is_held_to_the_same_rules_and_may_hold_recipes_built_there() -> Outcome {
    let photo = Path::new("a.jpg").to_path_buf();
    let built = RecipeSource::Built(Box::default());
    let made = Project::new(
        vec![photo.clone()],
        vec![built.clone()],
        PathBuf::from("out"),
        Settings::default(),
    )?;
    assert_eq!(made.recipes, std::slice::from_ref(&built));
    let refused = Project::new(vec![], vec![built.clone()], PathBuf::from("out"), Settings::default());
    assert!(
        matches!(&refused, Err(Error::Project(message)) if message.contains("photos")),
        "{refused:?}"
    );
    let refused = Project::new(vec![photo], vec![], PathBuf::from("out"), Settings::default());
    assert!(
        matches!(&refused, Err(Error::Project(message)) if message.contains("recipes")),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_project_is_written_as_json_with_the_settings_beside_the_lists_and_read_back_the_same() -> Outcome {
    let text = r#"{"photos": ["a.jpg", "b.jpg"], "recipes": ["warm.toml", {}], "output": "out", "sizes": [1080, 2048], "quality": 80}"#;
    let project = Project::from_json(text)?;
    assert_eq!(project.settings.sizes, [2048, 1080]);
    assert_eq!(project.settings.quality, 80);
    assert_eq!(project.recipes.len(), 2);
    assert_eq!(Project::from_json(&project.to_json().to_string())?, project);
    Ok(())
}

#[test]
fn json_that_is_not_a_project_is_refused_naming_the_key_and_the_value() {
    let base = r#""photos": ["a.jpg"], "recipes": ["r.toml"], "output": "out""#;
    for (rule, extra, words) in [
        ("a decimal quality", r#", "quality": 92.5"#, &["quality", "92.5"][..]),
        ("a quality that is a flag", r#", "quality": true"#, &["quality", "true"]),
        ("a size that is text", r#", "sizes": ["big"]"#, &["sizes", "big"]),
        ("an unknown key", r#", "colour": 1"#, &["colour"]),
        ("a format that is no format", r#", "formats": ["gif"]"#, &["gif"]),
        ("steps that are a number", r#", "steps": 1"#, &["steps"]),
    ] {
        let refused = Project::from_json(&format!("{{{base}{extra}}}"));
        assert!(
            matches!(&refused, Err(Error::Project(message)) if words.iter().all(|word| message.contains(word))),
            "{rule}: {refused:?}"
        );
    }
}

#[test]
fn a_starter_project_is_a_project_that_reads_and_is_not_made_over_an_existing_one() -> Outcome {
    let root = std::env::temp_dir().join(format!("bt-image-init-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&root));
    let file = bullterrier_image::pipeline::init_project(&root.join("new"))?;
    assert_eq!(file, root.join("new").join("bt-image.toml"));
    assert!(root.join("new/photos").is_dir());
    std::fs::write(root.join("new/photos/one.jpg"), b"")?;
    let project = Project::read(&file)?;
    assert_eq!(names(&project.photos), ["one.jpg"]);
    assert_eq!(project.recipes.len(), 2, "{:?}", project.recipes);
    let again = bullterrier_image::pipeline::init_project(&root.join("new"));
    assert!(
        matches!(&again, Err(error) if error.to_string().contains("bt-image.toml already exists")),
        "{again:?}"
    );
    Ok(())
}
