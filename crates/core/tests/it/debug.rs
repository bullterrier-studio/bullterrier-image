//! Printing a picture, a map or the stages of a trace (`{:?}`, `dbg!`, an assertion that fails, an `unwrap` of an
//! error that holds none) shows its shape, never its pixels: a 1080p picture would print tens of megabytes.

use bullterrier_image::{Error, Image, Labels, Layer, Options, Photo, Picture, Presets, Recipe, Step};

const HEIGHT: usize = 300;
const WIDTH: usize = 400;

#[test]
fn printing_a_picture_or_a_map_shows_its_shape_not_its_pixels() -> Result<(), Error> {
    let rgb = vec![0.25_f32; HEIGHT * WIDTH * 3];
    let shown = [
        ("Picture", format!("{:?}", Picture::new(HEIGHT, WIDTH, &rgb)?)),
        ("Image", format!("{:?}", Image::new(HEIGHT, WIDTH, &rgb)?)),
        (
            "Layer",
            format!("{:?}", Layer::new(HEIGHT, WIDTH, vec![0.5; HEIGHT * WIDTH])?),
        ),
        (
            "Labels",
            format!("{:?}", Labels::new(HEIGHT, WIDTH, vec![0; HEIGHT * WIDTH], 1)?),
        ),
    ];
    for (name, text) in shown {
        assert!(text.len() < 200, "{name} printed {} bytes", text.len());
        assert!(text.contains(name), "{name}: {text}");
        assert!(
            text.contains("300") && text.contains("400"),
            "{name} shows no size: {text}"
        );
    }
    Ok(())
}

#[test]
fn printing_the_stages_of_a_trace_shows_no_pixels() -> Result<(), Error> {
    let rgb = vec![0.25_f32; HEIGHT * WIDTH * 3];
    let photo = Photo::new(Picture::new(HEIGHT, WIDTH, &rgb)?);
    let recipe = Recipe::new((1..=3).map(|step| Step::new("grade").param("exposure", f64::from(step) / 10.0)));
    let stages = photo.trace(&recipe, &Presets::new(), Options::default())?;
    let text = format!("{stages:?}");
    assert!(
        text.len() < 1000,
        "{} stages printed {} bytes",
        stages.len(),
        text.len()
    );
    Ok(())
}
