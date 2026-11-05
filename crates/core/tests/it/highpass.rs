//! `highpass`: the picture minus its Gaussian blur, around mid-grey. The blur of the comparisons is the `gaussian_blur`
//! effect, itself held to its definition in `gaussian_blur.rs`; the rest is worked out by hand from the Gaussian's
//! weights.

use bullterrier_image::effects::{gaussian_blur, highpass};
use bullterrier_image::params::Declared;
use bullterrier_image::{Edge, EdgeOrConstant, Error};

use crate::common::gaussian::weights;
use crate::common::pictures::{ramp, varied};
use crate::common::pixels::run_picture;
use serde_json::json;

const MID_GREY: f32 = 0.5;
const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];
/// The same four rules by name, as `gaussian_blur` takes them (written out, not mapped by the code under test).
const BLUR_EDGES: [EdgeOrConstant; 4] = [
    EdgeOrConstant::Reflect,
    EdgeOrConstant::Nearest,
    EdgeOrConstant::Mirror,
    EdgeOrConstant::Wrap,
];

/// A 16 x 64 vertical step edge in the middle: `low` on the left, `high` on the right.
fn step(low: f32, high: f32) -> Vec<f32> {
    (0..16 * 64 * 3)
        .map(|index| if index / 3 % 64 < 32 { low } else { high })
        .collect()
}

