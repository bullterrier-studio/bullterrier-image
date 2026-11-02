//! Writing image files (`write`, `write_with_quality`), through the public API alone: what is read back from the file,
//! and the bytes of the file itself where the standard says what they must be (the JPEG's quantisation tables, the
//! chunks and segments a file holds).

#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the pictures are tiny and their levels 0..1, so the casts of the test's own numbers are exact"
)]

mod common;

use std::io::Cursor;
use std::path::{Path, PathBuf};

use bullterrier_image::{Error, ErrorKind, Image, read, write, write_with_quality};
use common::Outcome;
use common::files::{Png, Scratch, chunk, exif_geotagged};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::webp::WebPEncoder;
use image::{ExtendedColorType, ImageDecoder, ImageEncoder, ImageReader};

/// How close a picture is after a JPEG's rounding at quality 95: a few levels of 255.
const JPEG_LEVEL: f32 = 0.03;
/// One level of 255, the most that writing a 16-bit level as 8 bits may move it.
const ONE_LEVEL: f32 = 1.0 / 255.0;
/// The file formats the core writes losslessly, by extension.
const LOSSLESS: [&str; 2] = ["png", "webp"];
/// Every extension the core writes.
const ALL: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

// ---- pictures and readers ---------------------------------------------------------------------------------------------

/// A `height` x `width` image whose every channel of every pixel is a whole 8-bit level, nearly all different.
fn levels(height: usize, width: usize) -> Result<Image, Error> {
    let values: Vec<f32> = (0..height * width * 3)
        .map(|index| f32::from(u8::try_from((index * 37 + 11) % 256).unwrap_or(0)) / 255.0)
        .collect();
    Image::new(height, width, &values)
}

/// The same with an alpha channel of whole levels, one pixel fully transparent over a colour.
fn levels_with_alpha(height: usize, width: usize) -> Result<Image, Error> {
    let values: Vec<f32> = (0..height * width * 4)
        .map(|index| f32::from(u8::try_from((index * 53 + 5) % 256).unwrap_or(0)) / 255.0)
        .enumerate()
        .map(|(index, level)| if index == 3 { 0.0 } else { level })
        .collect();
    Image::rgba(height, width, &values)
}

/// A smooth picture: red falls down the rows, green along the columns, blue along the diagonal.
fn gradient(height: usize, width: usize) -> Result<Image, Error> {
    let mut values = Vec::new();
    for row in 0..height {
        for column in 0..width {
            values.extend([
                1.0 - row as f32 / height as f32,
                column as f32 / width as f32,
                (row + column) as f32 / (height + width) as f32,
            ]);
        }
    }
    Image::new(height, width, &values)
}

/// A picture of pseudo-random levels (xorshift, a fixed seed): detail at every size, so quality changes the file.
fn noise(height: usize, width: usize) -> Result<Image, Error> {
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let values: Vec<f32> = (0..height * width * 3)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            f32::from(u8::try_from(state % 256).unwrap_or(0)) / 255.0
        })
        .collect();
    Image::new(height, width, &values)
}

/// The values of an image, for comparing two.
fn values(image: &Image) -> Vec<f32> {
    image.clone().into_values()
}

/// The largest difference between two pictures' values, and the mean of them; both of the same size.
fn error(a: &Image, b: &Image) -> (f32, f32) {
    let (a, b) = (values(a), values(b));
    assert_eq!(a.len(), b.len(), "the pictures differ in size");
    let differences: Vec<f32> = a.iter().zip(&b).map(|(a, b)| (a - b).abs()).collect();
    (
        differences.iter().copied().fold(0.0, f32::max),
        differences.iter().sum::<f32>() / differences.len() as f32,
    )
}

/// Asserts two pictures hold the same whole 8-bit levels, naming the first that differs (not printing the pictures).
fn assert_same(what: &str, wrote: &Image, read_back: &Image) {
    assert_eq!(
        (wrote.height(), wrote.width(), wrote.channels()),
        (read_back.height(), read_back.width(), read_back.channels()),
        "{what}: shape"
    );
    let (wrote, read_back) = (whole_levels(wrote), whole_levels(read_back));
    let first = wrote.iter().zip(&read_back).position(|(a, b)| a != b);
    assert!(
        first.is_none(),
        "{what}: value {first:?} is {:?}, not {:?}",
        first.map(|i| read_back[i]),
        first.map(|i| wrote[i])
    );
}

