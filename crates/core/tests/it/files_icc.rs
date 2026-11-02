//! Embedded colour profiles (`read`, `read_map`), through the public API alone. The profiles are built byte by byte
//! (`common::icc`), so each level expected is worked out from the standards by hand: the sRGB transfer function
//! (IEC 61966-2-1), CIE L* to luminance (CIE 15), and the XYZ to sRGB matrix for the D50 white point of the profile
//! connection space (ICC.1, the standard sRGB colorants).

use crate::common::Outcome;
use crate::common::files::{Png, Scratch, cmyk_jpeg, progressive_jpeg};
use crate::common::icc::{cmyk, cmyk_by_cyan, iccp, linear_grey, matrix_rgb, png, tiff, with_profile};
use bullterrier_image::{Layer, read, read_map};

/// How close a converted level is to the number worked out by hand: two levels of 255.
const LEVEL: f32 = 2.0 / 255.0;
/// How close a level that must be left as it is, or an alpha: rounding only.
const EXACT: f32 = 1e-6;
/// How close after a JPEG's rounding: a few levels of 255.
const JPEG_LEVEL: f32 = 0.03;

/// A level worked out in `f64`, as the `f32` the reader gives.
#[expect(
    clippy::cast_possible_truncation,
    reason = "narrowing is this function's whole purpose"
)]
fn single(value: f64) -> f32 {
    value as f32
}

