//! The clean-up of a subject's matte (`refine_edge`): the edge pulled in by a disc, then softened by a Gaussian. Expected
//! values are worked out by hand, as for the Python function this replaces.

use bullterrier_image::{Layer, refine_edge};

use crate::common::Outcome;

fn rows(height: usize, width: usize, set: impl Fn(usize, usize) -> f32) -> Vec<f32> {
    (0..height * width)
        .map(|index| set(index / width, index % width))
        .collect()
}

fn square(start: usize, stop: usize) -> Vec<f32> {
    rows(40, 40, |row, column| {
        f32::from(u8::from(
            (start..stop).contains(&row) && (start..stop).contains(&column),
        ))
    })
}

fn half_plane(edge: usize) -> Vec<f32> {
    rows(20, 60, |_, column| f32::from(u8::from(column >= edge)))
}

#[test]
fn shrink_pulls_the_edge_in_by_a_disc_of_that_radius() -> Outcome {
    // a disc of radius 3 reaches 3 px along each axis; x^2 + y^2 <= 6.25 holds for (2, 1) but not for (3, 0); 1.5 -> 1
    for (shrink, pulled_in) in [(3.0, 3), (2.5, 2), (1.5, 1)] {
        let matte = Layer::new(40, 40, square(10, 30))?;
        let refined = refine_edge(&matte, shrink, 0.0);
        assert_eq!(
            refined.values(),
            square(10 + pulled_in, 30 - pulled_in),
            "shrink {shrink}"
        );
    }
    Ok(())
}

#[test]
fn nothing_to_shrink_or_feather_leaves_the_matte_as_it_is() -> Outcome {
    let matte = Layer::new(40, 40, square(10, 30))?;
    assert_eq!(refine_edge(&matte, 0.0, 0.0), matte);
    assert_eq!(
        refine_edge(&matte, -1.0, -1.0),
        matte,
        "a negative value turns it off, it is no error"
    );
    Ok(())
}

#[test]
fn a_subject_cut_off_by_the_picture_border_is_not_pulled_in_from_the_border() -> Outcome {
    let matte = Layer::new(40, 40, rows(40, 40, |row, _| f32::from(u8::from(row < 20))))?;
    let refined = refine_edge(&matte, 3.0, 0.0);
    assert_eq!(refined.values(), rows(40, 40, |row, _| f32::from(u8::from(row < 17))));
    Ok(())
}

/// The standard normal's distribution function, by the error function's series (an independent oracle).
fn normal_cdf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.231_641_9 * x.abs());
    let poly =
        t * (0.319_381_530 + t * (-0.356_563_782 + t * (1.781_477_937 + t * (-1.821_255_978 + t * 1.330_274_429))));
    let tail = (-0.5 * x * x).exp() / (2.0 * std::f64::consts::PI).sqrt() * poly;
    if x >= 0.0 { 1.0 - tail } else { tail }
}

#[test]
fn feather_softens_a_straight_edge_like_a_gaussian_of_that_sigma_and_keeps_the_amount() -> Outcome {
    let matte = Layer::new(20, 60, half_plane(30))?;
    let soft = refine_edge(&matte, 0.0, 2.0);
    let at = |row: usize, column: usize| f64::from(soft.values()[row * 60 + column]);
    // the pixel centres beside the edge lie 0.5 px from it: a sigma-2 blur gives them normal_cdf(+-0.25)
    assert!((at(10, 30) - normal_cdf(0.25)).abs() < 0.01, "{}", at(10, 30));
    assert!((at(10, 29) - normal_cdf(-0.25)).abs() < 0.01, "{}", at(10, 29));
    let total: f64 = soft.values().iter().copied().map(f64::from).sum();
    let before: f64 = half_plane(30).iter().copied().map(f64::from).sum();
    assert!((total / before - 1.0).abs() < 1e-4, "{total} against {before}");
    Ok(())
}

#[test]
fn shrink_happens_before_feather() -> Outcome {
    let soft = refine_edge(&Layer::new(20, 60, half_plane(30))?, 3.0, 2.0); // the edge moves in to column 33
    let at = |column: usize| f64::from(soft.values()[10 * 60 + column]);
    assert!((at(33) - normal_cdf(0.25)).abs() < 0.01, "{}", at(33));
    assert!((at(32) - normal_cdf(-0.25)).abs() < 0.01, "{}", at(32));
    Ok(())
}
