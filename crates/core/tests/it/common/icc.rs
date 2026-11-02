//! ICC profiles built byte by byte (ICC.1:2001, version 2), and the image files that carry them. Real CMYK and grey
//! profiles are large licensed files; these are minimal ones whose conversion is worked out by hand.

use super::files::{Png, chunk, zlib};

/// The ICC profile connection space white point (D50) as s15Fixed16: 0.9642, 1.0, 0.8249.
const D50: [i32; 3] = [63190, 65536, 54061];

/// `value` as an s15Fixed16 number, four bytes.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a colorant is between -32768 and 32768, which s15Fixed16 holds"
)]
fn s15(value: f64) -> [u8; 4] {
    ((value * 65536.0).round() as i32).to_be_bytes()
}

/// An ICC v2 profile with a white point and the given tags: the 128-byte header, the tag table, the tags.
fn profile(class: [u8; 4], space: [u8; 4], pcs: [u8; 4], tags: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let white = [b"XYZ ".as_slice(), &[0; 4], &D50.map(i32::to_be_bytes).concat()].concat();
    let tags = [&[(b"wtpt", white)][..], tags].concat();
    let start = 128 + 4 + 12 * tags.len();
    let (mut table, mut data) = (Vec::new(), Vec::<u8>::new());
    for (signature, content) in &tags {
        let padding = content.len().wrapping_neg() % 4;
        let offset = u32::try_from(start + data.len()).unwrap_or(u32::MAX);
        let length = u32::try_from(content.len() + padding).unwrap_or(u32::MAX);
        table.extend([signature.as_slice(), &offset.to_be_bytes(), &length.to_be_bytes()].concat());
        data.extend(content);
        data.extend(std::iter::repeat_n(0, padding));
    }
    let size = u32::try_from(start + data.len()).unwrap_or(u32::MAX);
    let mut bytes = [
        size.to_be_bytes().as_slice(),
        &[0; 4],
        &[2, 0x10, 0, 0],
        &class,
        &space,
        &pcs,
        &[0; 12],
    ]
    .concat();
    bytes.extend(b"acsp");
    bytes.extend([0; 28]);
    bytes.extend(D50.map(i32::to_be_bytes).concat());
    bytes.extend([0; 48]);
    bytes.extend((u32::try_from(tags.len()).unwrap_or(u32::MAX)).to_be_bytes());
    bytes.extend(table);
    bytes.extend(data);
    bytes
}

/// A tone curve of one gamma: `curv` with one entry, the exponent as u8Fixed8.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "an exponent is below 256"
)]
fn gamma(exponent: f64) -> Vec<u8> {
    let fixed = ((exponent * 256.0).round() as u16).to_be_bytes();
    [b"curv".as_slice(), &[0; 4], &1_u32.to_be_bytes(), &fixed].concat()
}

/// A greyscale profile whose levels are linear light (tone curve gamma 1.0).
pub(crate) fn linear_grey() -> Vec<u8> {
    profile(*b"mntr", *b"GRAY", *b"XYZ ", &[(b"kTRC", gamma(1.0))])
}

/// An RGB matrix profile of three primaries (D50 XYZ of each, as ICC colorants) and one gamma for all three curves.
pub(crate) fn matrix_rgb(primaries: [[f64; 3]; 3], exponent: f64) -> Vec<u8> {
    let xyz = |colorant: [f64; 3]| [b"XYZ ".as_slice(), &[0; 4], &colorant.map(s15).concat()].concat();
    profile(
        *b"mntr",
        *b"RGB ",
        *b"XYZ ",
        &[
            (b"rXYZ", xyz(primaries[0])),
            (b"gXYZ", xyz(primaries[1])),
            (b"bXYZ", xyz(primaries[2])),
            (b"rTRC", gamma(exponent)),
            (b"gTRC", gamma(exponent)),
            (b"bTRC", gamma(exponent)),
        ],
    )
}

/// A CMYK profile whose lightness L* is 100 with no ink, 0 with any black, and `rest` / 255 * 100 otherwise, where
/// `rest` is given the share of cyan, magenta and yellow (0 or 1: the corners of the table) and is interpolated linearly
/// between them.
///
/// One 8-bit lookup table (lut8, 2 grid points per ink) from CMYK to CIELAB; a lut8 encodes L* as byte / 255 * 100
/// and a*, b* as byte - 128, so a and b are 0: every colour is neutral.
fn cmyk_profile(rest: impl Fn(bool, bool, bool) -> u8) -> Vec<u8> {
    let ramp: Vec<u8> = (0..=255).collect();
    let mut grid = Vec::new();
    for cyan in [false, true] {
        for magenta in [false, true] {
            for yellow in [false, true] {
                for black in [false, true] {
                    let lightness = if black { 0 } else { rest(cyan, magenta, yellow) };
                    grid.extend([lightness, 128, 128]);
                }
            }
        }
    }
    let identity: Vec<u8> = [1., 0., 0., 0., 1., 0., 0., 0., 1.].into_iter().flat_map(s15).collect();
    let lut = [
        b"mft1".as_slice(),
        &[0; 4],
        &[4, 3, 2, 0],
        &identity,
        &ramp.repeat(4),
        &grid,
        &ramp.repeat(3),
    ]
    .concat();
    profile(*b"prtr", *b"CMYK", *b"Lab ", &[(b"A2B0", lut)])
}

