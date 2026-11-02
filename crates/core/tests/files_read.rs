//! Reading image files (`read`, `read_map`), through the public API alone. PNG files are written byte by byte so each
//! level expected is worked out from the PNG standard by hand; the other formats are written by the `image` encoders, whose
//! output only has to be a file of that format. Tiny pictures, every channel of every pixel a different number, so a swap
//! of channels or of pixels shows.

mod common;

use std::io::Cursor;

use bullterrier_image::{Error, ErrorKind, Image, Layer, read, read_map};
use common::Outcome;
use common::files::{Png, Scratch, chunk, cmyk_jpeg, exif, progressive_jpeg, tiff_wide};
use image::codecs::gif::GifEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ExtendedColorType, ImageBuffer, ImageFormat};

/// How close a level is to the number worked out by hand.
const LEVEL: f32 = 1e-6;
/// How close after a JPEG's rounding: a few levels of 255.
const JPEG_LEVEL: f32 = 0.03;

/// One file and the levels it must read as: the size and then red, green, blue (and alpha, if the file has one) of each
/// pixel, row after row; the channels are what the values divide by.
struct Case {
    id: &'static str,
    file: Vec<u8>,
    size: (usize, usize),
    expected: Vec<f32>,
}

fn case(id: &'static str, file: Vec<u8>, size: (usize, usize), expected: &[f32]) -> Case {
    Case {
        id,
        file,
        size,
        expected: expected.to_vec(),
    }
}

fn assert_levels(id: &str, image: Image, size: (usize, usize), expected: &[f32], tolerance: f32) {
    let channels = expected.len() / (size.0 * size.1);
    assert_eq!(
        (image.height(), image.width(), image.channels()),
        (size.0, size.1, channels),
        "{id}"
    );
    let values = image.into_values();
    assert_eq!(values.len(), expected.len(), "{id}");
    for (index, (value, wanted)) in values.iter().zip(expected).enumerate() {
        assert!(
            (value - wanted).abs() <= tolerance,
            "{id}: value {index} is {value}, not {wanted}"
        );
    }
}

fn written(name: &str, bytes: &[u8]) -> std::io::Result<(Scratch, std::path::PathBuf)> {
    let folder = Scratch::new()?;
    let path = folder.write(name, bytes)?;
    Ok((folder, path))
}

/// Bytes of `picture` in `format`.
fn encoded(picture: &DynamicImage, format: ImageFormat) -> Result<Vec<u8>, image::ImageError> {
    let mut bytes = Cursor::new(Vec::new());
    picture.write_to(&mut bytes, format)?;
    Ok(bytes.into_inner())
}

fn rgb8(height: u32, width: u32, values: Vec<u8>) -> DynamicImage {
    DynamicImage::ImageRgb8(ImageBuffer::from_raw(width, height, values).unwrap_or_default())
}

fn be16(values: &[u16]) -> Vec<u8> {
    values.iter().flat_map(|value| value.to_be_bytes()).collect()
}

const RED_BLUE_GREY: f32 = 0.4; // the level 102 of 255

// ---- every pixel layout of a file reads as the levels a viewer shows ----------------------------------------------

