//! `dither` with `separation`: spot inks laid over the paper, and the print realism (dot gain, ink noise, specks) of that
//! separation. The oracles are the paper under each combination of the full inks, the unmixed coverage of an ink, and the
//! mean tone of a flat grey.

use bullterrier_image::Error;
use bullterrier_image::effects::dither::{Method, Params, Separation, Space};
use bullterrier_image::params::Color;
use serde_json::json;

use crate::common::dither::{BLACK, WHITE, grey_matrix, near, pixels, rgb, run, with_matrix};
use crate::common::pictures::varied;
use crate::common::pixels::run_picture;

// ---- inks ---------------------------------------------------------------------------------------------------------------

const PAPER: [f32; 3] = [0.95, 0.93, 0.88];
const PINK: [f32; 3] = [1.0, 0.3, 0.45];
const BLUE: [f32; 3] = [0.17, 0.35, 1.0];
const YELLOW: [f32; 3] = [1.0, 0.9, 0.1];

fn inked(method: Method, inks: &[[f32; 3]]) -> Params {
    Params {
        method,
        separation: Separation::Spot,
        inks: inks.iter().map(|&ink| Color(ink)).collect(),
        paper: Color(PAPER),
        ..Params::default()
    }
}

/// The colour of the inks whose bits are set in `mask` laid over the paper: each full ink multiplies a channel by its ratio
/// to the paper (an ink lighter than the paper in a channel leaves it as it is).
fn laid(inks: &[[f32; 3]], mask: usize) -> [f32; 3] {
    [0, 1, 2].map(|c| {
        (0..inks.len())
            .filter(|ink| mask >> ink & 1 == 1)
            .fold(PAPER[c], |light, ink| light * (inks[ink][c] / PAPER[c]).min(1.0))
    })
}

/// Which inks (as bits) landed on each pixel, read from its colour; none of the combinations may be missed.
fn landed(inks: &[[f32; 3]], picture: &[f32]) -> Vec<usize> {
    pixels(picture)
        .into_iter()
        .map(|pixel| {
            (0..1 << inks.len())
                .find(|&mask| near(pixel, laid(inks, mask)))
                .unwrap_or(usize::MAX)
        })
        .collect()
}

#[test]
fn every_pixel_is_the_paper_under_some_combination_of_the_full_inks() -> Result<(), Error> {
    let picture = (32, 32, varied(32, 32));
    for inks in [vec![PINK], vec![PINK, BLUE], vec![PINK, BLUE, YELLOW]] {
        for method in [
            Method::FloydSteinberg,
            Method::Atkinson,
            Method::Bayer,
            Method::BlueNoise,
        ] {
            let params = Params {
                cell: 2.0,
                ..inked(method, &inks)
            };
            let out = run(&params, &picture)?;
            let layers = landed(&inks, &out);
            assert!(
                layers.iter().all(|&mask| mask != usize::MAX),
                "{} inks, {method:?}",
                inks.len()
            );
            // and the combinations are used: every ink lands somewhere
            for ink in 0..inks.len() {
                assert!(layers.iter().any(|mask| mask >> ink & 1 == 1), "ink {ink} never lands");
            }
        }
    }
    Ok(())
}

#[test]
fn the_share_of_pixels_an_ink_lands_on_is_its_unmixed_coverage() -> Result<(), Error> {
    // 0.3 of pink and 0.6 of blue, printed as the flat colour to redraw
    let flat = [0, 1, 2].map(|c| {
        PAPER[c]
            * (1.0 - 0.3 * (1.0 - (PINK[c] / PAPER[c]).min(1.0)))
            * (1.0 - 0.6 * (1.0 - (BLUE[c] / PAPER[c]).min(1.0)))
    });
    let picture = (64, 64, flat.repeat(64 * 64));
    // Atkinson drops a quarter of the error, so it is left out: its tones are lighter or darker by design. The ordered
    // methods work in sixteenths, so they are off by up to a thirty-second.
    for method in [Method::FloydSteinberg, Method::Bayer, Method::BlueNoise] {
        let out = run(&inked(method, &[PINK, BLUE]), &picture)?;
        let layers = landed(&[PINK, BLUE], &out);
        for (ink, coverage) in [(0, 0.3), (1, 0.6)] {
            let landing = layers.iter().filter(|mask| *mask >> ink & 1 == 1).count();
            let share = f32::from(u16::try_from(landing).unwrap_or(0)) / 4096.0;
            assert!((share - coverage).abs() < 0.04, "{method:?} ink {ink}: {share}");
        }
    }
    Ok(())
}