/// The file `name` of a new scratch folder (which goes with the folder), not yet written.
fn target(name: &str) -> std::io::Result<(Scratch, PathBuf)> {
    let folder = Scratch::new()?;
    let path = folder.path(name);
    Ok((folder, path))
}

/// The bytes of the file `name` after writing `image` to it.
fn written_bytes(image: &Image, name: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let (_folder, path) = target(name)?;
    write(&path, image)?;
    Ok(std::fs::read(path)?)
}

/// The embedded ICC profile of the image file at `path`, read by the `image` crate's decoder.
fn embedded_profile(path: &Path) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    Ok(ImageReader::open(path)?
        .with_guessed_format()?
        .into_decoder()?
        .icc_profile()?)
}

fn contains(bytes: &[u8], needle: &str) -> bool {
    contains_bytes(bytes, needle.as_bytes())
}

fn contains_bytes(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}

/// A picture's values as the whole 8-bit levels they are: a file's own numbers, which reading divides by 255 and a colour
/// profile moves by a small fraction of a level, so exactness is the level, not the float.
fn whole_levels(image: &Image) -> Vec<u8> {
    values(image)
        .iter()
        .map(|level| (level * 255.0).round() as u8)
        .collect()
}

// ---- the levels in the file ---------------------------------------------------------------------------------------------

#[test]
fn a_lossless_file_reads_back_the_exact_8_bit_levels_written() -> Outcome {
    for (extension, image) in [
        ("png", levels(7, 19)?),
        ("webp", levels(7, 19)?),
        ("png", levels_with_alpha(5, 6)?),
        ("webp", levels_with_alpha(5, 6)?),
        ("png", levels(1, 1)?),
        ("webp", levels(1, 1)?),
        ("png", levels(9, 1)?),
        ("webp", levels(1, 17)?),
    ] {
        let (_folder, path) = target(&format!("a.{extension}"))?;
        write(&path, &image)?;
        assert_same(
            &format!(
                "{extension} of {} x {} x {}",
                image.height(),
                image.width(),
                image.channels()
            ),
            &image,
            &read(&path)?,
        );
    }
    Ok(())
}

#[test]
fn a_level_is_cut_to_a_whole_number_after_half_a_level_is_added() -> Outcome {
    // 255 x level + 0.5, cut: worked out by hand
    let rows: [(f32, u8); 5] = [
        (0.0, 0),
        (0.4 / 255.0, 0), // 0.4 + 0.5 = 0.9
        (0.6 / 255.0, 1), // 0.6 + 0.5 = 1.1
        (0.5, 128),       // 127.5 + 0.5 = 128
        (1.0, 255),
    ];
    let image = Image::new(1, 5, &(0..15).map(|index| rows[index % 5].0).collect::<Vec<_>>())?;
    let (_folder, path) = target("levels.png")?;
    write(&path, &image)?;
    let read_back = whole_levels(&read(&path)?);
    for (index, level) in read_back.iter().enumerate() {
        assert_eq!(*level, rows[index % 5].1, "value {index}");
    }
    Ok(())
}

#[test]
fn a_16_bit_picture_written_and_read_again_is_within_one_level() -> Outcome {
    let rows = [
        be16(&[0, 1, 255, 256, 32767, 32768]),
        be16(&[65534, 65535, 12345, 54321, 40000, 700]),
    ];
    let file = Png::new(2, 2, 16, 2, &[&rows[0], &rows[1]]).bytes();
    let folder = Scratch::new()?;
    let original = read(&folder.write("deep.png", file)?)?;
    for extension in LOSSLESS {
        let path = folder.path(&format!("copy.{extension}"));
        write(&path, &original)?;
        let (largest, _) = error(&original, &read(&path)?);
        assert!(largest <= ONE_LEVEL, "{extension}: {largest}");
    }
    Ok(())
}