#[expect(clippy::too_many_lines, reason = "a table of rows, one layout each")]
fn png_cases() -> Vec<Case> {
    let third = 5.0 / 15.0; // a 4-bit sample 5 of 15
    let key = |colour: &[u8]| chunk("tRNS", colour);
    vec![
        case(
            "8-bit grey is its level over 255 in all three channels",
            Png::new(4, 1, 8, 0, &[&[0, 51, 204, 255]]).bytes(),
            (1, 4),
            &[0., 0., 0., 0.2, 0.2, 0.2, 0.8, 0.8, 0.8, 1., 1., 1.],
        ),
        case(
            "1-bit grey is black or white",
            Png::new(2, 1, 1, 0, &[&[0b0100_0000]]).bytes(),
            (1, 2),
            &[0., 0., 0., 1., 1., 1.],
        ),
        case(
            "4-bit grey is its level over 15",
            Png::new(2, 1, 4, 0, &[&[0x5F]]).bytes(),
            (1, 2),
            &[third, third, third, 1., 1., 1.],
        ),
        case(
            "16-bit grey is its level over 65535",
            Png::new(3, 1, 16, 0, &[&be16(&[0, 16384, 65535])]).bytes(),
            (1, 3),
            &[
                0.,
                0.,
                0.,
                16384. / 65535.,
                16384. / 65535.,
                16384. / 65535.,
                1.,
                1.,
                1.,
            ],
        ),
        case(
            "grey with alpha keeps the alpha straight",
            Png::new(3, 1, 8, 4, &[&[10, 0, 100, 128, 255, 255]]).bytes(),
            (1, 3),
            &[
                10. / 255.,
                10. / 255.,
                10. / 255.,
                0.,
                100. / 255.,
                100. / 255.,
                100. / 255.,
                128. / 255.,
                1.,
                1.,
                1.,
                1.,
            ],
        ),
        case(
            "RGB keeps the order of its channels",
            Png::new(2, 1, 8, 2, &[&[255, 0, 0, 0, 102, 255]]).bytes(),
            (1, 2),
            &[1., 0., 0., 0., RED_BLUE_GREY, 1.],
        ),
        case(
            "RGBA is straight: the colour under alpha 0 is kept",
            Png::new(3, 1, 8, 6, &[&[0, 255, 0, 0, 255, 0, 0, 128, 0, 0, 255, 255]]).bytes(),
            (1, 3),
            &[0., 1., 0., 0., 1., 0., 0., 128. / 255., 0., 0., 1., 1.],
        ),
        case(
            "16-bit RGB is each channel over 65535",
            Png::new(1, 2, 16, 2, &[&be16(&[65535, 32768, 0]), &be16(&[1, 256, 65534])]).bytes(),
            (2, 1),
            &[1., 32768. / 65535., 0., 1. / 65535., 256. / 65535., 65534. / 65535.],
        ),
        case(
            "16-bit RGBA is each channel over 65535",
            Png::new(1, 1, 16, 6, &[&be16(&[65535, 0, 32768, 16384])]).bytes(),
            (1, 1),
            &[1., 0., 32768. / 65535., 16384. / 65535.],
        ),
        case(
            "a palette gives the colour of each index",
            Png::new(2, 1, 8, 3, &[&[1, 0]])
                .before(chunk("PLTE", &[255, 0, 0, 0, 0, 255]))
                .bytes(),
            (1, 2),
            &[0., 0., 1., 1., 0., 0.],
        ),
        case(
            "a palette with transparency gives alpha, opaque for an index it does not list",
            Png::new(3, 1, 8, 3, &[&[0, 1, 2]])
                .before(chunk("PLTE", &[0, 255, 0, 255, 0, 0, 0, 0, 255]))
                .before(key(&[10, 128]))
                .bytes(),
            (1, 3),
            &[0., 1., 0., 10. / 255., 1., 0., 0., 128. / 255., 0., 0., 1., 1.],
        ),
        case(
            "a grey colour key makes the pixels of that level transparent",
            Png::new(2, 1, 8, 0, &[&[0, 102]]).before(key(&[0, 102])).bytes(),
            (1, 2),
            &[0., 0., 0., 1., RED_BLUE_GREY, RED_BLUE_GREY, RED_BLUE_GREY, 0.],
        ),
        case(
            "an RGB colour key makes the pixels of that colour transparent",
            Png::new(2, 1, 8, 2, &[&[0, 255, 0, 255, 0, 0]])
                .before(key(&[0, 0, 0, 255, 0, 0]))
                .bytes(),
            (1, 2),
            &[0., 1., 0., 0., 1., 0., 0., 1.],
        ),
    ]
}

#[test]
fn every_png_layout_reads_as_the_levels_worked_out_by_hand() -> Outcome {
    for Case {
        id,
        file,
        size,
        expected,
    } in png_cases()
    {
        let (_folder, path) = written("layout.png", &file)?;
        assert_levels(id, read(&path)?, size, &expected, LEVEL);
    }
    Ok(())
}

