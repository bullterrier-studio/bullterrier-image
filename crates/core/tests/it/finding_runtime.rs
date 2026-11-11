//! The ONNX Runtime library, through the public API against a server on this computer: the archive arrives verified, its one
//! library file is kept in the cache folder, and nothing else stays.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;

use bullterrier_image::Error;
use bullterrier_image::finding::onnx::Options;
use bullterrier_image::finding::runtime::{Archive, Runtime, VERSION, install, library, path};
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

/// A gzip tar holding `files` (path, content).
fn tarball(files: &[(&str, &[u8])]) -> Result<Vec<u8>, std::io::Error> {
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
    for (name, content) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append_data(&mut header, name, *content)?;
    }
    builder.into_inner()?.finish()
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

/// The release of a made-up platform whose archive is `bytes`, served at `url`.
fn runtime(url: &str, bytes: &[u8], archive: Archive, file: &'static str) -> Runtime {
    Runtime {
        os: "test",
        arch: "test",
        url: Box::leak(url.to_owned().into_boxed_str()),
        sha256: Box::leak(format!("{:x}", Sha256::digest(bytes)).into_boxed_str()),
        size: bytes.len() as u64,
        archive,
        file,
    }
}

fn names(folder: &Path) -> Result<Vec<String>, std::io::Error> {
    let mut found: Vec<String> = std::fs::read_dir(folder)?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect();
    found.sort();
    Ok(found)
}

#[test]
fn the_library_of_a_tarball_is_taken_out_and_kept_and_the_archive_is_not() -> Outcome {
    let bytes = tarball(&[
        ("onnxruntime-x/include/onnxruntime_c_api.h", b"header"),
        ("onnxruntime-x/lib/libonnxruntime.so.1.23.2", b"THE LIBRARY"),
        ("onnxruntime-x/lib/libonnxruntime_providers_shared.so", b"other"),
    ])?;
    let release = runtime(
        &serve(bytes.clone())?,
        &bytes,
        Archive::TarGz,
        "libonnxruntime.so.1.23.2",
    );
    let cache = folder("tar")?;
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
fn the_library_of_a_zip_is_taken_out_and_kept() -> Outcome {
    let bytes = zipped(&[
        ("onnxruntime-x/lib/onnxruntime.dll", b"DLL"),
        ("onnxruntime-x/lib/onnxruntime.lib", b"import"),
    ])?;
    let release = runtime(&serve(bytes.clone())?, &bytes, Archive::Zip, "onnxruntime.dll");
    let cache = folder("zip")?;
    let kept = install(&release, &cache)?;
    assert_eq!(std::fs::read(&kept)?, b"DLL");
    assert_eq!(names(kept.parent().ok_or("parent")?)?, ["onnxruntime.dll"]);
    Ok(())
}

#[test]
fn an_archive_without_the_library_is_refused_naming_the_fix_and_leaves_nothing() -> Outcome {
    let bytes = tarball(&[
        ("onnxruntime-x/lib/other.so", b"x"),
        ("elsewhere/libonnxruntime.so.1.23.2", b"not in lib"),
    ])?;
    let release = runtime(
        &serve(bytes.clone())?,
        &bytes,
        Archive::TarGz,
        "libonnxruntime.so.1.23.2",
    );
    let cache = folder("missing")?;
    let refused = install(&release, &cache);
    assert!(
        matches!(&refused, Err(Error::BackendUnavailable(message)) if message.contains("holds no libonnxruntime.so.1.23.2") && message.contains("by hand")),
        "{refused:?}"
    );
    let kept = path(&cache, &release);
    assert!(!kept.exists());
    assert_eq!(names(kept.parent().ok_or("parent")?)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn an_archive_that_is_not_the_pinned_one_is_refused() -> Outcome {
    let bytes = tarball(&[("a/lib/libonnxruntime.so.1.23.2", b"x")])?;
    let mut release = runtime(
        &serve(bytes.clone())?,
        &bytes,
        Archive::TarGz,
        "libonnxruntime.so.1.23.2",
    );
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
fn every_platform_has_one_pinned_release_with_a_checksum_and_a_library_file() {
    let releases = bullterrier_image::finding::runtime::runtimes();
    assert_eq!(releases.len(), 6);
    for release in releases {
        assert_eq!(release.sha256.len(), 64, "{release:?}");
        assert!(
            release.url.starts_with("https://") && release.url.contains(VERSION),
            "{release:?}"
        );
        let expected = if release.os == "windows" {
            Archive::Zip
        } else {
            Archive::TarGz
        };
        assert_eq!(release.archive, expected, "{release:?}");
    }
}