fn be16(samples: &[u16]) -> Vec<u8> {
    samples.iter().flat_map(|sample| sample.to_be_bytes()).collect()
}

#[test]
fn alpha_is_written_straight_and_the_colour_under_it_is_kept() -> Outcome {
    let image = levels_with_alpha(4, 4)?; // the first pixel is fully transparent over a colour
    assert!(values(&image)[3] == 0.0 && values(&image)[..3].iter().any(|&level| level > 0.0));
    for extension in LOSSLESS {
        let (_folder, path) = target(&format!("cut.{extension}"))?;
        write(&path, &image)?;
        let read_back = read(&path)?;
        assert_eq!(read_back.channels(), 4, "{extension}");
        assert_same(extension, &image, &read_back);
    }
    Ok(())
}

// ---- JPEG --------------------------------------------------------------------------------------------------------------

#[test]
fn a_jpeg_reads_back_within_a_few_levels_at_the_default_quality() -> Outcome {
    let image = gradient(37, 45)?; // sizes that are no multiple of the 8 x 8 blocks
    for extension in ["jpg", "jpeg"] {
        let (_folder, path) = target(&format!("a.{extension}"))?;
        write(&path, &image)?;
        let read_back = read(&path)?;
        assert_eq!(
            (read_back.height(), read_back.width(), read_back.channels()),
            (37, 45, 3)
        );
        let (largest, _) = error(&image, &read_back);
        assert!(largest <= JPEG_LEVEL, "{extension}: {largest}");
    }
    Ok(())
}

#[test]
fn a_jpeg_keeps_full_resolution_colour() -> Outcome {
    // Columns one pixel wide alternate red and blue: 4:2:0 or 4:2:2 would average each pair of columns into one purple.
    let mut stripes = vec![0.0; 16 * 16 * 3];
    for (index, pixel) in stripes.as_chunks_mut::<3>().0.iter_mut().enumerate() {
        pixel[if index % 2 == 0 { 0 } else { 2 }] = 1.0;
    }
    let image = Image::new(16, 16, &stripes)?;
    let (_folder, path) = target("stripes.jpg")?;
    write_with_quality(&path, &image, 100)?;
    let (largest, _) = error(&image, &read(&path)?);
    assert!(largest < 0.1, "{largest}");
    Ok(())
}

#[test]
fn a_lower_quality_makes_a_smaller_file_and_a_worse_picture() -> Outcome {
    let image = noise(48, 48)?;
    let folder = Scratch::new()?;
    let mut previous: Option<(u64, f32, u8)> = None;
    for quality in [5, 20, 40, 60, 80, 95, 100] {
        let path = folder.path("q.jpg");
        write_with_quality(&path, &image, quality)?;
        let (size, (_, mean)) = (std::fs::metadata(&path)?.len(), error(&image, &read(&path)?));
        if let Some((smaller, rougher, lower)) = previous {
            assert!(
                size > smaller,
                "quality {quality} is {size} bytes, quality {lower} {smaller}"
            );
            assert!(
                mean < rougher,
                "quality {quality} is off by {mean}, quality {lower} by {rougher}"
            );
        }
        previous = Some((size, mean, quality));
    }
    Ok(())
}

/// The base tables of ITU-T T.81 Annex K (K.1, K.2), row after row: the scaling of the IJG formula starts from them.
const LUMINANCE: [u8; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56, 14, 17, 22, 29, 51,
    87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113, 92, 49, 64, 78, 87, 103, 121, 120, 101,
    72, 92, 95, 98, 112, 100, 103, 99,
];
const CHROMINANCE: [u8; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99, 47, 66, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99,
];

