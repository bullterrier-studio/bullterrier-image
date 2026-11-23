//! Reading a picture or a map from bytes (`read_bytes`, `read_map_bytes`) gives what reading the same file by path gives,
//! through the public API alone. One code path decodes both, so the two are compared exactly, on tiny files.

use std::io::Cursor;
use std::path::Path;

use crate::common::Outcome;
use crate::common::files::{Png, Scratch, chunk, cmyk_jpeg, exif, tiff_wide};
use crate::common::icc::{cmyk, png, tiff, with_profile};
use bullterrier_image::{Error, ErrorKind, read, read_bytes, read_map, read_map_bytes};
use image::{DynamicImage, ImageBuffer, ImageFormat};

fn encoded(picture: &DynamicImage, format: ImageFormat) -> Result<Vec<u8>, image::ImageError> {
    let mut bytes = Cursor::new(Vec::new());
    picture.write_to(&mut bytes, format)?;
    Ok(bytes.into_inner())
}

type Named = Vec<(&'static str, Vec<u8>)>;

fn files() -> Result<Named, Box<dyn std::error::Error>> {
    let rgb = DynamicImage::ImageRgb8(ImageBuffer::from_raw(2, 1, vec![255, 0, 0, 0, 102, 255]).unwrap_or_default());
    let sixteen = DynamicImage::ImageLuma16(ImageBuffer::from_raw(3, 1, vec![0, 16384, 65535]).unwrap_or_default());
    let gif = encoded(&rgb, ImageFormat::Gif)?;
    let integers = [0_i32, 32768, 65535].map(i32::to_le_bytes);
    let floats = [0.25_f32, 1.5, -0.5].map(f32::to_le_bytes);
    let rotated = Png::new(3, 2, 8, 2, &[&[1; 9], &[200; 9]])
        .before(chunk("eXIf", &exif(6)))
        .bytes();
    let inks = [0_u8, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 255];
    Ok(vec![
        ("png", Png::new(2, 1, 8, 2, &[&[1, 2, 3, 4, 5, 6]]).bytes()),
        (
            "palette png",
            Png::new(2, 1, 8, 3, &[&[0, 1]])
                .before(chunk("PLTE", &[255, 0, 0, 0, 0, 255]))
                .bytes(),
        ),
        ("16-bit png", Png::new(2, 1, 16, 0, &[&[0, 0, 128, 1]]).bytes()),
        ("exif-rotated png", rotated),
        (
            "icc-tagged png",
            png(3, 2, &[255, 0, 0, 0, 102, 255, 30, 60, 90], &cmyk()),
        ),
        ("gif", gif),
        ("webp", encoded(&rgb, ImageFormat::WebP)?),
        ("tiff", encoded(&rgb, ImageFormat::Tiff)?),
        ("16-bit tiff", encoded(&sixteen, ImageFormat::Tiff)?),
        ("tiff mode I", tiff_wide(3, 1, 2, None, &integers)),
        ("tiff mode F", tiff_wide(3, 1, 3, None, &floats)),
        ("cmyk tiff with profile", tiff(3, 5, &inks, &cmyk())),
        ("cmyk jpeg", cmyk_jpeg()),
        ("cmyk jpeg with profile", with_profile(&cmyk_jpeg(), &cmyk())),
    ])
}

/// The same outcome: equal pictures, or the same kind of refusal.
fn same<T: PartialEq>(id: &str, by_bytes: Result<T, Error>, by_path: Result<T, Error>) {
    match (by_bytes, by_path) {
        (Ok(one), Ok(other)) => assert!(one == other, "{id}: the bytes read differently from the file"),
        (Err(one), Err(other)) => assert_eq!(one.kind(), other.kind(), "{id}"),
        (one, other) => assert_eq!(one.is_ok(), other.is_ok(), "{id}"),
    }
}

#[test]
fn bytes_read_as_the_file_they_came_from_does() -> Outcome {
    let folder = Scratch::new()?;
    for (id, file) in files()? {
        let path = folder.write("file", &file)?;
        same(id, read_bytes(&file), read(&path));
        same(id, read_map_bytes(&file), read_map(&path));
        assert!(read_bytes(&file).is_ok(), "{id}: known good file unreadable");
    }
    Ok(())
}

#[test]
fn bytes_that_are_not_a_whole_image_are_refused_naming_the_given_bytes() {
    let jpeg = cmyk_jpeg();
    let broken: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("text", b"just some text".to_vec()),
        ("cut-short jpeg", jpeg[..jpeg.len() - 20].to_vec()),
    ];
    assert!(read_bytes(&jpeg).is_ok(), "known good: the jpeg whole");
    for (id, bytes) in broken {
        for (reading, error) in [
            ("read_bytes", read_bytes(&bytes).err()),
            ("read_map_bytes", read_map_bytes(&bytes).err()),
        ] {
            assert_eq!(
                error.as_ref().map(Error::kind),
                Some(ErrorKind::ImageRead),
                "{id}, {reading}"
            );
            assert!(
                error.is_some_and(|error| error.to_string().contains("the given bytes")),
                "{id}, {reading}"
            );
        }
    }
}

#[test]
fn a_path_error_still_names_the_path() {
    let error = read(Path::new("missing-picture.png")).err();
    assert!(error.is_some_and(|error| error.to_string().contains("missing-picture.png")));
}

#[test]
fn a_file_longer_than_any_picture_can_be_is_refused_by_its_size_without_being_read() -> Outcome {
    // 2 * 89,478,485 pixels * 8 bytes: the most a decoder may allocate, so no picture the core reads is longer
    const LONGEST: u64 = 2 * 89_478_485 * 8;
    let folder = Scratch::new()?;
    let path = folder.write("huge.png", Png::new(1, 1, 8, 2, &[&[1, 2, 3]]).bytes())?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)?
        .set_len(LONGEST + 1)?; // sparse: no disk
    for (reading, error) in [("read", read(&path).err()), ("read_map", read_map(&path).err())] {
        assert_eq!(error.as_ref().map(Error::kind), Some(ErrorKind::ImageRead), "{reading}");
        let message = error.map(|error| error.to_string()).unwrap_or_default();
        assert!(
            message.contains("huge.png") && message.contains(&(LONGEST + 1).to_string()),
            "{reading}: {message}"
        );
    }
    Ok(())
}
