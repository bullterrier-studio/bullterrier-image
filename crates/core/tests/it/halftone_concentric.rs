//! `halftone` on concentric screens (rings, spiral, rays): inks of a separation keep the tone, and rays do not crowd.

use bullterrier_image::Error;
use bullterrier_image::effects::halftone::{Params, Screen, Separation, Shape};
use bullterrier_image::params::Color;

use crate::halftone::{WHITE, mean_colour, mixed};

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