/// The positions of a block in the order a JPEG stores them (the zigzag of T.81 Figure A.6): row * 8 + column.
fn zigzag() -> Vec<usize> {
    let mut order = Vec::new();
    for diagonal in 0..15_usize {
        let rows: Vec<usize> = (0..8).filter(|&row| row <= diagonal && diagonal - row < 8).collect();
        // even diagonals run up and to the right, odd ones down and to the left
        let rows: Vec<usize> = if diagonal % 2 == 0 {
            rows.into_iter().rev().collect()
        } else {
            rows
        };
        order.extend(rows.into_iter().map(|row| row * 8 + diagonal - row));
    }
    order
}

/// The table for `quality` by the IJG formula (libjpeg, `jpeg_quality_scaling`), in the order a file stores it.
fn ijg_table(base: &[u8; 64], quality: u32) -> Vec<u8> {
    let scale = if quality < 50 {
        5000 / quality
    } else {
        200 - 2 * quality
    };
    zigzag()
        .into_iter()
        .map(|place| u8::try_from(((u32::from(base[place]) * scale + 50) / 100).clamp(1, 255)).unwrap_or(255))
        .collect()
}

/// The segments of a JPEG file before its pixels: the marker byte after `FF` and the bytes after the length.
fn jpeg_segments(file: &[u8]) -> Vec<(u8, Vec<u8>)> {
    assert_eq!(file[..2], [0xFF, 0xD8], "not a JPEG");
    let (mut segments, mut at) = (Vec::new(), 2);
    while file[at] == 0xFF {
        let (marker, length) = (
            file[at + 1],
            usize::from(u16::from_be_bytes([file[at + 2], file[at + 3]])),
        );
        segments.push((marker, file[at + 4..at + 2 + length].to_vec()));
        if marker == 0xDA {
            break; // the scan: the pixels follow
        }
        at += 2 + length;
    }
    segments
}

#[test]
fn the_quantisation_tables_are_the_standard_ones_scaled_by_the_ijg_formula_for_the_quality() -> Outcome {
    let image = noise(16, 16)?;
    let folder = Scratch::new()?;
    for quality in [1_u8, 10, 25, 49, 50, 51, 75, 90, 95, 99, 100] {
        let path = folder.path("q.jpg");
        write_with_quality(&path, &image, quality)?;
        let file = std::fs::read(&path)?;
        let mut tables = Vec::new();
        for (_, payload) in jpeg_segments(&file).iter().filter(|(marker, _)| *marker == 0xDB) {
            // each table: a byte with precision (high nibble, 0 = 8 bits) and number, then 64 entries
            for table in payload.chunks(65) {
                assert_eq!(table[0] >> 4, 0, "quality {quality}: 8-bit tables");
                tables.push((table[0] & 0x0F, table[1..].to_vec()));
            }
        }
        tables.sort();
        assert_eq!(
            tables,
            [
                (0, ijg_table(&LUMINANCE, u32::from(quality))),
                (1, ijg_table(&CHROMINANCE, u32::from(quality)))
            ],
            "quality {quality}"
        );
    }
    Ok(())
}

#[test]
fn only_a_jpeg_has_a_quality() -> Outcome {
    let image = noise(16, 16)?;
    for extension in LOSSLESS {
        let (_folder, path) = target(&format!("a.{extension}"))?;
        write_with_quality(&path, &image, 1)?;
        let rough = std::fs::read(&path)?;
        write_with_quality(&path, &image, 100)?;
        assert!(
            rough == std::fs::read(&path)?,
            "{extension}: the quality changed the file"
        );
    }
    Ok(())
}

// ---- refusals ----------------------------------------------------------------------------------------------------------

