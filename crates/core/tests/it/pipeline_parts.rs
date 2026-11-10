//! The parts of the pipeline that a run is made of, through the public API: the automatic checks, the export of a still,
//! the record of a run (`manifest.json`) and the report page.

use std::path::{Path, PathBuf};

use bullterrier_image::pipeline::{
    Format, Item, Settings, Status, export_still, matte_warnings, read_manifest, report_html, still_warnings,
    write_record,
};
use bullterrier_image::{Error, Image, Layer};

use crate::common::Outcome;

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("bt-image-parts-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&path));
    path
}

// --- checks ---------------------------------------------------------------------------------------------------

fn matte(share_of_100: usize) -> Result<Layer, Error> {
    // a 10 x 10 matte whose first `share_of_100` pixels are the subject
    Layer::new(
        10,
        10,
        (0..100)
            .map(|index| f32::from(u8::from(index < share_of_100)))
            .collect(),
    )
}

#[test]
fn a_plausible_cutout_raises_no_warning_and_an_implausible_one_says_its_share() -> Outcome {
    let range = (0.02, 0.97);
    assert_eq!(matte_warnings(&matte(30)?, range), Vec::<String>::new());
    let missed = matte_warnings(&matte(1)?, range);
    assert!(
        missed.len() == 1 && missed[0].contains("only 1.0%") && missed[0].contains("missed"),
        "{missed:?}"
    );
    let all = matte_warnings(&matte(100)?, range);
    assert!(
        all.len() == 1 && all[0].contains("100.0%") && all[0].contains("almost no background"),
        "{all:?}"
    );
    // the limits are the caller's: 30 % is too little for (0.5, 0.9) and too much for (0.05, 0.2)
    assert!(matte_warnings(&matte(30)?, (0.5, 0.9))[0].contains("only"));
    assert!(matte_warnings(&matte(30)?, (0.05, 0.2))[0].contains("almost no background"));
    // a value of exactly 0.5 is the subject's
    assert_eq!(
        matte_warnings(&Layer::new(1, 2, vec![0.5, 0.0])?, (0.4, 0.9)),
        Vec::<String>::new()
    );
    Ok(())
}

fn flat(rgb: [f32; 3]) -> Result<Image, Error> {
    Image::new(8, 8, &rgb.repeat(64))
}

#[test]
fn a_blank_white_or_flat_result_is_flagged_and_a_varied_one_is_not() -> Outcome {
    let varied: Vec<f32> = (0..8 * 8 * 3)
        .map(|index| f32::from(u8::try_from(index % 64).unwrap_or(0)) / 63.0)
        .collect();
    assert_eq!(still_warnings(&Image::new(8, 8, &varied)?), Vec::<String>::new());
    assert!(still_warnings(&flat([0.0; 3])?)[0].contains("blank"));
    assert!(still_warnings(&flat([1.0; 3])?)[0].contains("white"));
    assert!(still_warnings(&flat([0.5; 3])?)[0].contains("flat"));
    Ok(())
}

#[test]
fn only_the_colour_of_a_picture_with_transparency_is_judged() -> Outcome {
    let varied: Vec<f32> = (0..8 * 8)
        .flat_map(|index| {
            let v = f32::from(u8::try_from(index % 64).unwrap_or(0)) / 63.0;
            [v, 1.0 - v, 0.5 * v, 0.0] // good colour, nothing opaque
        })
        .collect();
    assert_eq!(still_warnings(&Image::rgba(8, 8, &varied)?), Vec::<String>::new());
    let black = Image::rgba(8, 8, &[0.0, 0.0, 0.0, 1.0].repeat(64))?; // bad colour, fully opaque
    assert!(still_warnings(&black)[0].contains("blank"));
    Ok(())
}

#[test]
fn the_limits_of_the_share_and_of_blank_are_inside_the_plausible() -> Outcome {
    // exactly 2 % and exactly 97 % of the photo are fine; just outside is not
    assert_eq!(matte_warnings(&matte(2)?, (0.02, 0.97)), Vec::<String>::new());
    assert_eq!(matte_warnings(&matte(97)?, (0.02, 0.97)), Vec::<String>::new());
    assert_eq!(matte_warnings(&matte(1)?, (0.02, 0.97)).len(), 1);
    assert_eq!(matte_warnings(&matte(98)?, (0.02, 0.97)).len(), 1);
    // a picture whose brightest value is just over 0.02 is not blank (it is flat: nothing varies)
    assert!(still_warnings(&flat([0.0201; 3])?)[0].contains("flat"));
    assert!(still_warnings(&flat([0.0199; 3])?)[0].contains("blank"));
    Ok(())
}