fn other_format_cases() -> Result<Vec<Case>, image::ImageError> {
    let pixels = || rgb8(1, 2, vec![255, 0, 0, 0, 102, 255]);
    let (red, blue) = ([255, 0, 0, 255], [0, 0, 255, 255]);
    let mut gif = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut gif);
        for colour in [red, blue] {
            let frame = ImageBuffer::from_fn(4, 3, |_, _| image::Rgba(colour));
            encoder.encode_frame(image::Frame::new(frame))?;
        }
    }
    let mut clear_gif = Vec::new();
    {
        let frame = ImageBuffer::from_fn(2, 1, |column, _| {
            image::Rgba(if column == 0 { red } else { [0, 0, 0, 0] })
        });
        GifEncoder::new(&mut clear_gif).encode_frame(image::Frame::new(frame))?;
    }
    let sixteen = DynamicImage::ImageLuma16(ImageBuffer::from_raw(3, 1, vec![0, 16384, 65535]).unwrap_or_default());
    Ok(vec![
        case(
            "BMP",
            encoded(&pixels(), ImageFormat::Bmp)?,
            (1, 2),
            &[1., 0., 0., 0., RED_BLUE_GREY, 1.],
        ),
        case(
            "lossless WebP",
            encoded(&pixels(), ImageFormat::WebP)?,
            (1, 2),
            &[1., 0., 0., 0., RED_BLUE_GREY, 1.],
        ),
        case(
            "TIFF",
            encoded(&pixels(), ImageFormat::Tiff)?,
            (1, 2),
            &[1., 0., 0., 0., RED_BLUE_GREY, 1.],
        ),
        case(
            "16-bit grey TIFF is its level over 65535",
            encoded(&sixteen, ImageFormat::Tiff)?,
            (1, 3),
            &[
                0.,
                0.,
                0.,
                16384. / 65535.,
                16384. / 65535.,
                16384. / 65535.,
                1.,
                1.,
                1.,
            ],
        ),
        case(
            "an animated GIF gives its first frame",
            gif,
            (3, 4),
            &[1., 0., 0., 1.].repeat(12),
        ),
        case(
            "a GIF with a transparent index gives alpha 0 there",
            clear_gif,
            (1, 2),
            &[1., 0., 0., 1., 0., 0., 0., 0.],
        ),
    ])
}

#[test]
fn formats_beside_png_read_as_the_levels_of_the_pixels_written() -> Outcome {
    for Case {
        id,
        file,
        size,
        expected,
    } in other_format_cases()?
    {
        let (_folder, path) = written("layout.bin", &file)?;
        assert_levels(id, read(&path)?, size, &expected, LEVEL);
    }
    Ok(())
}

#[test]
fn the_format_is_found_by_content_not_by_extension() -> Outcome {
    let (_folder, path) = written("picture.jpg", &Png::new(1, 1, 8, 2, &[&[255, 0, 0]]).bytes())?;
    assert_levels("a PNG named .jpg", read(&path)?, (1, 1), &[1., 0., 0.], LEVEL);
    Ok(())
}

#[test]
fn jpegs_read_within_their_rounding_whatever_their_kind() -> Outcome {
    // two flat halves, 8 pixels wide each, left (200, 100, 50) and right (30, 160, 220)
    let flat = |left: [u8; 3], right: [u8; 3]| -> Vec<u8> {
        (0..8_u8)
            .flat_map(|_| (0..16_u8).flat_map(move |column| if column < 8 { left } else { right }))
            .collect()
    };
    let colour = {
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut bytes, 100).encode(
            &flat([200, 100, 50], [30, 160, 220]),
            16,
            8,
            ExtendedColorType::Rgb8,
        )?;
        bytes
    };
    let grey = {
        let levels: Vec<u8> = (0..8_u8)
            .flat_map(|_| (0..16_u8).map(|column| if column < 8 { 60 } else { 200 }))
            .collect();
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut bytes, 100).encode(&levels, 16, 8, ExtendedColorType::L8)?;
        bytes
    };
    let (_folder, path) = written("a.jpg", &colour)?;
    let image = read(&path)?;
    assert_eq!((image.height(), image.width(), image.channels()), (8, 16, 3));
    let values = image.into_values();
    let at = |column: usize| &values[(3 * 16 + column) * 3..][..3];
    for (found, wanted) in at(3).iter().zip([200., 100., 50.]) {
        assert!(
            (found - wanted / 255.).abs() <= JPEG_LEVEL,
            "left half: {found} not {}",
            wanted / 255.
        );
    }
    for (found, wanted) in at(12).iter().zip([30., 160., 220.]) {
        assert!(
            (found - wanted / 255.).abs() <= JPEG_LEVEL,
            "right half: {found} not {}",
            wanted / 255.
        );
    }
    // greyscale JPEG: three equal channels
    let (_folder, path) = written("grey.jpg", &grey)?;
    let values = read(&path)?.into_values();
    assert!((values[3 * 16 * 3] - 60. / 255.).abs() <= JPEG_LEVEL);
    assert_eq!(values[3 * 16 * 3], values[3 * 16 * 3 + 2]);
    // progressive: one colour
    let (_folder, path) = written("prog.jpg", &progressive_jpeg())?;
    assert_levels(
        "progressive JPEG",
        read(&path)?,
        (8, 8),
        &[200. / 255., 100. / 255., 50. / 255.].repeat(64),
        JPEG_LEVEL,
    );
    // CMYK without a profile: red from cyan 0, magenta 255, yellow 255, black 0
    let (_folder, path) = written("cmyk.jpg", &cmyk_jpeg())?;
    assert_levels("CMYK JPEG", read(&path)?, (8, 8), &[1., 0., 0.].repeat(64), JPEG_LEVEL);
    Ok(())
}