#[test]
fn a_constant_picture_becomes_mid_grey_whatever_lies_beyond_its_edge() -> Result<(), Error> {
    for value in [0.0, 0.2, 0.5, 0.9, 1.0] {
        for edge in EDGES {
            let params = highpass::Params {
                edge,
                ..Default::default()
            };
            let out = run_picture(&params, 20, 30, &vec![value; 20 * 30 * 3])?;
            assert_eq!(
                out.iter().position(|&out| (out - MID_GREY).abs() > 1e-6),
                None,
                "{value}, {edge:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn radius_zero_keeps_nothing_but_mid_grey() -> Result<(), Error> {
    let params = highpass::Params {
        radius: 0.0,
        ..Default::default()
    };
    let out = run_picture(&params, 9, 13, &varied(9, 13))?;
    assert_eq!(out.iter().position(|&out| (out - MID_GREY).abs() > 1e-6), None);
    Ok(())
}

#[test]
fn a_bump_leaves_its_difference_from_the_blur_around_mid_grey() -> Result<(), Error> {
    // the blur at a bump of height h is h w0^2 above the grey, one pixel beside it h w0 w1 (w the 1-D Gaussian weights),
    // so the picture minus its blur is h (1 - w0^2) and -h w0 w1, whatever the grey
    let height = 0.2_f32;
    for radius in [1.5_f32, 3.0] {
        let (weights, middle) = weights(f64::from(radius));
        let (w0, w1) = (weights[middle], weights[middle + 1]);
        let mut grey = vec![0.3_f32; 41 * 41 * 3];
        grey[(20 * 41 + 20) * 3..][..3]
            .iter_mut()
            .for_each(|value| *value += height);
        let params = highpass::Params {
            radius,
            ..Default::default()
        };
        let out = run_picture(&params, 41, 41, &grey)?;
        let at_bump = f64::from(MID_GREY) + f64::from(height) * (1.0 - w0 * w0);
        let beside = f64::from(MID_GREY) - f64::from(height) * w0 * w1;
        assert!(
            (f64::from(out[(20 * 41 + 20) * 3]) - at_bump).abs() < 2e-6,
            "sigma {radius}"
        );
        assert!(
            (f64::from(out[(20 * 41 + 21) * 3]) - beside).abs() < 2e-6,
            "sigma {radius}"
        );
    }
    Ok(())
}

#[test]
fn the_blur_added_back_gives_the_picture_for_every_edge_rule() -> Result<(), Error> {
    // detail well inside 0.5 +- 0.5, so nothing is clipped: highpass - mid-grey + blur is the picture
    let picture: Vec<f32> = varied(14, 19).iter().map(|value| 0.45 + value * 0.1).collect();
    for (edge, blur_edge) in EDGES.into_iter().zip(BLUR_EDGES) {
        let params = highpass::Params {
            radius: 2.0,
            edge,
            ..Default::default()
        };
        let blur = gaussian_blur::Params {
            radius: 2.0,
            edge: blur_edge,
            ..Default::default()
        };
        let blurred = run_picture(&blur, 14, 19, &picture)?;
        let detail = run_picture(&params, 14, 19, &picture)?;
        let at = detail
            .iter()
            .zip(&blurred)
            .zip(&picture)
            .position(|((detail, blurred), picture)| (detail - MID_GREY + blurred - picture).abs() > 2e-6);
        assert_eq!(at, None, "{edge:?}");
    }
    Ok(())
}

#[test]
fn an_edge_is_odd_about_mid_grey_and_far_from_it_the_picture_is_mid_grey() -> Result<(), Error> {
    let params = highpass::Params {
        radius: 2.0,
        ..Default::default()
    };
    for (low, high) in [(0.3, 0.7), (0.1, 0.4)] {
        let rising = run_picture(&params, 16, 64, &step(low, high))?;
        let falling = run_picture(&params, 16, 64, &step(high, low))?;
        let at = |column: usize| (8 * 64 + column) * 3;
        for column in 0..64 {
            let (up, down) = (rising[at(column)], falling[at(column)]);
            assert!(
                (up - MID_GREY - (MID_GREY - down)).abs() < 2e-6,
                "{low}, {high}, column {column}"
            );
        }
        assert!(rising[at(31)] < MID_GREY && MID_GREY < rising[at(32)]);
        // 4 sigma = 8 px from the edge is 24 px away from the ends
        assert!(
            (0..20)
                .chain(44..64)
                .all(|column| (rising[at(column)] - MID_GREY).abs() < 1e-6)
        );
    }
    Ok(())
}

#[test]
fn radius_sets_which_detail_is_kept() -> Result<(), Error> {
    // a fine checkerboard survives a large radius, an area-wide ramp does not
    let checker: Vec<f32> = (0..48 * 48 * 3)
        .map(|index| f32::from(u8::try_from((index / 3 / 48 + index / 3 % 48) % 2).unwrap_or(0)))
        .collect();
    let ramp = ramp(48, 48);
    let params = highpass::Params {
        radius: 8.0,
        ..Default::default()
    };
    let kept = |picture: &[f32]| -> Result<Vec<f32>, Error> {
        let out = run_picture(&params, 48, 48, picture)?;
        Ok((20..28)
            .flat_map(|row| (20..28).map(move |column| (row, column)))
            .map(|(row, column)| (out[(row * 48 + column) * 3] - MID_GREY).abs())
            .collect())
    };
    assert!(kept(&checker)?.into_iter().all(|detail| detail > 0.4));
    assert!(kept(&ramp)?.into_iter().all(|detail| detail < 0.02));
    Ok(())
}

#[test]
fn the_edge_rule_sets_what_the_blur_sees_beyond_the_picture() -> Result<(), Error> {
    // in a ramp from dark to bright, wrap puts the bright end beside the dark one, so the blur at the dark edge is brighter
    // than the picture there and the detail dips below mid-grey; reflecting does not
    let ramp = ramp(8, 64);
    let first = |edge| -> Result<f32, Error> {
        let params = highpass::Params {
            radius: 4.0,
            edge,
            ..Default::default()
        };
        Ok(run_picture(&params, 8, 64, &ramp)?[4 * 64 * 3])
    };
    assert!((first(Edge::Reflect)? - MID_GREY).abs() < 0.1);
    assert!(first(Edge::Wrap)? < MID_GREY - 0.1);
    Ok(())
}

#[test]
fn truncate_sets_where_the_blur_is_cut_off() -> Result<(), Error> {
    // six px from a bump: sigma 2 reaches there with truncate 4 (8 px), not with truncate 2 (4 px)
    let mut grey = vec![0.5_f32; 41 * 41 * 3];
    grey[(20 * 41 + 20) * 3..][..3].fill(1.0);
    for (truncate, reached) in [(4.0, true), (2.0, false)] {
        let params = highpass::Params {
            radius: 2.0,
            truncate,
            ..Default::default()
        };
        let out = run_picture(&params, 41, 41, &grey)?;
        assert_eq!(
            (out[(20 * 41 + 26) * 3] - MID_GREY).abs() > 1e-4,
            reached,
            "truncate {truncate}"
        );
    }
    Ok(())
}

#[test]
fn detail_beyond_mid_grey_is_cut_off_at_black_and_white() -> Result<(), Error> {
    // a sigma-1 blur leaves a lone dot about 16% of its height, so its detail is about 0.84 from mid-grey
    for (ground, dot) in [(0.0_f32, 1.0_f32), (1.0, 0.0)] {
        let mut dotted = vec![ground; 21 * 21 * 3];
        dotted[(10 * 21 + 10) * 3..][..3].fill(dot);
        let params = highpass::Params {
            radius: 1.0,
            ..Default::default()
        };
        assert_eq!(run_picture(&params, 21, 21, &dotted)?[(10 * 21 + 10) * 3], dot);
    }
    Ok(())
}

#[test]
fn the_edge_rule_constant_is_not_one_of_its_choices() {
    // (it has no constant_value to name what lies beyond the edge)
    let check = |edge: &str| {
        let given = json!({ "edge": edge });
        highpass::Params::table().check("highpass", given.as_object().unwrap_or(&serde_json::Map::new()))
    };
    assert!(check("wrap").is_ok());
    assert!(check("constant").is_err());
}