// --- export ---------------------------------------------------------------------------------------------------

fn photo(height: usize, width: usize) -> Result<Image, Error> {
    let values: Vec<f32> = (0..height * width * 3)
        .map(|index| f32::from(u8::try_from(index % 200).unwrap_or(0)) / 200.0)
        .collect();
    Image::new(height, width, &values)
}

fn names(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .filter_map(|path| path.file_name()?.to_str().map(str::to_owned))
        .collect()
}

#[test]
fn a_still_is_written_in_every_format_and_size_largest_first_and_sizes_at_or_above_the_picture_mean_as_rendered()
-> Outcome {
    let out = scratch("export");
    let folder = out.join("a");
    let settings = Settings {
        formats: vec![Format::Jpg, Format::Png],
        sizes: vec![2048, 64, 32],
        ..Settings::default()
    };
    let written = export_still(&photo(48, 96)?, "warm", &folder, &out, &settings)?;
    // 96 px wide: 2048 is as rendered; 64 and 32 are copies; each in jpg then png
    assert_eq!(
        names(&written),
        [
            "warm.jpg",
            "warm.png",
            "warm_64.jpg",
            "warm_64.png",
            "warm_32.jpg",
            "warm_32.png"
        ]
    );
    let size = |name: &str| -> Result<(usize, usize), Box<dyn std::error::Error>> {
        let image = bullterrier_image::read(&folder.join(name))?;
        Ok((image.height(), image.width()))
    };
    assert_eq!(size("warm.png")?, (48, 96));
    assert_eq!(size("warm_64.png")?, (32, 64));
    assert_eq!(size("warm_32.jpg")?, (16, 32));
    Ok(())
}

#[test]
fn a_size_equal_to_the_longest_side_is_the_picture_as_rendered() -> Outcome {
    let out = scratch("equal");
    let settings = Settings {
        sizes: vec![96, 95],
        ..Settings::default()
    };
    let written = export_still(&photo(48, 96)?, "r", &out.join("a"), &out, &settings)?;
    assert_eq!(names(&written), ["r.jpg", "r_95.jpg"]);
    Ok(())
}

#[test]
fn without_sizes_a_still_is_written_as_rendered_and_a_picture_with_transparency_needs_png_or_webp() -> Outcome {
    let out = scratch("plain");
    let written = export_still(&photo(20, 30)?, "r", &out.join("a"), &out, &Settings::default())?;
    assert_eq!(names(&written), ["r.jpg"]);
    let clear = Image::rgba(4, 4, &[1.0, 0.0, 0.0, 0.5].repeat(16))?;
    let refused = export_still(&clear, "c", &out.join("a"), &out, &Settings::default());
    assert!(matches!(refused, Err(Error::BackendUnavailable(_))), "{refused:?}");
    Ok(())
}

#[test]
fn nothing_is_written_outside_the_output_folder() -> Outcome {
    let out = scratch("inside");
    let refused = export_still(
        &photo(8, 8)?,
        "r",
        &out.join("..").join("elsewhere"),
        &out,
        &Settings::default(),
    );
    assert!(
        matches!(&refused, Err(Error::Project(message)) if message.contains("outside the output folder")),
        "{refused:?}"
    );
    assert!(!out.join("..").join("elsewhere").exists());
    Ok(())
}

// --- the record and the report ----------------------------------------------------------------------------------

fn item(photo: &str, recipe: &str, status: Status) -> Item {
    Item {
        photo: photo.to_owned(),
        recipe: recipe.to_owned(),
        status,
        ..Item::default()
    }
}