// ---- orientation ----------------------------------------------------------------------------------------------------

/// The upright picture, 2 rows of 3 pixels, each its own colour.
const UPRIGHT: [[[u8; 3]; 3]; 2] = [
    [[255, 0, 0], [0, 255, 0], [0, 0, 255]],
    [[10, 20, 30], [40, 50, 60], [70, 80, 90]],
];

/// The picture to store so that a viewer applying EXIF orientation `orientation` (1..=8, the table of the EXIF standard:
/// what the stored first row and first column are on the display) shows `UPRIGHT`, as rows of pixels.
fn stored(orientation: u16) -> Vec<Vec<[u8; 3]>> {
    let (rows, columns) = (UPRIGHT.len(), UPRIGHT[0].len());
    let (height, width) = if orientation <= 4 {
        (rows, columns)
    } else {
        (columns, rows)
    };
    (0..height)
        .map(|i| {
            (0..width)
                .map(|j| match orientation {
                    1 => UPRIGHT[i][j],
                    2 => UPRIGHT[i][columns - 1 - j],
                    3 => UPRIGHT[rows - 1 - i][columns - 1 - j],
                    4 => UPRIGHT[rows - 1 - i][j],
                    5 => UPRIGHT[j][i],
                    6 => UPRIGHT[j][columns - 1 - i],
                    7 => UPRIGHT[rows - 1 - j][columns - 1 - i],
                    _ => UPRIGHT[rows - 1 - j][i],
                })
                .collect()
        })
        .collect()
}

fn upright_values() -> Vec<f32> {
    UPRIGHT
        .iter()
        .flatten()
        .flatten()
        .map(|&level| f32::from(level) / 255.)
        .collect()
}

#[test]
fn every_exif_orientation_reads_as_the_same_upright_picture() -> Outcome {
    for orientation in 1..=8 {
        let rows: Vec<Vec<u8>> = stored(orientation).iter().map(|row| row.concat()).collect();
        let rows: Vec<&[u8]> = rows.iter().map(Vec::as_slice).collect();
        let (height, width) = (u32::try_from(rows.len())?, u32::try_from(rows[0].len() / 3)?);
        let file = Png::new(width, height, 8, 2, &rows)
            .before(chunk("eXIf", &exif(orientation)))
            .bytes();
        let (_folder, path) = written("turned.png", &file)?;
        assert_levels(
            &format!("orientation {orientation}"),
            read(&path)?,
            (2, 3),
            &upright_values(),
            LEVEL,
        );
    }
    Ok(())
}

#[test]
fn a_map_is_upright_too_so_it_lines_up_with_its_photo() -> Outcome {
    // orientation 6 (the first column is the top): stored 3 rows of 2 grey levels, displayed 2 rows of 3
    let file = Png::new(2, 3, 8, 0, &[&[0, 30], &[60, 90], &[120, 150]])
        .before(chunk("eXIf", &exif(6)))
        .bytes();
    let (_folder, path) = written("turned.png", &file)?;
    let map = read_map(&path)?;
    assert_eq!((map.height(), map.width()), (2, 3));
    // displayed(r, c) = stored[2 - c][r]
    let wanted = [120., 60., 0., 150., 90., 30.].map(|level| level / 255.);
    assert_close(&map, &wanted, LEVEL);
    Ok(())
}

