//! `finding::download::fetch`, through the public API against a server on this computer: a file arrives only if it is
//! the one asked for, and no failure leaves a partial file behind.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;

use bullterrier_image::Error;
use bullterrier_image::finding::download::fetch;

use crate::common::Outcome;

/// The SHA-256 of "abc" (FIPS 180-4's own example): the file every row asks for.
const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

/// A server on this computer that answers each request with the next of `answers` (raw HTTP), then stops. Its base URL.
fn serve(answers: Vec<String>) -> Result<String, std::io::Error> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let base = format!("http://{}", listener.local_addr()?);
    thread::spawn(move || {
        for answer in answers {
            let Ok((mut stream, _)) = listener.accept() else { return };
            let mut request = [0_u8; 4096];
            drop(stream.read(&mut request));
            drop(stream.write_all(answer.as_bytes()));
        }
    });
    Ok(base)
}

fn ok(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn folder(name: &str) -> Result<PathBuf, std::io::Error> {
    let path = std::env::temp_dir().join(format!("bt-image-download-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&path));
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

fn names(folder: &Path) -> Result<Vec<String>, std::io::Error> {
    let mut found: Vec<String> = std::fs::read_dir(folder)?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect();
    found.sort();
    Ok(found)
}

fn refused_with(result: &Result<(), Error>, words: &[&str]) -> bool {
    matches!(result, Err(Error::BackendUnavailable(message)) if words.iter().all(|word| message.contains(word)))
}

#[test]
fn a_file_with_the_right_size_and_checksum_arrives_and_nothing_else_stays() -> Outcome {
    let (base, dir) = (serve(vec![ok("abc")])?, folder("good")?);
    fetch(&format!("{base}/m.onnx"), ABC, 3, &dir.join("deep").join("m.onnx"))?;
    assert_eq!(std::fs::read(dir.join("deep").join("m.onnx"))?, b"abc");
    assert_eq!(names(&dir.join("deep"))?, ["m.onnx"], "no temporary file beside it");
    Ok(())
}

#[test]
fn a_file_that_verifies_is_used_without_the_network() -> Outcome {
    let dir = folder("kept")?;
    std::fs::write(dir.join("m.onnx"), b"abc")?;
    fetch("http://127.0.0.1:9/never", ABC, 3, &dir.join("m.onnx"))?; // nothing listens there
    Ok(())
}

#[test]
fn a_broken_copy_is_replaced() -> Outcome {
    let (base, dir) = (serve(vec![ok("abc")])?, folder("broken")?);
    std::fs::write(dir.join("m.onnx"), b"xyz")?;
    fetch(&format!("{base}/m.onnx"), ABC, 3, &dir.join("m.onnx"))?;
    assert_eq!(std::fs::read(dir.join("m.onnx"))?, b"abc");
    Ok(())
}

#[test]
fn a_file_with_another_checksum_is_not_saved_and_the_message_says_how_to_fetch_it_by_hand() -> Outcome {
    let (base, dir) = (serve(vec![ok("abd")])?, folder("digest")?);
    let url = format!("{base}/m.onnx");
    let refused = fetch(&url, ABC, 3, &dir.join("m.onnx"));
    assert!(
        refused_with(&refused, &["SHA-256", ABC, "by hand", &url, "m.onnx"]),
        "{refused:?}"
    );
    assert!(names(&dir)?.is_empty(), "nothing left: {:?}", names(&dir)?);
    Ok(())
}

#[test]
fn a_server_that_sends_more_than_the_size_is_cut_off() -> Outcome {
    let (base, dir) = (serve(vec![ok("abcd")])?, folder("large")?);
    let refused = fetch(&format!("{base}/m.onnx"), ABC, 3, &dir.join("m.onnx"));
    assert!(
        refused_with(&refused, &["larger than the expected 3 bytes"]),
        "{refused:?}"
    );
    assert_eq!(names(&dir)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn a_server_that_does_not_say_its_size_is_cut_off_at_the_expected_one() -> Outcome {
    // no Content-Length: the body runs to the end of the connection, so only counting what arrives can stop it
    let endless = "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nabcd".to_owned();
    let (base, dir) = (serve(vec![endless])?, folder("unsaid")?);
    let refused = fetch(&format!("{base}/m.onnx"), ABC, 3, &dir.join("m.onnx"));
    assert!(
        refused_with(&refused, &["larger than the expected 3 bytes"]),
        "{refused:?}"
    );
    assert_eq!(names(&dir)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn a_body_that_ends_before_the_size_says_how_much_arrived() -> Outcome {
    let (base, dir) = (
        serve(vec!["HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nab".to_owned()])?,
        folder("ended")?,
    );
    let refused = fetch(&format!("{base}/m.onnx"), ABC, 3, &dir.join("m.onnx"));
    assert!(
        refused_with(&refused, &["stopped after 2 of 3 bytes", "by hand"]),
        "{refused:?}"
    );
    assert_eq!(names(&dir)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn a_download_that_stops_early_is_refused_and_leaves_nothing() -> Outcome {
    let short = "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nab".to_owned();
    let (base, dir) = (serve(vec![short])?, folder("short")?);
    let refused = fetch(&format!("{base}/m.onnx"), ABC, 3, &dir.join("m.onnx"));
    assert!(matches!(refused, Err(Error::BackendUnavailable(_))), "{refused:?}");
    assert_eq!(names(&dir)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn an_answer_that_is_no_file_is_refused_naming_it() -> Outcome {
    let missing = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned();
    let (base, dir) = (serve(vec![missing])?, folder("missing")?);
    let refused = fetch(&format!("{base}/m.onnx"), ABC, 3, &dir.join("m.onnx"));
    assert!(refused_with(&refused, &["404", "by hand"]), "{refused:?}");
    Ok(())
}

#[test]
fn plain_http_to_another_computer_is_refused_before_anything_is_sent() -> Outcome {
    let dir = folder("http")?;
    let refused = fetch("http://example.com/m.onnx", ABC, 3, &dir.join("m.onnx"));
    assert!(
        refused_with(&refused, &["only https", "http", "by hand"]),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_redirect_down_to_plain_http_elsewhere_is_refused() -> Outcome {
    let to_http =
        "HTTP/1.1 302 Found\r\nLocation: http://example.com/m.onnx\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    let (base, dir) = (serve(vec![to_http.to_owned()])?, folder("redirect-down")?);
    let refused = fetch(&format!("{base}/m.onnx"), ABC, 3, &dir.join("m.onnx"));
    assert!(
        refused_with(&refused, &["redirected to http://example.com/m.onnx", "not allowed"]),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_redirect_to_a_path_on_the_same_server_is_followed() -> Outcome {
    let to_path = "HTTP/1.1 302 Found\r\nLocation: /real.onnx\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    let (base, dir) = (serve(vec![to_path.to_owned(), ok("abc")])?, folder("redirect-path")?);
    fetch(&format!("{base}/m.onnx"), ABC, 3, &dir.join("m.onnx"))?;
    assert_eq!(std::fs::read(dir.join("m.onnx"))?, b"abc");
    Ok(())
}

#[test]
fn an_unwritable_folder_is_refused_naming_it() -> Outcome {
    let dir = folder("unwritable")?;
    std::fs::write(dir.join("in-the-way"), b"a file where a folder should be")?;
    let refused = fetch(
        "https://example.com/m.onnx",
        ABC,
        3,
        &dir.join("in-the-way").join("m.onnx"),
    );
    assert!(
        refused_with(&refused, &["cannot write to the folder", "in-the-way"]),
        "{refused:?}"
    );
    Ok(())
}
