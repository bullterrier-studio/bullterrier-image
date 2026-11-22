//! The ONNX Runtime library, through the public API against a server on this computer: the archive arrives verified, its one
//! library file is kept in the cache folder, and nothing else stays.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;

use bullterrier_image::Error;
use bullterrier_image::finding::onnx::Options;
use bullterrier_image::finding::runtime::{Runtime, VERSION, install, library, path};
use sha2::{Digest, Sha256};

use crate::common::Outcome;

/// A server on this computer that answers the next request with `body` as a file, then stops. Its URL.
fn serve(body: Vec<u8>) -> Result<String, std::io::Error> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}/archive", listener.local_addr()?);
    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else { return };
        let mut request = [0_u8; 4096];
        drop(stream.read(&mut request));
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        drop(stream.write_all(head.as_bytes()));
        drop(stream.write_all(&body));
    });
    Ok(url)
}

fn folder(name: &str) -> Result<PathBuf, std::io::Error> {
    let path = std::env::temp_dir().join(format!("bt-image-runtime-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&path));
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// A zip holding `files` (path, content).
fn zipped(files: &[(&str, &[u8])]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, content) in files {
        writer.start_file(*name, zip::write::SimpleFileOptions::default())?;
        writer.write_all(content)?;
    }
    Ok(writer.finish()?.into_inner())
}

/// The release of a made-up platform whose archive is `bytes`, served at `url`, with its library at `entry`.
fn runtime(url: &str, bytes: &[u8], entry: &'static str) -> Runtime {
    Runtime {
        os: "test",
        arch: "test",
        url: Box::leak(url.to_owned().into_boxed_str()),
        sha256: Box::leak(format!("{:x}", Sha256::digest(bytes)).into_boxed_str()),
        size: bytes.len() as u64,
        entry,
    }
}

fn names(folder: &Path) -> Result<Vec<String>, std::io::Error> {
    let mut found: Vec<String> = std::fs::read_dir(folder)?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect();
    found.sort();
    Ok(found)
}

/// Where a wheel of ONNX Runtime keeps its library.
const ENTRY: &str = "onnxruntime/capi/libonnxruntime.so.1.23.2";

#[test]
fn the_library_at_the_entry_is_taken_out_and_kept_and_the_archive_is_not() -> Outcome {
    let bytes = zipped(&[
        ("onnxruntime/__init__.py", b"python"),
        ("elsewhere/libonnxruntime.so.1.23.2", b"a file of the same name"),
        (ENTRY, b"THE LIBRARY"),
        ("onnxruntime/capi/libonnxruntime_providers_shared.so", b"other"),
    ])?;
    let release = runtime(&serve(bytes.clone())?, &bytes, ENTRY);
    let cache = folder("wheel")?;
    let kept = install(&release, &cache)?;
    assert_eq!(kept, path(&cache, &release));
    assert_eq!(
        kept,
        cache
            .join(format!("onnxruntime-{VERSION}"))
            .join("libonnxruntime.so.1.23.2")
    );
    assert_eq!(std::fs::read(&kept)?, b"THE LIBRARY");
    assert_eq!(
        names(kept.parent().ok_or("parent")?)?,
        ["libonnxruntime.so.1.23.2"],
        "no archive, no partial file"
    );
    // a library kept is used without touching the network: the server answered once and is gone
    assert_eq!(install(&release, &cache)?, kept);
    Ok(())
}

#[test]
fn an_archive_without_the_library_is_refused_naming_the_fix_and_leaves_nothing() -> Outcome {
    let bytes = zipped(&[
        ("onnxruntime/capi/other.so", b"x"),
        ("elsewhere/libonnxruntime.so.1.23.2", b"not at the entry"),
    ])?;
    let release = runtime(&serve(bytes.clone())?, &bytes, ENTRY);
    let cache = folder("missing")?;
    let refused = install(&release, &cache);
    assert!(
        matches!(&refused, Err(Error::BackendUnavailable(message)) if message.contains("holds no onnxruntime/capi/libonnxruntime.so.1.23.2") && message.contains("by hand")),
        "{refused:?}"
    );
    let kept = path(&cache, &release);
    assert!(!kept.exists());
    assert_eq!(names(kept.parent().ok_or("parent")?)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn a_damaged_library_in_a_verified_archive_is_refused_as_damaged_and_leaves_nothing() -> Outcome {
    // stored, not compressed, so a flipped byte of the library fails its CRC when it is read; the archive's own
    // checksum is of the damaged bytes, so the download verifies
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file(ENTRY, stored)?;
    writer.write_all(b"THE LIBRARY")?;
    let mut bytes = writer.finish()?.into_inner();
    let at = bytes
        .windows(11)
        .position(|window| window == b"THE LIBRARY")
        .ok_or("the library's bytes")?;
    bytes[at] ^= 1;
    let release = runtime(&serve(bytes.clone())?, &bytes, ENTRY);
    let cache = folder("damaged")?;
    let refused = install(&release, &cache);
    assert!(
        matches!(&refused, Err(Error::BackendUnavailable(message)) if message.contains("damaged")),
        "{refused:?}"
    );
    let kept = path(&cache, &release);
    assert_eq!(
        names(kept.parent().ok_or("parent")?)?,
        Vec::<String>::new(),
        "no partial file"
    );
    Ok(())
}

#[test]
fn an_archive_that_is_not_the_pinned_one_is_refused() -> Outcome {
    let bytes = zipped(&[(ENTRY, b"x")])?;
    let mut release = runtime(&serve(bytes.clone())?, &bytes, ENTRY);
    release.sha256 = "0000000000000000000000000000000000000000000000000000000000000000";
    let cache = folder("wrong")?;
    assert!(matches!(install(&release, &cache), Err(Error::BackendUnavailable(_))));
    assert!(!path(&cache, &release).exists());
    Ok(())
}

#[test]
fn the_library_is_the_named_one_else_the_kept_one_else_an_error_that_says_how_to_get_it() -> Outcome {
    let here = bullterrier_image::finding::runtime::here()?;
    let cache = folder("library")?;
    // none named, none kept, no download allowed
    let missing = library(&Options::new(), &cache);
    assert!(
        matches!(&missing, Err(Error::BackendUnavailable(message)) if message.contains("bt-image install onnx") && message.contains("BT_IMAGE_ONNX_LIB")),
        "{missing:?}"
    );
    // kept
    let kept = path(&cache, here);
    std::fs::create_dir_all(kept.parent().ok_or("parent")?)?;
    std::fs::write(&kept, b"x")?;
    assert_eq!(library(&Options::new(), &cache)?, kept);
    // named wins over kept
    assert_eq!(
        library(&Options::new().with_library("/elsewhere/lib.so"), &cache)?,
        Path::new("/elsewhere/lib.so")
    );
    Ok(())
}

#[test]
fn each_platform_with_a_wheel_pins_that_wheel_of_the_release_and_its_library() {
    // (os, arch, the wheel's platform tag on PyPI): ONNX Runtime 1.23.2 publishes no Windows arm64 wheel
    let wheels = [
        ("macos", "aarch64", "macosx_13_0_arm64"),
        ("macos", "x86_64", "macosx_13_0_x86_64"),
        ("linux", "x86_64", "manylinux_2_27_x86_64"),
        ("linux", "aarch64", "manylinux_2_27_aarch64"),
        ("windows", "x86_64", "win_amd64"),
    ];
    // where a wheel keeps the library, named as ONNX Runtime names it on each system
    let library = |os: &str| match os {
        "macos" => format!("onnxruntime/capi/libonnxruntime.{VERSION}.dylib"),
        "linux" => format!("onnxruntime/capi/libonnxruntime.so.{VERSION}"),
        _ => "onnxruntime/capi/onnxruntime.dll".to_owned(),
    };
    let releases = bullterrier_image::finding::runtime::runtimes();
    assert_eq!(releases.len(), wheels.len());
    for (os, arch, tag) in wheels {
        let found: Vec<&Runtime> = releases
            .iter()
            .filter(|release| release.os == os && release.arch == arch)
            .collect();
        assert_eq!(found.len(), 1, "{os} on {arch}: {found:?}");
        for release in found {
            let wheel = release.url.rsplit('/').next().unwrap_or_default();
            assert!(
                release.url.starts_with("https://files.pythonhosted.org/")
                    && wheel.starts_with(&format!("onnxruntime-{VERSION}-"))
                    && wheel.contains(tag)
                    && Path::new(wheel).extension().is_some_and(|extension| extension == "whl"),
                "{os} on {arch}: {}",
                release.url
            );
            assert_eq!(release.entry, library(os), "{os} on {arch}");
            assert_eq!(release.sha256.len(), 64, "{os} on {arch}");
        }
    }
}