#[test]
fn an_orientation_in_a_jpeg_is_applied_too() -> Outcome {
    // 3 x 2 blocks of 16 pixels, orientation 6 stored as 2 x 3 blocks; the upright block colours are well apart
    let blocks = stored(6);
    let (height, width) = (blocks.len() * 16, blocks[0].len() * 16);
    let pixels: Vec<u8> = (0..height)
        .flat_map(|row| {
            (0..width)
                .flat_map(|column| blocks[row / 16][column / 16])
                .collect::<Vec<u8>>()
        })
        .collect();
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 100).encode(
        &pixels,
        u32::try_from(width)?,
        u32::try_from(height)?,
        ExtendedColorType::Rgb8,
    )?;
    let tiff = exif(6);
    let mut segment = vec![0xFF, 0xE1];
    segment.extend_from_slice(&u16::try_from(2 + 6 + tiff.len())?.to_be_bytes());
    segment.extend_from_slice(b"Exif\0\0");
    segment.extend_from_slice(&tiff);
    jpeg.splice(2..2, segment); // right after the start-of-image marker
    let (_folder, path) = written("turned.jpg", &jpeg)?;
    let image = read(&path)?;
    assert_eq!((image.height(), image.width()), (32, 48));
    let values = image.into_values();
    for (block, wanted) in upright_values().chunks(3).enumerate() {
        let (row, column) = (block / 3, block % 3);
        let centre = (row * 16 + 8) * 48 + column * 16 + 8;
        for (found, wanted) in values[centre * 3..][..3].iter().zip(wanted) {
            assert!(
                (found - wanted).abs() <= JPEG_LEVEL,
                "block {block}: {found} not {wanted}"
            );
        }
    }
    Ok(())
}

// ---- files that cannot be read --------------------------------------------------------------------------------------

#[test]
fn a_file_that_cannot_be_read_as_an_image_is_an_image_read_error_naming_it() -> Outcome {
    let folder = Scratch::new()?;
    let whole = Png::new(3, 1, 8, 2, &[&[1, 2, 3, 4, 5, 6, 7, 8, 9]]).bytes();
    let paths: Vec<(&str, std::path::PathBuf)> = vec![
        ("missing", folder.path("broken-missing.png")),
        ("not an image", folder.write("broken-text.png", "just some text")?),
        ("empty", folder.write("broken-empty.png", "")?),
        (
            "truncated",
            folder.write("broken-truncated.png", &whole[..whole.len() - 20])?,
        ),
        ("a folder", {
            let path = folder.path("broken-folder.png");
            std::fs::create_dir(&path)?;
            path
        }),
    ];
    // known good: the same picture, whole
    assert!(read(&folder.write("whole.png", &whole)?).is_ok());
    for (id, path) in &paths {
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        for (reading, result) in [("read", read(path).map(drop)), ("read_map", read_map(path).map(drop))] {
            let error = result.err();
            assert_eq!(
                error.as_ref().map(Error::kind),
                Some(ErrorKind::ImageRead),
                "{id}, {reading}: {error:?}"
            );
            assert!(
                error.is_some_and(|error| error.to_string().contains(name)),
                "{id}, {reading}"
            );
        }
    }
    Ok(())
}

/// A 32 x 32 picture of pseudo-random levels (xorshift, a fixed seed): detail everywhere, so every format needs all its
/// bytes.
fn noisy(channels: usize) -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    (0..32 * 32 * channels)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            u8::try_from(state % 256).unwrap_or(0)
        })
        .collect()
}

/// The 32 x 32 `noisy` picture written in `format`, as grey, RGB or (where the format has none, RGB) the colour type.
fn noisy_file(format: ImageFormat, channels: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let picture = if channels == 1 {
        DynamicImage::ImageLuma8(ImageBuffer::from_raw(32, 32, noisy(1)).ok_or("size")?)
    } else {
        DynamicImage::ImageRgb8(ImageBuffer::from_raw(32, 32, noisy(3)).ok_or("size")?)
    };
    let mut bytes = Cursor::new(Vec::new());
    picture.write_to(&mut bytes, format)?;
    Ok(bytes.into_inner())
}