/// A CMYK profile: no ink is white (L* 100), any K is black (L* 0), any other ink is neutral grey L* 128/255*100.
pub(crate) fn cmyk() -> Vec<u8> {
    cmyk_profile(|cyan, magenta, yellow| if cyan || magenta || yellow { 128 } else { 255 })
}

/// A CMYK profile whose lightness falls with the cyan alone: L* 100 with none, 0 with all of it (magenta and yellow
/// make no difference), and any black is black.
pub(crate) fn cmyk_by_cyan() -> Vec<u8> {
    cmyk_profile(|cyan, _, _| if cyan { 0 } else { 255 })
}

/// The PNG `iCCP` chunk holding `profile`.
pub(crate) fn iccp(profile: &[u8]) -> Vec<u8> {
    chunk("iCCP", &[b"profile\0\0".as_slice(), &zlib(profile)].concat())
}

/// A PNG of 8-bit samples carrying `profile`: `colour_type` 0 grey, 2 RGB, 4 grey with alpha, 6 RGBA.
pub(crate) fn png(width: u32, colour_type: u8, row: &[u8], profile: &[u8]) -> Vec<u8> {
    Png::new(width, 1, 8, colour_type, &[row]).before(iccp(profile)).bytes()
}

/// A little-endian TIFF of 8-bit samples, `width` x 1, in one strip, carrying `profile`: `photometric` 1 is grey, 2 RGB,
/// 5 CMYK, and `row` holds the samples of each pixel in turn, as many as the model has.
pub(crate) fn tiff(width: u32, photometric: u16, row: &[u8], profile: &[u8]) -> Vec<u8> {
    let samples = match photometric {
        1 => 1_u16,
        2 => 3,
        _ => 4,
    };
    let entry = |tag: u16, kind: u16, count: u32, value: u32| {
        [
            tag.to_le_bytes().as_slice(),
            &kind.to_le_bytes(),
            &count.to_le_bytes(),
            &value.to_le_bytes(),
        ]
        .concat()
    };
    let (short, long, undefined) = (3, 4, 7);
    let length = |bytes: &[u8]| u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    let bits_at = 8 + 2 + 9 * 12 + 4; // after the directory: the bits of each sample (when there are several), ...
    let bits_length = if samples == 1 { 0 } else { 2 * u32::from(samples) };
    let profile_at = bits_at + bits_length; // ... the profile, ...
    let strip_at = profile_at + length(profile) + length(profile) % 2; // ... the pixels
    let bits = if samples == 1 {
        entry(258, short, 1, 8)
    } else {
        entry(258, short, u32::from(samples), bits_at)
    };
    let entries = [
        entry(256, long, 1, width),
        entry(257, long, 1, 1),
        bits,
        entry(262, short, 1, u32::from(photometric)),
        entry(273, long, 1, strip_at),
        entry(277, short, 1, u32::from(samples)),
        entry(278, long, 1, 1),
        entry(279, long, 1, length(row)),
        entry(34675, undefined, length(profile), profile_at),
    ];
    let mut file = vec![b'I', b'I', 42, 0, 8, 0, 0, 0];
    file.extend_from_slice(&9_u16.to_le_bytes());
    file.extend(entries.concat());
    file.extend_from_slice(&[0; 4]); // no next directory
    if samples > 1 {
        file.extend([8, 0].repeat(usize::from(samples)));
    }
    file.extend_from_slice(profile);
    file.extend(std::iter::repeat_n(0, profile.len() % 2));
    file.extend_from_slice(row);
    file
}

/// `jpeg` with `profile` in an `APP2` segment right after its start (the profile fits one segment).
pub(crate) fn with_profile(jpeg: &[u8], profile: &[u8]) -> Vec<u8> {
    let body = [b"ICC_PROFILE\0".as_slice(), &[1, 1], profile].concat();
    let length = u16::try_from(body.len() + 2).unwrap_or(u16::MAX);
    [&jpeg[..2], &[0xFF, 0xE2], &length.to_be_bytes(), &body, &jpeg[2..]].concat()
}