#[test]
fn a_file_of_an_extension_the_core_does_not_write_is_refused_by_name_and_nothing_is_written() -> Outcome {
    let image = levels(2, 2)?;
    for (name, said) in [
        ("a.bmp", "'.bmp'"),
        ("a.TIFF", "'.tiff'"),
        ("a.gif", "'.gif'"),
        ("a.png.txt", "'.txt'"),
        ("noextension", "no extension"),
        (".png", "no extension"), // a name that is only an extension has none: it is a hidden file
    ] {
        let (folder, path) = target(name)?;
        let result = write(&path, &image);
        let message = result.as_ref().err().map(ToString::to_string).unwrap_or_default();
        assert_eq!(
            result.err().map(|error| error.kind()),
            Some(ErrorKind::BackendUnavailable),
            "{name}"
        );
        assert!(message.contains(said), "{name}: {message}");
        assert!(message.contains(".png"), "{name} should say what to use: {message}");
        assert!(
            !path.exists() && std::fs::read_dir(folder.path(""))?.next().is_none(),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn a_jpeg_cannot_hold_transparency_so_it_is_refused_and_nothing_is_written() -> Outcome {
    let image = levels_with_alpha(2, 2)?;
    for name in ["a.jpg", "a.JPEG"] {
        let (_folder, path) = target(name)?;
        let result = write(&path, &image);
        let message = result.as_ref().err().map(ToString::to_string).unwrap_or_default();
        assert_eq!(
            result.err().map(|error| error.kind()),
            Some(ErrorKind::BackendUnavailable),
            "{name}"
        );
        assert!(
            message.contains(&format!(".{}", name.rsplit('.').next().unwrap_or("").to_lowercase())),
            "{message}"
        );
        assert!(message.contains("png") && message.contains("webp"), "{message}");
        assert!(!path.exists(), "{name}");
    }
    Ok(())
}

#[test]
fn a_refusal_leaves_the_file_that_was_there() -> Outcome {
    let folder = Scratch::new()?;
    let path = folder.write("keep.jpg", b"the earlier file")?;
    assert!(write(&path, &levels_with_alpha(2, 2)?).is_err());
    assert_eq!(std::fs::read(&path)?, b"the earlier file");
    Ok(())
}

#[test]
fn a_quality_outside_1_to_100_is_refused_naming_it_for_every_format_and_nothing_is_written() -> Outcome {
    let image = levels(2, 2)?;
    for extension in ALL {
        for quality in [0_u8, 101, 255] {
            let (_folder, path) = target(&format!("a.{extension}"))?;
            let result = write_with_quality(&path, &image, quality);
            let message = result.as_ref().err().map(ToString::to_string).unwrap_or_default();
            assert!(
                message.contains(&quality.to_string()),
                "{extension} {quality}: {message}"
            );
            assert!(result.is_err() && !path.exists(), "{extension} {quality}");
        }
        for quality in [1_u8, 100] {
            let (_folder, path) = target(&format!("a.{extension}"))?;
            write_with_quality(&path, &image, quality)?;
            assert!(path.exists(), "{extension} {quality}");
        }
    }
    Ok(())
}

#[test]
fn a_path_that_cannot_be_created_is_an_image_write_error_naming_it() -> Outcome {
    let image = levels(2, 2)?;
    let folder = Scratch::new()?;
    let missing = folder.path("no-such-folder").join("a.png");
    let a_folder = folder.path("folder.png");
    std::fs::create_dir(&a_folder)?;
    for path in [missing, a_folder] {
        let result = write(&path, &image);
        let message = result.as_ref().err().map(ToString::to_string).unwrap_or_default();
        assert_eq!(
            result.err().map(|error| error.kind()),
            Some(ErrorKind::ImageWrite),
            "{path:?}"
        );
        assert!(message.contains(&path.display().to_string()), "{message}");
    }
    Ok(())
}

#[test]
fn a_picture_too_large_for_jpeg_is_an_image_write_error_naming_the_path_and_leaves_no_file() -> Outcome {
    let image = Image::new(1, 65536, &vec![0.5; 65536 * 3])?;
    let (_folder, path) = target("wide.jpg")?;
    let result = write(&path, &image);
    let message = result.as_ref().err().map(ToString::to_string).unwrap_or_default();
    assert_eq!(result.err().map(|error| error.kind()), Some(ErrorKind::ImageWrite));
    assert!(message.contains("wide.jpg"), "{message}");
    assert!(!path.exists());
    Ok(())
}

#[test]
fn a_file_is_replaced_and_the_extension_may_be_in_any_case() -> Outcome {
    let folder = Scratch::new()?;
    let path = folder.write("A.PnG", b"the earlier file")?;
    let image = levels(3, 3)?;
    write(&path, &image)?;
    assert_same("replaced", &image, &read(&path)?);
    Ok(())
}

// ---- metadata -----------------------------------------------------------------------------------------------------------

/// A file of each format, by extension.
type Geotagged = [(&'static str, Vec<u8>); 3];

/// The same geotagged 2 x 2 picture as a PNG, a JPEG and a WebP, each with the metadata its format holds: EXIF with the
/// maker and a GPS block, text, a time and an XMP packet where the format has them.
fn geotagged() -> Result<Geotagged, Box<dyn std::error::Error>> {
    let rows = [[200, 120, 40, 10, 20, 30], [90, 80, 70, 250, 5, 99]];
    let png = Png::new(2, 2, 8, 2, &[&rows[0], &rows[1]])
        .before(chunk("eXIf", &exif_geotagged()))
        .before(chunk("tEXt", b"Author\0PhoneMaker"))
        .before(chunk("tIME", &[7, 232, 1, 2, 3, 4, 5]))
        .before(chunk(
            "iTXt",
            b"XML:com.adobe.xmp\0\0\0\0\0<x:xmpmeta>GPS 51.5N</x:xmpmeta>",
        ))
        .bytes();
    let raw: Vec<u8> = rows.concat();
    let mut jpeg = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut jpeg, 95);
    encoder.set_exif_metadata(exif_geotagged())?;
    encoder.write_image(&raw, 2, 2, ExtendedColorType::Rgb8)?;
    let mut webp = Vec::new();
    let mut encoder = WebPEncoder::new_lossless(&mut webp);
    encoder.set_exif_metadata(exif_geotagged())?;
    encoder.write_image(&raw, 2, 2, ExtendedColorType::Rgb8)?;
    Ok([("png", png), ("jpg", jpeg), ("webp", webp)])
}

/// What an EXIF, GPS, text, time or XMP block of any of the three formats contains, as the bytes that spell it.
const METADATA: [&str; 9] = ["Exif", "EXIF", "GPS", "eXIf", "tEXt", "iTXt", "zTXt", "tIME", "XMP"];

/// The chunks of a PNG, in order, by type.
fn png_chunks(file: &[u8]) -> Vec<String> {
    let (mut types, mut at) = (Vec::new(), 8);
    while at < file.len() {
        let length = u32::from_be_bytes([file[at], file[at + 1], file[at + 2], file[at + 3]]) as usize;
        types.push(String::from_utf8_lossy(&file[at + 4..at + 8]).into_owned());
        at += 12 + length;
    }
    types
}

/// The chunks of a WebP (RIFF), in order, by four-character code.
fn webp_chunks(file: &[u8]) -> Vec<String> {
    let (mut codes, mut at) = (Vec::new(), 12);
    while at < file.len() {
        let length = u32::from_le_bytes([file[at + 4], file[at + 5], file[at + 6], file[at + 7]]) as usize;
        codes.push(String::from_utf8_lossy(&file[at..at + 4]).into_owned());
        at += 8 + length + length % 2;
    }
    codes
}

#[test]
fn a_picture_read_from_a_geotagged_file_is_written_with_no_exif_gps_text_time_or_xmp() -> Outcome {
    let folder = Scratch::new()?;
    for (extension, input) in geotagged()? {
        let own_marker = match extension {
            "png" => "eXIf",
            "jpg" => "Exif",
            _ => "EXIF",
        };
        assert!(
            contains(&input, own_marker) && contains(&input, "PhoneMaker"),
            "{extension}: the fixture is geotagged"
        );
        let from = folder.write(&format!("in.{extension}"), &input)?;
        let image = read(&from)?;
        for target in ALL {
            let to = folder.path(&format!("out.{target}"));
            write(&to, &image)?;
            let file = std::fs::read(&to)?;
            for marker in METADATA.iter().chain(&["PhoneMaker"]) {
                assert!(
                    !contains(&file, marker),
                    "{extension} -> {target}: the file holds {marker:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn a_file_holds_the_pixels_the_profile_and_the_format_s_own_framing_and_nothing_more() -> Outcome {
    let image = levels(9, 11)?;
    let png = png_chunks(&written_bytes(&image, "a.png")?);
    assert_eq!(png.first().map(String::as_str), Some("IHDR"));
    assert_eq!(png.last().map(String::as_str), Some("IEND"));
    assert!(
        png.iter()
            .all(|kind| ["IHDR", "iCCP", "IDAT", "IEND"].contains(&kind.as_str())),
        "{png:?}"
    );
    let webp = webp_chunks(&written_bytes(&image, "a.webp")?);
    assert_eq!(webp, ["VP8X", "ICCP", "VP8L"]);
    // JFIF is the JPEG's own framing (version and pixel shape, no date, place or camera)
    let jpeg = jpeg_segments(&written_bytes(&image, "a.jpg")?);
    let markers: Vec<u8> = jpeg.iter().map(|(marker, _)| *marker).collect();
    assert!(
        markers
            .iter()
            .all(|marker| [0xE0, 0xE2, 0xDB, 0xC0, 0xC4, 0xDA].contains(marker)),
        "{markers:02X?}"
    );
    assert_eq!(jpeg[0].1[..5], *b"JFIF\0");
    Ok(())
}

// ---- the profile ---------------------------------------------------------------------------------------------------------

#[test]
fn every_format_embeds_the_srgb_profile_with_its_fixed_date() -> Outcome {
    let expected = include_bytes!("../data/srgb.icc");
    // what the profile is, apart from the file it is read from: the sRGB profile littlecms builds, 588 bytes, dated 2000-01-01
    assert_eq!(expected.len(), 588);
    assert_eq!(expected[24..36], [0x07, 0xD0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&expected[36..40], b"acsp");
    assert_eq!(&expected[12..24], b"mntrRGB XYZ ");
    assert!(contains_bytes(expected, &[0, b's', 0, b'R', 0, b'G', 0, b'B'])); // its description, in UTF-16
    let image = levels(5, 6)?;
    for extension in ALL {
        let (_folder, path) = target(&format!("a.{extension}"))?;
        write(&path, &image)?;
        let profile = embedded_profile(&path)?;
        assert!(
            profile.as_deref() == Some(expected.as_slice()),
            "{extension}: {:?}",
            profile.map(|p| p.len())
        );
    }
    // and with alpha, where the format can hold it
    for extension in LOSSLESS {
        let (_folder, path) = target(&format!("cut.{extension}"))?;
        write(&path, &levels_with_alpha(3, 3)?)?;
        assert!(
            embedded_profile(&path)?.as_deref() == Some(expected.as_slice()),
            "{extension} with alpha"
        );
    }
    Ok(())
}

#[test]
fn the_same_image_gives_the_same_bytes_every_time_and_a_different_one_gives_others() -> Outcome {
    for extension in ALL {
        let image = noise(13, 21)?;
        let first = written_bytes(&image, &format!("a.{extension}"))?;
        assert!(
            first == written_bytes(&image, &format!("a.{extension}"))?,
            "{extension}"
        );
        assert!(
            first != written_bytes(&noise(13, 22)?, &format!("a.{extension}"))?,
            "{extension}"
        );
    }
    for extension in LOSSLESS {
        let image = levels_with_alpha(6, 7)?;
        assert!(
            written_bytes(&image, &format!("a.{extension}"))? == written_bytes(&image, &format!("a.{extension}"))?,
            "{extension} with alpha"
        );
    }
    Ok(())
}

#[test]
fn a_file_written_is_a_file_other_readers_open() -> Outcome {
    // The `image` crate's reader (no code of ours) opens each format and sees the picture's size.
    let image = levels(6, 8)?;
    for extension in ALL {
        let (_folder, path) = target(&format!("a.{extension}"))?;
        write(&path, &image)?;
        let opened = ImageReader::new(Cursor::new(std::fs::read(&path)?))
            .with_guessed_format()?
            .decode()?;
        assert_eq!((opened.height(), opened.width()), (6, 8), "{extension}");
    }
    Ok(())
}