/// The sRGB transfer function: linear light to the encoded value, both 0..1.
fn srgb_encode(linear: f64) -> f64 {
    if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// CIE L* (0..100) to relative luminance Y (0..1), for L* above 8.
fn luminance(lightness: f64) -> f64 {
    ((lightness + 16.0) / 116.0).powi(3)
}

/// The sRGB level a viewer shows for a greyscale level `l` of 0..1 in the linear profile.
fn linear_grey_to_srgb(level: f32) -> f32 {
    single(srgb_encode(f64::from(level)))
}

/// The neutral grey the CMYK profile gives any ink but black: L* 128/255*100.
fn ink_grey() -> f32 {
    single(srgb_encode(luminance(128.0 / 255.0 * 100.0)))
}

/// Adobe RGB (1998) as an ICC profile holds it: its primaries adapted to D50, and a gamma of 563/256.
const ADOBE_PRIMARIES: [[f64; 3]; 3] = [
    [0.60974, 0.31111, 0.01947],
    [0.20528, 0.62567, 0.06087],
    [0.14919, 0.06322, 0.74457],
];
const ADOBE_GAMMA: f64 = 563.0 / 256.0;

/// ROMM RGB as an ICC profile holds it: primaries far wider than sRGB's (the green and blue are not real
/// colours), D50, a gamma of 461/256 (1.8 as u8Fixed8).
const ROMM_PRIMARIES: [[f64; 3]; 3] = [
    [0.797_7, 0.288_0, 0.0],
    [0.135_1, 0.711_9, 0.0],
    [0.031_3, 0.0, 0.824_9],
];
const ROMM_GAMMA: f64 = 461.0 / 256.0;

/// What a viewer shows for an Adobe RGB colour of levels `rgb` (0..1), see [`to_srgb`].
fn adobe_to_srgb(rgb: [f32; 3]) -> [f32; 3] {
    to_srgb(ADOBE_PRIMARIES, ADOBE_GAMMA, rgb)
}

/// What a viewer shows for a colour of levels `rgb` (0..1) in the profile of `primaries` (their D50 XYZ) and one
/// `exponent` for all three curves: the levels over the exponent give linear light, the primaries give XYZ, the inverse
/// of the sRGB primaries (D50) give linear sRGB, which is clipped to 0..1 and encoded. All in `f64`.
fn to_srgb(primaries: [[f64; 3]; 3], exponent: f64, rgb: [f32; 3]) -> [f32; 3] {
    const XYZ_TO_SRGB: [[f64; 3]; 3] = [
        [3.133_856_1, -1.616_866_7, -0.490_614_6],
        [-0.978_768_4, 1.916_141_5, 0.033_454_0],
        [0.071_945_3, -0.228_991_4, 1.405_242_7],
    ];
    let linear = rgb.map(|level| f64::from(level).powf(exponent));
    let xyz: [f64; 3] =
        std::array::from_fn(|axis| (0..3).map(|primary| primaries[primary][axis] * linear[primary]).sum());
    std::array::from_fn(|channel| {
        let light: f64 = (0..3).map(|axis| XYZ_TO_SRGB[channel][axis] * xyz[axis]).sum();
        single(srgb_encode(light.clamp(0.0, 1.0)))
    })
}

/// The grey the profile that lightens with the cyan alone shows for a cyan ink of `level` of 255.
fn cyan_grey(level: f64) -> f32 {
    single(srgb_encode(luminance(100.0 * (1.0 - level / 255.0))))
}

/// `cmyk_jpeg` with its Adobe marker saying the components are YCCK rather than CMYK (transform 2 instead of 0).
fn ycck_jpeg() -> Vec<u8> {
    let mut jpeg = cmyk_jpeg();
    let marker = jpeg.windows(5).position(|bytes| bytes == b"Adobe").unwrap_or(0);
    jpeg[marker + 11] = 2; // after "Adobe": the version, two flags (2 bytes each), then the transform
    jpeg
}

fn adobe() -> Vec<u8> {
    matrix_rgb(ADOBE_PRIMARIES, ADOBE_GAMMA)
}

/// One file and the levels it must read as: red, green, blue (and alpha, if the file has it) of each pixel in turn.
struct Case {
    id: &'static str,
    file: Vec<u8>,
    expected: Vec<f32>,
    tolerance: f32,
}

fn case(id: &'static str, file: Vec<u8>, expected: Vec<f32>, tolerance: f32) -> Case {
    Case {
        id,
        file,
        expected,
        tolerance,
    }
}

fn written(bytes: &[u8]) -> std::io::Result<(Scratch, std::path::PathBuf)> {
    let folder = Scratch::new()?;
    let path = folder.write("file", bytes)?;
    Ok((folder, path))
}

/// The levels of each grey `level` over `white`, repeated for red, green and blue.
fn grey_rgb(levels: &[f32]) -> Vec<f32> {
    levels.iter().flat_map(|&level| [level; 3]).collect()
}

fn assert_read(case: &Case) -> Outcome {
    let (_folder, path) = written(&case.file)?;
    let image = read(&path)?;
    let channels = image.channels();
    let values = image.into_values();
    assert_eq!(values.len(), case.expected.len(), "{}", case.id);
    for (index, (value, wanted)) in values.iter().zip(&case.expected).enumerate() {
        let alpha = channels == 4 && index % 4 == 3;
        let tolerance = if alpha { EXACT } else { case.tolerance };
        assert!(
            (value - wanted).abs() <= tolerance,
            "{}: value {index} is {value}, not {wanted}",
            case.id
        );
    }
    Ok(())
}

#[expect(clippy::too_many_lines, reason = "a table of rows, one file each")]
fn applied_cases() -> Vec<Case> {
    let grey = [128_u8, 0, 255, 64];
    let grey_levels: Vec<f32> = grey
        .iter()
        .map(|&level| linear_grey_to_srgb(f32::from(level) / 255.0))
        .collect();
    let adobe_pixels: [[u8; 3]; 5] = [
        [153, 128, 102],
        [128, 128, 128],
        [0, 0, 0],
        [255, 255, 255],
        [255, 0, 0],
    ];
    let adobe_expected: Vec<f32> = adobe_pixels
        .iter()
        .flat_map(|pixel| adobe_to_srgb(pixel.map(|level| f32::from(level) / 255.0)))
        .collect();
    let adobe_row: Vec<u8> = adobe_pixels.concat();
    let adobe_row16: Vec<u8> = adobe_row.iter().flat_map(|&level| [level, level]).collect();
    let romm_pixels: [[u8; 3]; 3] = [[164, 248, 206], [200, 100, 50], [255, 255, 255]];
    let romm_expected: Vec<f32> = romm_pixels
        .iter()
        .flat_map(|pixel| to_srgb(ROMM_PRIMARIES, ROMM_GAMMA, pixel.map(|level| f32::from(level) / 255.0)))
        .collect();
    let inks: [u8; 12] = [
        0, 255, 255, 0, /* naively red */ 0, 0, 0, 0, /* white */ 0, 0, 0, 255, /* black */
    ];
    let ink_expected = [ink_grey(); 3].into_iter().chain([1.0; 3]).chain([0.0; 3]).collect();
    vec![
        case(
            "a grey profile is applied to the levels of a PNG",
            png(4, 0, &grey, &linear_grey()),
            grey_rgb(&grey_levels),
            LEVEL,
        ),
        case(
            "a grey profile is applied to the levels of a TIFF",
            tiff(4, 1, &grey, &linear_grey()),
            grey_rgb(&grey_levels),
            LEVEL,
        ),
        case(
            "a grey profile is applied under transparency and leaves the alpha as it is",
            png(3, 4, &[128, 255, 128, 128, 200, 0], &linear_grey()),
            [128_u8, 128, 200]
                .iter()
                .zip([1.0, 128.0 / 255.0, 0.0])
                .flat_map(|(&level, alpha)| {
                    let shown = linear_grey_to_srgb(f32::from(level) / 255.0);
                    [shown, shown, shown, alpha]
                })
                .collect(),
            LEVEL,
        ),
        case(
            "an RGB profile whose primaries are not sRGB's is applied to a PNG",
            png(5, 2, &adobe_row, &adobe()),
            adobe_expected.clone(),
            LEVEL,
        ),
        case(
            "colours beyond sRGB's gamut are converted exactly, then clipped",
            png(3, 2, &romm_pixels.concat(), &matrix_rgb(ROMM_PRIMARIES, ROMM_GAMMA)),
            romm_expected,
            LEVEL,
        ),
        case(
            "an RGB profile is applied to a TIFF",
            tiff(5, 2, &adobe_row, &adobe()),
            adobe_expected.clone(),
            LEVEL,
        ),
        case(
            "an RGB profile is applied to 16-bit levels",
            Png::new(5, 1, 16, 2, &[&adobe_row16]).before(iccp(&adobe())).bytes(),
            adobe_expected,
            LEVEL,
        ),
        case(
            "a CMYK profile is applied to the inks of a TIFF, not to a naive RGB conversion",
            tiff(3, 5, &inks, &cmyk()),
            ink_expected,
            LEVEL,
        ),
        case(
            "a CMYK profile is applied to the inks of a JPEG",
            with_profile(&cmyk_jpeg(), &cmyk()),
            vec![ink_grey(); 3 * 64],
            JPEG_LEVEL,
        ),
        case(
            "a CMYK JPEG's inks are not inverted again: no cyan is white",
            with_profile(&cmyk_jpeg(), &cmyk_by_cyan()),
            vec![1.0; 3 * 64],
            JPEG_LEVEL,
        ),
        case(
            "a YCCK profile reads the inks of the YCC components: cyan is the red they make",
            with_profile(&ycck_jpeg(), &cmyk_by_cyan()),
            // the components 255, 0, 0, 255 are luma 255, blue and red differences -128 and -128: red 255 - 1.402 * 128
            vec![cyan_grey(255.0 - 1.402 * 128.0); 3 * 64],
            JPEG_LEVEL,
        ),
    ]
}

#[test]
fn an_embedded_profile_is_converted_to_srgb_from_the_colour_model_of_the_file() -> Outcome {
    for case in applied_cases() {
        assert_read(&case)?;
    }
    Ok(())
}

/// What the file reads as when its profile is not used: the levels it stores.
fn untouched_cases() -> Vec<Case> {
    let rgb = [255_u8, 0, 0, 0, 102, 255, 30, 60, 90];
    let stored: Vec<f32> = rgb.iter().map(|&level| f32::from(level) / 255.0).collect();
    let grey = [128_u8, 0, 255];
    let stored_grey = grey_rgb(&[128.0 / 255.0, 0.0, 1.0]);
    let adobe_head = adobe()[..100].to_vec();
    let inks = [
        0_u8, 255, 255, 0, /* naively red */ 0, 0, 0, 0, /* white */ 0, 0, 0, 255, /* black */
    ];
    let naive = [1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0];
    let wide: Vec<u8> = [0_u16, 16384, 65535]
        .iter()
        .flat_map(|level| level.to_be_bytes())
        .collect();
    vec![
        case(
            "an unreadable profile leaves an RGB PNG as it is",
            png(3, 2, &rgb, b"not a profile at all"),
            stored.clone(),
            EXACT,
        ),
        case(
            "a truncated profile leaves an RGB PNG as it is",
            png(3, 2, &rgb, &adobe_head),
            stored.clone(),
            EXACT,
        ),
        case(
            "an unreadable profile leaves a grey TIFF as it is",
            tiff(3, 1, &grey, b"not a profile at all"),
            stored_grey.clone(),
            EXACT,
        ),
        case(
            "an unreadable profile leaves a CMYK TIFF to the naive conversion",
            tiff(3, 5, &inks, b"not a profile at all"),
            naive.to_vec(),
            EXACT,
        ),
        case(
            "an unreadable profile leaves a JPEG as it is",
            with_profile(&progressive_jpeg(), b"not a profile at all"),
            [200.0 / 255.0, 100.0 / 255.0, 50.0 / 255.0].repeat(64),
            JPEG_LEVEL,
        ),
        case(
            "a grey profile on an RGB PNG is for another colour space",
            png(3, 2, &rgb, &linear_grey()),
            stored.clone(),
            EXACT,
        ),
        case(
            "a CMYK profile on an RGB PNG is for another colour space",
            png(3, 2, &rgb, &cmyk()),
            stored.clone(),
            EXACT,
        ),
        case(
            "an RGB profile on a grey PNG is for another colour space",
            png(3, 0, &grey, &adobe()),
            stored_grey.clone(),
            EXACT,
        ),
        case(
            "an RGB profile on a CMYK TIFF is for another colour space",
            tiff(3, 5, &inks, &adobe()),
            naive.to_vec(),
            EXACT,
        ),
        case(
            "an RGB profile on a CMYK JPEG is for another colour space",
            with_profile(&cmyk_jpeg(), &adobe()),
            [1.0, 0.0, 0.0].repeat(64),
            JPEG_LEVEL,
        ),
        case(
            "a grey profile on a CMYK JPEG is for another colour space",
            with_profile(&cmyk_jpeg(), &linear_grey()),
            [1.0, 0.0, 0.0].repeat(64),
            JPEG_LEVEL,
        ),
        case(
            "a profile on 16-bit grey is not applied: the levels are taken as sRGB",
            Png::new(3, 1, 16, 0, &[&wide]).before(iccp(&linear_grey())).bytes(),
            grey_rgb(&[0.0, 16384.0 / 65535.0, 1.0]),
            EXACT,
        ),
    ]
}

#[test]
fn a_profile_that_cannot_be_used_leaves_the_levels_as_they_are_without_an_error() -> Outcome {
    for case in untouched_cases() {
        assert_read(&case)?;
    }
    Ok(())
}

/// A file with a profile: what it is, the file, the levels a map must give, the levels a read gives (first channel).
type Mapped<'a> = (&'a str, Vec<u8>, Vec<f32>, Vec<f32>);