#[test]
fn paper_stays_paper_and_a_full_ink_stays_that_ink() -> Result<(), Error> {
    for method in [
        Method::FloydSteinberg,
        Method::Atkinson,
        Method::Bayer,
        Method::BlueNoise,
    ] {
        let on = |colour: [f32; 3]| -> Result<Vec<[f32; 3]>, Error> {
            let picture = (16, 16, colour.repeat(256));
            Ok(pixels(&run(&inked(method, &[PINK, BLUE]), &picture)?))
        };
        assert!(on(PAPER)?.iter().all(|&pixel| pixel == PAPER), "{method:?}");
        assert!(
            on(PINK)?.iter().all(|&pixel| near(pixel, laid(&[PINK, BLUE], 1))),
            "{method:?}"
        );
    }
    Ok(())
}

#[test]
fn the_inks_matter_only_with_a_separation() -> Result<(), Error> {
    let picture = (24, 24, varied(24, 24));
    let plain = run(&Params::default(), &picture)?;
    let with_unused = Params {
        inks: vec![Color(YELLOW)],
        paper: Color(PAPER),
        offset: 9.0,
        offset_angle: 33.0,
        ..Params::default()
    };
    assert_eq!(plain, run(&with_unused, &picture)?);
    Ok(())
}

#[test]
fn ink_k_lands_k_offsets_away_along_the_angle_and_no_offset_leaves_the_angle_unused() -> Result<(), Error> {
    let picture = (30, 30, varied(30, 30));
    let inks = [PINK, BLUE, YELLOW];
    let base = inked(Method::FloydSteinberg, &inks);
    let in_register = landed(&inks, &run(&base, &picture)?);
    let turned = Params {
        offset_angle: 70.0,
        ..base.clone()
    };
    assert_eq!(in_register, landed(&inks, &run(&turned, &picture)?), "offset 0");
    // (right, down) of one offset, as whole pixels
    for (offset, angle, step) in [
        (3.0, 0.0, (3, 0)),
        (2.0, 90.0, (0, 2)),
        (2.0, 180.0, (-2, 0)),
        (4.0, 360.0, (4, 0)),
    ] {
        let moved = Params {
            offset,
            offset_angle: angle,
            ..base.clone()
        };
        let got = landed(&inks, &run(&moved, &picture)?);
        for row in 0..30_isize {
            for column in 0..30_isize {
                for ink in 0..3_isize {
                    let (from_row, from_column) = (row - step.1 * ink, column - step.0 * ink);
                    let expected = if (0..30).contains(&from_row) && (0..30).contains(&from_column) {
                        in_register[(from_row * 30 + from_column).cast_unsigned()] >> ink & 1
                    } else {
                        0
                    };
                    assert_eq!(
                        got[(row * 30 + column).cast_unsigned()] >> ink & 1,
                        expected,
                        "ink {ink} at {row},{column}, offset {offset} at {angle}"
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn separation_inks_is_refused_naming_the_choice_and_spot_prints_the_inks() {
    let picture = (4, 4, vec![0.5_f32; 48]);
    let refused = with_matrix("floyd_steinberg", None, &[("separation", json!("inks"))], &picture);
    assert!(
        matches!(&refused, Err(error) if error.to_string().contains("inks")),
        "{refused:?}"
    );
    assert!(with_matrix("floyd_steinberg", None, &[("separation", json!("spot"))], &picture).is_ok());
}

// ---- print realism (spot separation) ------------------------------------------------------------------------------------

/// One black ink on white paper, error diffused, `cell` 1: the mean ink of a flat grey is its coverage.
fn black_ink() -> Params {
    Params {
        method: Method::FloydSteinberg,
        separation: Separation::Spot,
        inks: vec![Color(BLACK)],
        paper: Color(WHITE),
        ..Params::default()
    }
}

/// The ink of an even picture that a black ink at coverage `coverage` prints on white paper: its colour is the paper
/// less the coverage (the ink's ratio to the paper is the floor 1e-4, see `ops::inks`).
fn ink_of(params: &Params, size: usize, coverage: f32) -> Result<Vec<f32>, Error> {
    let values = vec![1.0 - coverage * (1.0 - 1e-4); size * size * 3];
    Ok(run_picture(params, size, size, &values)?
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| 1.0 - pixel[0])
        .collect())
}

fn mean_of(values: &[f32]) -> f32 {
    values.iter().sum::<f32>() / f32::from(u16::try_from(values.len()).unwrap_or(u16::MAX)) // the pictures here have under 65536 pixels
}

#[test]
fn dither_spot_print_realism_at_zero_is_the_clean_picture() -> Result<(), Error> {
    let rgb = varied(40, 50);
    let clean = inked(Method::Bayer, &[PINK, BLUE]);
    let zeroed = Params {
        speck_size: 8.0,
        ..clean.clone()
    };
    assert_eq!(run_picture(&zeroed, 40, 50, &rgb)?, run_picture(&clean, 40, 50, &rgb)?);
    Ok(())
}

#[test]
fn dither_spot_dot_gain_raises_the_ink_to_the_curve() -> Result<(), Error> {
    for (gain, tone) in [(0.25, 0.5), (0.1, 0.5), (0.25, 0.2), (0.2, 0.8)] {
        let params = Params {
            dot_gain: gain,
            ..black_ink()
        };
        let want = tone + 4.0 * gain * tone * (1.0 - tone);
        let got = mean_of(&ink_of(&params, 100, tone)?);
        assert!(
            (got - want).abs() < 0.02,
            "gain {gain} tone {tone}: ink {got}, formula {want}"
        );
    }
    Ok(())
}

#[test]
fn dither_spot_ink_noise_is_seeded_and_keeps_the_mean() -> Result<(), Error> {
    let noisy = |seed| Params {
        ink_noise: 1.0,
        seed,
        cell: 4.0,
        ..black_ink()
    };
    let (a, again, b) = (
        ink_of(&noisy(1), 200, 0.5)?,
        ink_of(&noisy(1), 200, 0.5)?,
        ink_of(&noisy(2), 200, 0.5)?,
    );
    assert_eq!(a, again);
    assert_ne!(a, b);
    assert_ne!(
        a,
        ink_of(
            &Params {
                ink_noise: 0.0,
                ..noisy(1)
            },
            200,
            0.5
        )?
    );
    assert!((mean_of(&a) - 0.5).abs() < 0.03, "{}", mean_of(&a));
    Ok(())
}

#[test]
fn dither_spot_specks_bare_the_paper_only_in_ink_and_their_density_is_the_parameter() -> Result<(), Error> {
    let specked = |specks, speck_size| Params {
        specks,
        speck_size,
        seed: 3,
        ..black_ink()
    };
    // a full tone is ink everywhere: the specks are the paper
    let full = ink_of(&specked(2.0, 3.0), 300, 1.0)?;
    let paper_pixels = full.iter().filter(|&&ink| ink < 0.5).count();
    // each speck of 3 px diameter covers about pi x 1.5^2 = 7 px (the ramp's half pixel adds a little), 180 of them
    assert!((900..=1500).contains(&paper_pixels), "{paper_pixels}");
    // no ink, no specks
    assert_eq!(ink_of(&specked(5.0, 10.0), 100, 0.0)?, ink_of(&black_ink(), 100, 0.0)?);
    // a speck only takes ink away
    let (with, without) = (ink_of(&specked(5.0, 4.0), 150, 0.5)?, ink_of(&black_ink(), 150, 0.5)?);
    assert!(with.iter().zip(&without).all(|(w, wo)| *w <= *wo + 1e-6));
    assert!(with.iter().zip(&without).any(|(w, wo)| *w < *wo - 0.5));
    Ok(())
}

#[test]
fn dither_spot_prints_the_tone_of_what_the_inks_can_mix() -> Result<(), Error> {
    // fine cells, error diffused: the share of pixels each ink lands on is its amount, the layers overlap independently
    let mean = |inks: &[[f32; 3]], photo: [f32; 3]| -> Result<[f32; 3], Error> {
        let params = Params {
            paper: Color(WHITE),
            ..inked(Method::FloydSteinberg, inks)
        };
        let out = run_picture(&params, 100, 100, &photo.repeat(100 * 100))?;
        Ok([0, 1, 2].map(|c| out.as_chunks::<3>().0.iter().map(|pixel| pixel[c]).sum::<f32>() / 10_000.0))
    };
    for grey in [0.2, 0.5, 0.8] {
        let got = mean(&[BLACK], [grey; 3])?;
        assert!(got.iter().all(|c| (c - grey).abs() < 0.03), "grey {grey}: {got:?}");
    }
    let inks = [PINK, BLUE];
    for areas in [[0.3_f32, 0.4], [0.6, 0.1]] {
        let want =
            [0, 1, 2].map(|c| (0..2).fold(1.0, |light, i| light * (1.0 - areas[i] * (1.0 - inks[i][c].min(1.0)))));
        let got = mean(&inks, want)?;
        assert!(
            (0..3).all(|c| (got[c] - want[c]).abs() < 0.03),
            "areas {areas:?}: {got:?} against {want:?}"
        );
    }
    Ok(())
}

/// The mean of the output of `params` on a flat `photo` of 240 x 240, per channel.
fn flat_mean(params: &Params, photo: [f32; 3]) -> Result<[f32; 3], Error> {
    let out = run_picture(params, 240, 240, &photo.repeat(240 * 240))?;
    Ok([0, 1, 2].map(|c| out.as_chunks::<3>().0.iter().map(|pixel| pixel[c]).sum::<f32>() / 57_600.0))
}

/// Every method that dithers keeps the tone of what the inks mix: the output's mean against the print model
/// `paper x prod (1 - a (1 - ink / paper))` of the amounts. The bound is the ordered mixing's step, a sixteenth of the
/// distance between two palette colours (at most 1 per channel): Knoll's mixing of 16 candidates leaves at most one
/// palette distance of error owed, spread over the 16, so a channel's mean is within 1/16.
#[test]
fn dither_spot_keeps_the_tone_of_every_method_with_several_inks_and_with_one_black_ink() -> Result<(), Error> {
    let bound = 1.0 / 16.0;
    let methods = [
        Method::Bayer,
        Method::Clustered,
        Method::BlueNoise,
        Method::WhiteNoise,
        Method::FloydSteinberg,
        Method::Riemersma,
        Method::DotDiffusion,
        Method::Ostromoukhov,
        Method::GameboyCamera,
    ];
    let inks = [PINK, BLUE];
    let two = [0, 1, 2].map(|c| (0..2).fold(1.0, |light, i| light * (1.0 - 0.5 * (1.0 - inks[i][c].min(1.0)))));
    for method in methods {
        for space in [Space::Oklab, Space::Rgb] {
            let params = Params {
                cell: 2.0,
                paper: Color(WHITE),
                space,
                ..inked(method, &inks)
            };
            let got = flat_mean(&params, two)?;
            assert!(
                (0..3).all(|c| (got[c] - two[c]).abs() < bound),
                "{method:?} {space:?}: {got:?} against {two:?}"
            );
            for grey in [0.2, 0.5, 0.8] {
                let params = Params {
                    cell: 2.0,
                    paper: Color(WHITE),
                    space,
                    ..inked(method, &[BLACK])
                };
                let got = flat_mean(&params, [grey; 3])?;
                assert!(
                    got.iter().all(|c| (c - grey).abs() < bound),
                    "{method:?} {space:?} black on grey {grey}: {got:?}"
                );
            }
        }
    }
    Ok(())
}

/// The mean per channel of a picture of `rgb` values.
fn channel_means(rgb: &[f32]) -> [f32; 3] {
    let count = f32::from(u16::try_from(rgb.len() / 3).unwrap_or(u16::MAX)); // the pictures here have under 65536 pixels
    [0, 1, 2].map(|c| rgb.as_chunks::<3>().0.iter().map(|pixel| pixel[c]).sum::<f32>() / count)
}

/// Every ordered and noise method prints the exact share of every overprint: two inks at one half each print the
/// independent-overlap mix (the default inks over white give green 0.439), where a mix in sixteenths gave 0.410. Shares of
/// a quarter are whole cells of every matrix (2 x 2 or larger); 128 is a multiple of every tile side, 0.01 the noise's
/// sampling spread.
#[test]
fn dither_spot_ordered_and_noise_methods_print_the_exact_mix_of_the_inks() -> Result<(), Error> {
    let (pink, blue) = (rgb(0xff_4d6d), rgb(0x2b_59ff));
    let want = [0, 1, 2].map(|c| 0.25 * 1.0 + 0.25 * pink[c] + 0.25 * blue[c] + 0.25 * pink[c] * blue[c]);
    let matrix = grey_matrix(
        4,
        4,
        &[
            0.9, 0.1, 0.5, 0.3, 0.7, 0.2, 0.8, 0.4, 0.0, 0.6, 0.15, 0.95, 0.35, 0.55, 0.25, 0.75,
        ],
    )?;
    let picture = (128, 128, want.repeat(128 * 128));
    for method in [
        "bayer2",
        "bayer4",
        "bayer",
        "clustered",
        "clustered45",
        "blue_noise",
        "white_noise",
        "gameboy_camera",
        "matrix",
    ] {
        let extra = [
            ("separation", json!("spot")),
            ("paper", json!("#ffffff")),
            ("matrix", json!("m")),
        ];
        let out = with_matrix(method, Some(&matrix), &extra, &picture)?;
        let got = channel_means(&out);
        assert!(
            (0..3).all(|c| (got[c] - want[c]).abs() < 0.01),
            "{method}: {got:?} against {want:?}"
        );
    }
    Ok(())
}

#[test]
fn dither_spot_ordered_and_noise_methods_print_one_black_ink_at_its_grey() -> Result<(), Error> {
    // a matrix of n cells holds a share only to 1 / n: the bound adds half of that
    for (method, cells) in [
        ("bayer2", 4.0_f32),
        ("bayer", 64.0),
        ("clustered45", 64.0),
        ("blue_noise", 4096.0),
        ("white_noise", 65536.0),
        ("gameboy_camera", 16.0),
    ] {
        for grey in [0.2_f32, 0.5, 0.8] {
            let extra = [
                ("separation", json!("spot")),
                ("paper", json!("#ffffff")),
                ("inks", json!(["#000000"])),
            ];
            let out = with_matrix(method, None, &extra, &(128, 128, vec![grey; 128 * 128 * 3]))?;
            let got = channel_means(&out);
            assert!(
                got.iter().all(|c| (c - grey).abs() < 0.01 + 0.5 / cells),
                "{method} grey {grey}: {got:?}"
            );
        }
    }
    Ok(())
}
