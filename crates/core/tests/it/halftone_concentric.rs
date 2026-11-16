//! `halftone` on concentric screens (rings, spiral, rays): inks of a separation keep the tone, and rays do not crowd.

#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "small picture sizes and indices"
)]

use bullterrier_image::Error;
use bullterrier_image::effects::halftone::{Params, Screen, Separation, Shape};
use bullterrier_image::params::Color;

use crate::halftone::{WHITE, bent, flat, ink_where, mean_colour, mixed, red};

/// The reviewer's two inks, `#ff4d6d` and `#2b59ff`.
const PINK: [f32; 3] = [1.0, 0.302, 0.427];
const BLUE: [f32; 3] = [0.169, 0.349, 1.0];

fn two_inks(screen: Screen, shape: Shape) -> Params {
    Params {
        screen,
        shape,
        cell: 12.0,
        sigma: 0.0,
        feather: 1.0,
        separation: Separation::Spot,
        inks: vec![Color(PINK), Color(BLUE)],
        paper: WHITE,
        ..Params::default()
    }
}

/// Two inks on a concentric screen print the colours they can mix at those colours, as on a grid: each ink's area is its
/// amount within 0.01 (see `halftone.rs`), a channel moves by at most the sum of both, so 0.02. Oracle: the print model
/// `paper x prod (1 - area (1 - ink / paper))`. Before the fix every ink's rows coincided and the green of 0.5 + 0.5
/// (0.439) printed 0.48 to 0.54.
#[test]
fn two_inks_on_a_concentric_screen_print_the_colours_they_can_mix_at_those_colours() -> Result<(), Error> {
    for screen in [Screen::Rings, Screen::Spiral, Screen::Rays] {
        for shape in [Shape::Dot, Shape::Line] {
            let params = two_inks(screen, shape);
            for areas in [[0.3, 0.4], [0.6, 0.1], [0.2, 0.7], [0.5, 0.5]] {
                let photo = mixed([1.0; 3], &[PINK, BLUE], &areas);
                let got = mean_colour(&params, photo)?;
                assert!(
                    (0..3).all(|c| (got[c] - photo[c]).abs() <= 0.02),
                    "{screen:?} {shape:?} areas {areas:?}: printed {got:?}, wanted {photo:?}"
                );
            }
        }
    }
    Ok(())
}

// ---- rays do not crowd at the centre ---------------------------------------------------------------------------------

/// The oracle rays (cell 20, 401 px, centre pixel 200, black on white) at an even `tone`: the ink of every pixel.
fn rays_ink(shape: Shape, tone: f32) -> Result<Vec<f32>, Error> {
    Ok(red(&bent(Screen::Rays, shape), 401, 401, &flat(401, 401, 1.0 - tone))?
        .into_iter()
        .map(|r| 1.0 - r)
        .collect())
}

/// The number of rays the circle of `radius` cells about the centre crosses: runs of ink (above a half) going round it,
/// sampled every 1/8 px of arc at the nearest pixel.
fn rays_round(ink: &[f32], radius: f32) -> usize {
    let px = radius * 20.0;
    let samples = (std::f32::consts::TAU * px * 8.0) as usize;
    let at = |k: usize| {
        let angle = std::f32::consts::TAU * k as f32 / samples as f32;
        let (x, y) = (200.0 + px * angle.cos(), 200.0 + px * angle.sin());
        ink[y.round() as usize * 401 + x.round() as usize] > 0.5
    };
    (0..samples)
        .filter(|&k| at(k) && !at((k + samples - 1) % samples))
        .count()
}

/// The spacing of neighbouring rays on the circle of a cell's middle radius is from half a cell up to a cell (62 rays at
/// the half-way radius of 10 cells and fewer inside, each count half the one outside), so the rays number from `2 pi r` up to
/// `4 pi r` there (one more for the last, narrower ray), where there were 62 at every radius and, at 1.5 cells, 62 rays on a
/// circle 9.4 cells round.
#[test]
fn rays_are_never_closer_than_half_a_cell() -> Result<(), Error> {
    let ink = rays_ink(Shape::Line, 0.5)?;
    for radius in [1.5_f32, 2.5, 3.5, 4.5, 6.5, 8.5, 9.5] {
        let rays = rays_round(&ink, radius);
        let (fewest, most) = (
            std::f32::consts::TAU * radius,
            2.0 * std::f32::consts::TAU * radius + 1.0,
        );
        assert!(
            (fewest..=most).contains(&(rays as f32)),
            "radius {radius}: {rays} rays, wanted {fewest:.1} to {most:.1}"
        );
    }
    Ok(())
}

/// The tone holds in every ring of radius, near the centre too: the mean ink of the flat tone in rings of whole cells from 2
/// cells out is within 0.004 (`every_screen_geometry_keeps_the_tone`: the bound of the far rings, where dots measure 0.002), and
/// a line's from 1 cell out (before, the 0.25 and 0.75 tones printed 0.2665 and 0.7335 at 1 to 1.5 cells, where wedges were
/// narrower than a pixel; a dot's rings under 2 cells are a handful of curved cells and stay off by up to 0.03).
#[test]
fn rays_keep_the_tone_in_every_ring_of_radius() -> Result<(), Error> {
    let rings = |shape: Shape, tone: f32, from: f32, to: f32| {
        ink_where(&bent(Screen::Rays, shape), 401, tone, |dx, dy| {
            (from * 20.0..to * 20.0).contains(&dx.hypot(dy))
        })
        .map(|ink| (ink - tone).abs())
    };
    for tone in [0.25, 0.5, 0.75] {
        for (from, to) in [(2.0, 3.0), (3.0, 4.0), (4.0, 6.0), (6.0, 9.5)] {
            for shape in [Shape::Line, Shape::Dot] {
                let off = rings(shape, tone, from, to)?;
                assert!(off < 0.004, "{shape:?} tone {tone}, {from} to {to} cells: off by {off}");
            }
        }
        for (from, to) in [(1.0, 1.5), (1.5, 2.0)] {
            let off = rings(Shape::Line, tone, from, to)?;
            assert!(off < 0.004, "line tone {tone}, {from} to {to} cells: off by {off}");
        }
    }
    Ok(())
}