#[test]
fn a_map_is_data_so_an_embedded_profile_is_not_applied_to_it_while_a_read_applies_it() -> Outcome {
    let rgb = [[153_u8, 128, 102], [255, 0, 0], [30, 60, 90]];
    let stored = |pixel: &[u8; 3]| {
        let [red, green, blue] = pixel.map(|level| f32::from(level) / 255.0);
        (299.0 * red + 587.0 * green + 114.0 * blue) / 1000.0 // BT.601 luma of the stored levels
    };
    let cases: [Mapped; 2] = [
        (
            "an RGB file under an Adobe RGB profile",
            png(3, 2, &rgb.concat(), &adobe()),
            rgb.iter().map(stored).collect(),
            rgb.iter()
                .map(|pixel| {
                    let [red, green, blue] = adobe_to_srgb(pixel.map(|level| f32::from(level) / 255.0));
                    (299.0 * red + 587.0 * green + 114.0 * blue) / 1000.0
                })
                .collect(),
        ),
        (
            "a grey file under a linear profile",
            png(3, 0, &[128, 0, 255], &linear_grey()),
            vec![128.0 / 255.0, 0.0, 1.0],
            [128.0 / 255.0, 0.0, 1.0].map(linear_grey_to_srgb).to_vec(),
        ),
    ];
    for (id, file, stored, converted) in cases {
        let (_folder, path) = written(&file)?;
        let map: Layer = read_map(&path)?;
        assert_eq!(map.values().len(), stored.len(), "{id}");
        for (index, (value, wanted)) in map.values().iter().zip(&stored).enumerate() {
            assert!(
                (value - wanted).abs() <= EXACT,
                "{id}: the map's value {index} is {value}, not the stored {wanted}"
            );
        }
        // the same file read as a picture is converted: the two differ, so the test sees which one was done
        let seen = read(&path)?.into_values();
        let first = seen.as_chunks::<3>().0.iter().map(|pixel| pixel[0]).collect::<Vec<_>>();
        assert!(
            converted.len() == first.len() && converted.iter().zip(&stored).any(|(c, s)| (c - s).abs() > LEVEL),
            "{id}: the profile must change some level, or this test shows nothing"
        );
    }
    Ok(())
}

#[test]
fn a_cmyk_jpeg_gives_the_same_map_with_a_profile_as_without() -> Outcome {
    let (_plain, without) = written(&cmyk_jpeg())?;
    let (_tagged, with) = written(&with_profile(&cmyk_jpeg(), &cmyk()))?;
    assert!(
        read_map(&without)? == read_map(&with)?,
        "the CMYK profile changed the map"
    );
    Ok(())
}

#[test]
fn a_grey_file_stays_grey_under_its_profile() -> Outcome {
    let (_folder, path) = written(&png(4, 0, &[128, 0, 255, 64], &linear_grey()))?;
    for pixel in read(&path)?.into_values().as_chunks::<3>().0 {
        assert!(
            pixel.iter().all(|level| level.to_bits() == pixel[0].to_bits()),
            "red, green and blue are {pixel:?}"
        );
    }
    Ok(())
}