/// A whole file and what it is.
type Whole = (&'static str, Vec<u8>);

/// Whole files of every format the core reads, as `(id, bytes)`: baseline and progressive JPEG, grey and colour and CMYK.
fn whole_files() -> Result<Vec<Whole>, Box<dyn std::error::Error>> {
    Ok(vec![
        ("baseline RGB JPEG", noisy_file(ImageFormat::Jpeg, 3)?),
        ("baseline grey JPEG", noisy_file(ImageFormat::Jpeg, 1)?),
        ("progressive RGB JPEG", progressive_jpeg()),
        ("CMYK JPEG", cmyk_jpeg()),
        ("PNG", noisy_file(ImageFormat::Png, 3)?),
        ("TIFF", noisy_file(ImageFormat::Tiff, 3)?),
        ("lossless WebP", noisy_file(ImageFormat::WebP, 3)?),
        ("GIF", noisy_file(ImageFormat::Gif, 3)?),
        ("BMP", noisy_file(ImageFormat::Bmp, 3)?),
    ])
}

#[test]
fn a_file_cut_short_is_refused_whatever_its_format_and_however_much_is_left() -> Outcome {
    let folder = Scratch::new()?;
    for (id, whole) in whole_files()? {
        let name = id.replace(' ', "-");
        assert!(
            read(&folder.write(&format!("{name}-whole"), &whole)?).is_ok(),
            "{id}: whole"
        );
        for percent in [30_usize, 60, 90, 98] {
            let path = folder.write(&format!("{name}-{percent}"), &whole[..whole.len() * percent / 100])?;
            for (reading, result) in [("read", read(&path).map(drop)), ("read_map", read_map(&path).map(drop))] {
                let error = result.err();
                assert_eq!(
                    error.as_ref().map(Error::kind),
                    Some(ErrorKind::ImageRead),
                    "{id} cut at {percent} %, {reading}: {error:?}"
                );
                assert!(
                    error.is_some_and(|error| error.to_string().contains(&format!("{name}-{percent}"))),
                    "{id} cut at {percent} %, {reading}: the error does not name the path"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn a_jpeg_with_bytes_after_its_end_is_still_read_and_so_is_one_with_a_thumbnail_inside() -> Outcome {
    let folder = Scratch::new()?;
    let whole = noisy_file(ImageFormat::Jpeg, 3)?;
    let with_garbage = [whole.as_slice(), b"trailing bytes, not a picture \xFF\x00\xFF"].concat();
    // an APP1 segment holding a whole JPEG of its own (an EXIF thumbnail): its end marker is not the file's
    let thumbnail = noisy_file(ImageFormat::Jpeg, 1)?;
    let mut segment = vec![0xFF, 0xE1];
    segment.extend_from_slice(&u16::try_from(2 + thumbnail.len())?.to_be_bytes());
    segment.extend_from_slice(&thumbnail);
    let mut with_thumbnail = whole.clone();
    with_thumbnail.splice(2..2, segment);
    let expected = read(&folder.write("whole.jpg", &whole)?)?.into_values();
    for (id, file) in [("trailing bytes", with_garbage), ("an inner JPEG", with_thumbnail)] {
        let got = read(&folder.write("other.jpg", &file)?).map_err(|error| format!("{id}: {error}"))?;
        assert!(got.into_values() == expected, "{id}: the picture changed");
    }
    // cut inside the inner thumbnail's end marker, the outer picture is still unfinished
    let cut = [&with_inner_cut(&whole, &thumbnail)?[..]].concat();
    let path = folder.write("cut.jpg", &cut)?;
    assert_eq!(read(&path).err().map(|error| error.kind()), Some(ErrorKind::ImageRead));
    Ok(())
}

/// `whole` with `thumbnail` in an APP1 segment, cut right after the segment: the thumbnail's own end marker is the
/// last one in the file.
fn with_inner_cut(whole: &[u8], thumbnail: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut segment = vec![0xFF, 0xE1];
    segment.extend_from_slice(&u16::try_from(2 + thumbnail.len())?.to_be_bytes());
    segment.extend_from_slice(thumbnail);
    let mut file = whole[..2].to_vec();
    file.extend(segment);
    file.extend_from_slice(&whole[2..whole.len() / 2]);
    Ok(file)
}

/// TIFF files of 32-bit levels, `(id, file, size, expected levels over 3 channels' worth of grey)`.
fn wide_tiff_cases() -> Vec<Case> {
    let ints = |values: &[i32]| values.iter().map(|value| value.to_le_bytes()).collect::<Vec<_>>();
    let floats = |values: &[f32]| values.iter().map(|value| value.to_le_bytes()).collect::<Vec<_>>();
    let grey = |levels: &[f32]| levels.iter().flat_map(|&level| [level; 3]).collect::<Vec<_>>();
    vec![
        Case {
            id: "32-bit integers are read as 16-bit grey is: the level over 65535",
            file: tiff_wide(3, 1, 2, None, &ints(&[0, 32768, 65535])),
            size: (1, 3),
            expected: grey(&[0., 32768. / 65535., 1.]),
        },
        Case {
            id: "32-bit integers beyond 0..65535 are clipped",
            file: tiff_wide(3, 1, 2, None, &ints(&[-5, 70000, 26214])),
            size: (1, 3),
            expected: grey(&[0., 1., 26214. / 65535.]),
        },
        Case {
            id: "32-bit unsigned integers are levels over 65535 too",
            file: tiff_wide(3, 1, 1, None, &[0_u32, 16384, 4_000_000_000].map(u32::to_le_bytes)),
            size: (1, 3),
            expected: grey(&[0., 16384. / 65535., 1.]),
        },
        Case {
            id: "floats are their level, clipped to 0..1",
            file: tiff_wide(3, 1, 3, None, &floats(&[0.25, 1.5, -0.5])),
            size: (1, 3),
            expected: grey(&[0.25, 1., 0.]),
        },
        Case {
            id: "a TIFF of floats is upright by its orientation (6: the first column is the top)",
            file: tiff_wide(2, 3, 3, Some(6), &floats(&[0., 0.1, 0.2, 0.3, 0.4, 0.5])),
            size: (2, 3),
            expected: grey(&[0.4, 0.2, 0., 0.5, 0.3, 0.1]),
        },
    ]
}

#[test]
fn tiffs_of_32_bit_levels_read_as_the_levels_worked_out_by_hand() -> Outcome {
    for Case {
        id,
        file,
        size,
        expected,
    } in wide_tiff_cases()
    {
        let (_folder, path) = written("levels.tif", &file)?;
        assert_levels(id, read(&path)?, size, &expected, LEVEL);
    }
    Ok(())
}

#[test]
fn a_map_of_32_bit_integer_levels_within_16_bits_reads_as_16_bit_grey_does() -> Outcome {
    let samples = [0_i32, 26214].map(i32::to_le_bytes);
    let (_folder, path) = written("map.tif", &tiff_wide(2, 1, 2, None, &samples))?;
    assert_close(&read_map(&path)?, &[0., 0.4], LEVEL);
    Ok(())
}

#[test]
fn a_map_of_float_levels_or_of_integers_beyond_16_bits_is_refused_naming_the_mode() -> Outcome {
    let rows = [
        (
            "floats",
            tiff_wide(2, 1, 3, None, &[0.0_f32, 0.4].map(f32::to_le_bytes)),
            "'F'",
        ),
        (
            "one past 65535",
            tiff_wide(2, 1, 2, None, &[0_i32, 65536].map(i32::to_le_bytes)),
            "'I'",
        ),
        (
            "above 65535",
            tiff_wide(2, 1, 2, None, &[0_i32, 70000].map(i32::to_le_bytes)),
            "'I'",
        ),
        (
            "negative",
            tiff_wide(2, 1, 2, None, &[-5_i32, 100].map(i32::to_le_bytes)),
            "'I'",
        ),
    ];
    for (id, file, mode) in rows {
        let (_folder, path) = written("map.tif", &file)?;
        let message = read_map(&path).err().map(|error| error.to_string()).unwrap_or_default();
        assert!(message.contains("map.tif") && message.contains(mode), "{id}: {message}");
        assert!(read(&path).is_ok(), "{id}: read clips instead");
    }
    Ok(())
}

#[test]
fn a_tiff_of_32_bit_levels_claiming_too_many_pixels_is_refused_before_it_is_read() -> Outcome {
    let (_folder, path) = written(
        "bomb.tif",
        &tiff_wide(100_000, 100_000, 3, None, &[0.5_f32.to_le_bytes()]),
    )?;
    let message = read(&path).err().map(|error| error.to_string()).unwrap_or_default();
    assert!(
        message.contains("bomb.tif") && message.contains("more than the 178956970"),
        "{message}"
    );
    Ok(())
}

#[test]
fn a_palette_tiff_is_refused_naming_the_path() -> Outcome {
    let (_folder, path) = written("palette.tif", &common::files::tiff_palette())?;
    for result in [read(&path).map(drop), read_map(&path).map(drop)] {
        assert!(
            matches!(&result, Err(error) if error.kind() == ErrorKind::ImageRead && error.to_string().contains("palette.tif")),
            "{result:?}"
        );
    }
    Ok(())
}

#[test]
fn the_same_file_read_twice_is_the_same_image() -> Outcome {
    let (_folder, path) = written("twice.png", &png_cases().remove(4).file)?;
    assert_eq!(read(&path)?, read(&path)?);
    assert_eq!(read_map(&path)?, read_map(&path)?);
    Ok(())
}

// ---- maps -----------------------------------------------------------------------------------------------------------

fn assert_close(map: &Layer, expected: &[f32], tolerance: f32) {
    let values = map.values();
    assert_eq!(values.len(), expected.len());
    for (index, (value, wanted)) in values.iter().zip(expected).enumerate() {
        assert!(
            (value - wanted).abs() <= tolerance,
            "value {index} is {value}, not {wanted}"
        );
    }
}

/// Files and the map each must read as (one value per pixel), with the width of the file.
fn map_cases() -> Vec<(&'static str, Vec<u8>, usize, Vec<f32>)> {
    vec![
        (
            "a file with transparency gives its alpha whatever its colours",
            Png::new(2, 1, 8, 6, &[&[200, 200, 200, 51, 200, 200, 200, 255]]).bytes(),
            2,
            vec![0.2, 1.0],
        ),
        (
            "a drawing's silhouette over colours of brightness 0.8 gives its alpha",
            Png::new(2, 1, 8, 6, &[&[204, 204, 204, 0, 204, 204, 204, 255]]).bytes(),
            2,
            vec![0.0, 1.0],
        ),
        (
            "an alpha channel opaque everywhere gives the brightness",
            Png::new(2, 1, 8, 6, &[&[0, 0, 0, 255, 102, 102, 102, 255]]).bytes(),
            2,
            vec![0.0, 0.4],
        ),
        (
            "grey gives its levels over 255",
            Png::new(2, 1, 8, 0, &[&[0, 102]]).bytes(),
            2,
            vec![0.0, 0.4],
        ),
        (
            "16-bit grey keeps its range: the level over 65535",
            Png::new(2, 1, 16, 0, &[&be16(&[0, 16384])]).bytes(),
            2,
            vec![0.0, 16384. / 65535.],
        ),
        (
            "the same map in 16 bits reads as the same value",
            Png::new(2, 1, 16, 0, &[&be16(&[0, 102 * 257])]).bytes(),
            2,
            vec![0.0, 0.4],
        ),
        (
            "colour gives the luma 0.299 red + 0.587 green + 0.114 blue",
            Png::new(4, 1, 8, 2, &[&[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]]).bytes(),
            4,
            vec![0.299, 0.587, 0.114, 1.0],
        ),
        (
            "a transparent palette index gives alpha",
            Png::new(2, 1, 8, 3, &[&[0, 1]])
                .before(chunk("PLTE", &[255, 255, 255, 255, 255, 255]))
                .before(chunk("tRNS", &[0]))
                .bytes(),
            2,
            vec![0.0, 1.0],
        ),
        (
            "a colour key gives alpha",
            Png::new(2, 1, 8, 0, &[&[0, 102]])
                .before(chunk("tRNS", &[0, 102]))
                .bytes(),
            2,
            vec![1.0, 0.0],
        ),
        (
            "grey with alpha, a pixel clear, gives alpha",
            Png::new(2, 1, 8, 4, &[&[255, 51, 255, 255]]).bytes(),
            2,
            vec![0.2, 1.0],
        ),
    ]
}

#[test]
fn a_map_file_gives_its_value_per_pixel() -> Outcome {
    for (id, file, width, expected) in map_cases() {
        let (_folder, path) = written("map.png", &file)?;
        let map = read_map(&path)?;
        assert_eq!((map.height(), map.width()), (1, width), "{id}");
        assert_close(&map, &expected, LEVEL);
    }
    Ok(())
}

// ---- the limit on size ----------------------------------------------------------------------------------------------

#[test]
fn a_file_claiming_more_pixels_than_the_limit_is_refused_from_its_header_naming_the_path() -> Outcome {
    // 100000 x 100000 pixels would take 10 GB as 8-bit grey: it is refused, or the test is killed, long before
    let (_folder, path) = written("bomb.png", &common::files::png_claiming(100_000, 100_000))?;
    for result in [read(&path).map(drop), read_map(&path).map(drop)] {
        assert!(
            matches!(&result, Err(Error::ImageRead(message)) if message.contains("bomb.png") && message.contains("178956970")),
            "{result:?}"
        );
    }
    Ok(())
}
