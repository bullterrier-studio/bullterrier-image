//! The shipped `curves` presets on a grey ramp: what each promises about the tones, read off the output of the ramp's
//! darkest, middle and brightest greys.

use bullterrier_image::{Options, Photo, Picture, Presets, Recipe, Step};

use crate::common::Outcome;

/// The ramp's greys: 0, 1/4, 1/2, 3/4 and 1.
const RAMP: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

/// The five greys of the ramp after the `curves` preset `name`.
fn curved(name: &str) -> Result<[f32; 5], Box<dyn std::error::Error>> {
    let rgb: Vec<f32> = RAMP.iter().flat_map(|&grey| [grey; 3]).collect();
    let mut step = Step::new("curves");
    step.preset = Some(name.to_owned());
    let photo = Photo::new(Picture::new(1, RAMP.len(), &rgb)?);
    let out = photo.render(&Recipe::new([step]), &Presets::shipped()?, Options::default())?;
    let values = out.into_values();
    Ok([0, 1, 2, 3, 4].map(|at| values[at * 3]))
}

#[test]
fn every_curves_preset_but_solarize_keeps_the_tones_in_order_and_in_range() -> Outcome {
    for name in Presets::shipped()?.names("curves") {
        if name == "solarize" {
            continue; // folds the tones back on purpose
        }
        let out = curved(name)?;
        assert!(out.windows(2).all(|pair| pair[0] <= pair[1]), "{name}: {out:?} is not in order");
        assert!(out.iter().all(|value| (0.0..=1.0).contains(value)), "{name}: {out:?}");
    }
    Ok(())
}

#[test]
fn the_contrast_presets_push_shadows_down_and_highlights_up_by_their_degree() -> Outcome {
    let (low, mild, strong) = (curved("low_contrast")?, curved("contrast")?, curved("strong_contrast")?);
    for out in [low, mild, strong] {
        assert!((out[2] - 0.5).abs() < 1e-5, "the middle grey stays: {out:?}");
    }
    assert!(low[1] > 0.25 && low[3] < 0.75, "low contrast pulls the quarters in: {low:?}");
    assert!(mild[1] < 0.25 && mild[3] > 0.75, "contrast pushes them out: {mild:?}");
    assert!(strong[1] < mild[1] && strong[3] > mild[3], "strong goes further: {strong:?} {mild:?}");
    Ok(())
}

#[test]
fn fade_and_matte_lift_black_and_fade_dims_white() -> Outcome {
    let (fade, matte) = (curved("fade")?, curved("matte")?);
    assert!(fade[0] >= 0.05 && fade[4] <= 0.97, "fade: {fade:?}");
    assert!(matte[0] >= 0.08 && (matte[4] - 1.0).abs() < 1e-5, "matte lifts black only: {matte:?}");
    Ok(())
}

#[test]
fn crush_darkens_the_shadows_and_lift_shadows_opens_them() -> Outcome {
    assert!(curved("crush")?[1] <= 0.15, "crush: {:?}", curved("crush")?);
    assert!(curved("lift_shadows")?[1] >= 0.32, "lift_shadows: {:?}", curved("lift_shadows")?);
    Ok(())
}

#[test]
fn the_film_presets_roll_the_highlights_off_the_shoulder() -> Outcome {
    // the slope between the top two points is gentler than between the middle two: the curve flattens toward white
    for name in ["film", "shoulder"] {
        let out = curved(name)?;
        assert!(out[4] - out[3] < out[3] - out[2], "{name}: {out:?} does not flatten at the top");
    }
    // the film curve also has a toe: it darkens the quarter tone
    assert!(curved("film")?[1] < 0.25);
    Ok(())
}