#[test]
fn a_record_keeps_what_this_run_did_and_what_an_earlier_run_did_for_pairs_it_did_not_reach() -> Outcome {
    let out = scratch("record");
    let earlier = vec![
        item("a.jpg", "warm", Status::Done),
        item("b.jpg", "warm", Status::Done),
        item("gone.jpg", "warm", Status::Done),
        item("b.jpg", "retired", Status::Done),
    ];
    write_record(
        &out,
        &earlier,
        &[],
        &["a.jpg", "b.jpg", "gone.jpg"],
        &["warm", "cool", "retired"],
        "code1",
        "2026-11-10T12:00:00",
    )?;
    let previous = read_manifest(&out);
    assert_eq!(previous.len(), 4);
    // this run did only a.jpg x cool (an --only): b.jpg stays, a.jpg x warm stays, gone.jpg is no photo of the project any more
    let now = vec![item("a.jpg", "cool", Status::Done)];
    let record = write_record(
        &out,
        &now,
        &previous,
        &["a.jpg", "b.jpg"],
        &["warm", "cool"],
        "code2",
        "2026-11-10T12:05:00",
    )?;
    let pairs: Vec<(&str, &str)> = record.iter().map(|i| (i.photo.as_str(), i.recipe.as_str())).collect();
    assert_eq!(
        pairs,
        [("a.jpg", "warm"), ("a.jpg", "cool"), ("b.jpg", "warm")],
        "photos, then recipes in their order"
    );
    let text = std::fs::read_to_string(out.join("manifest.json"))?;
    assert!(
        text.contains("\"generated\": \"2026-11-10T12:05:00\"") && text.contains("\"code\": \"code2\""),
        "{text}"
    );
    assert_eq!(read_manifest(&out), record);
    Ok(())
}

#[test]
fn a_record_that_is_missing_or_damaged_is_no_record() -> Outcome {
    let out = scratch("damaged");
    assert_eq!(read_manifest(&out), Vec::<Item>::new());
    std::fs::create_dir_all(&out)?;
    for text in [
        "",
        "not json",
        "{\"items\": 3}",
        "{\"items\": [{\"photo\": \"a\"}]}",
        "{\"items\": [{\"photo\": \"a\", \"recipe\": \"r\", \"status\": \"weird\"}]}",
    ] {
        std::fs::write(out.join("manifest.json"), text)?;
        assert_eq!(read_manifest(&out), Vec::<Item>::new(), "{text}");
    }
    Ok(())
}

#[test]
fn the_report_counts_groups_by_photo_escapes_names_and_shows_warnings_errors_and_steps() {
    let out = Path::new("/tmp/bt-report/out");
    let mut done = item("<b>.jpg", "warm", Status::Done);
    done.outputs = vec!["<b>/warm.jpg".to_owned()];
    done.warnings = vec!["the result is blank & dark".to_owned()];
    done.steps = Some("<b>/steps/warm/_sheet.jpg".to_owned());
    let mut failed = item("<b>.jpg", "cool", Status::Failed);
    failed.error = Some("SubjectNotFoundError: none".to_owned());
    let items = [done, failed, item("c.jpg", "warm", Status::Skipped)];
    let page = report_html(
        &[
            PathBuf::from("/tmp/bt-report/photos/<b>.jpg"),
            PathBuf::from("/tmp/bt-report/photos/c.jpg"),
        ],
        out,
        &items,
    );
    assert!(
        page.contains("1 done, 1 up to date, 1 failed, 1 with warnings"),
        "{page}"
    );
    assert!(
        page.contains("&lt;b&gt;.jpg") && !page.contains("<b>.jpg"),
        "names are escaped"
    );
    assert!(page.contains("<p class=\"warn\">the result is blank &amp; dark</p>"));
    assert!(page.contains("<p class=\"error\">SubjectNotFoundError: none</p>"));
    assert!(page.contains("<figure class=\"failed\">") && page.contains("<figure class=\"done\">"));
    assert!(page.contains("href=\"&lt;b&gt;/steps/warm/_sheet.jpg\">steps</a>"));
    assert!(
        page.contains("src=\"../photos/&lt;b&gt;.jpg\""),
        "the photo is linked relative to the output folder"
    );
    assert!(
        page.find("&lt;b&gt;.jpg").unwrap_or(0) < page.find("c.jpg</h2>").unwrap_or(0),
        "in the order of the items"
    );
}

#[test]
fn quotes_in_a_name_cannot_break_out_of_an_attribute_of_the_report() {
    let item = item("a\"b'c.jpg", "r", Status::Done);
    let page = report_html(&[PathBuf::from("/tmp/x/a\"b'c.jpg")], Path::new("/tmp/x/out"), &[item]);
    assert!(
        page.contains("a&quot;b&#x27;c.jpg") && !page.contains("a\"b'c.jpg"),
        "{page}"
    );
}
