//! What a remembered result depends on: the fingerprint of the parts of a key, the identity of the core that made it, and the
//! cache of maps. The cache is minimal on purpose: a file that is not exactly what was written is simply not there.

use std::path::PathBuf;

use bullterrier_image::Layer;
use bullterrier_image::pipeline::{MapCache, core_identity, fingerprint, identity_files};

use crate::common::Outcome;

fn folder(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("bt-image-cache-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&path));
    path
}

#[test]
fn a_fingerprint_is_the_sha256_of_its_parts_joined_by_a_nul() {
    // FIPS 180-4's own example for one part; the join for two is a NUL, so ("a", "bc") is not ("ab", "c")
    assert_eq!(
        fingerprint(&["abc"]),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_ne!(fingerprint(&["a", "bc"]), fingerprint(&["ab", "c"]));
    assert_ne!(fingerprint(&["a", "bc"]), fingerprint(&["abc"]));
    assert_eq!(fingerprint(&["a", "bc"]), fingerprint(&["a", "bc"]));
}

#[test]
fn the_identity_of_the_core_covers_its_code_and_the_data_that_ships_with_it() {
    let identity = core_identity();
    assert_eq!(identity.len(), 64);
    assert!(identity.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(identity, core_identity());
    let files = identity_files();
    // a kernel, the runner, a shipped preset and a shipped look are all in what the identity hashes: changing any of them
    // changes the key of every result
    for covered in [
        "src/effects/grade.rs",
        "src/run.rs",
        "data/presets/grade.toml",
        "data/looks/crt.toml",
        "Cargo.toml",
    ] {
        assert!(
            files.contains(&covered),
            "{covered} is not covered: {} files",
            files.len()
        );
    }
    assert!(
        files.windows(2).all(|pair| pair[0] < pair[1]),
        "in order, so the hash does not depend on the file system"
    );
}

fn matte(height: usize, width: usize) -> Result<Layer, bullterrier_image::Error> {
    Layer::new(
        height,
        width,
        (0..height * width)
            .map(|index| f32::from(u8::try_from(index % 7).unwrap_or(0)) / 7.0)
            .collect(),
    )
}

#[test]
fn a_map_put_in_the_cache_is_found_with_the_same_values() -> Outcome {
    let cache = MapCache::new(folder("roundtrip"));
    assert!(cache.get("key", 3, 5).is_none(), "nothing there yet");
    cache.put("key", &matte(3, 5)?)?;
    assert_eq!(cache.get("key", 3, 5), Some(matte(3, 5)?));
    assert!(cache.get("other", 3, 5).is_none(), "another key is another map");
    Ok(())
}

#[test]
fn a_map_of_another_size_is_not_found_and_is_made_again() -> Outcome {
    let cache = MapCache::new(folder("size"));
    cache.put("key", &matte(3, 5)?)?;
    assert!(cache.get("key", 5, 3).is_none() && cache.get("key", 3, 6).is_none() && cache.get("key", 4, 5).is_none());
    Ok(())
}

#[test]
fn a_file_that_is_not_what_was_written_is_not_found_whatever_is_wrong_with_it() -> Outcome {
    let dir = folder("damaged");
    let cache = MapCache::new(dir.clone());
    cache.put("key", &matte(3, 5)?)?;
    let path = std::fs::read_dir(&dir)?.next().ok_or("one file")??.path();
    let good = std::fs::read(&path)?;
    let mut wrong_magic = good.clone();
    wrong_magic[0] ^= 0xFF;
    let mut wrong_version = good.clone();
    wrong_version[8] = wrong_version[8].wrapping_add(1); // the version follows the 8-byte magic
    let cases: [(&str, Vec<u8>); 5] = [
        ("an empty file", Vec::new()),
        ("a truncated file", good[..good.len() - 1].to_vec()),
        ("a longer file", [good.clone(), vec![0]].concat()),
        ("another magic", wrong_magic),
        ("another version", wrong_version),
    ];
    for (what, bytes) in cases {
        std::fs::write(&path, bytes)?;
        assert!(cache.get("key", 3, 5).is_none(), "{what}");
    }
    std::fs::write(&path, &good)?;
    assert!(cache.get("key", 3, 5).is_some(), "the original bytes are found again");
    Ok(())
}

#[test]
fn a_value_outside_a_map_is_not_found_rather_than_handed_back() -> Outcome {
    let dir = folder("range");
    let cache = MapCache::new(dir.clone());
    cache.put("key", &matte(1, 2)?)?;
    let path = std::fs::read_dir(&dir)?.next().ok_or("one file")??.path();
    let mut bytes = std::fs::read(&path)?;
    let at = bytes.len() - 4;
    bytes[at..].copy_from_slice(&f32::NAN.to_le_bytes());
    std::fs::write(&path, bytes)?;
    assert!(cache.get("key", 1, 2).is_none(), "a NaN is no map");
    Ok(())
}

#[test]
fn putting_a_map_leaves_no_partial_file_and_an_unwritable_folder_is_an_error() -> Outcome {
    let dir = folder("atomic");
    let cache = MapCache::new(dir.clone());
    cache.put("key", &matte(2, 2)?)?;
    cache.put("key", &matte(2, 2)?)?; // twice: the second replaces the first
    let names: Vec<String> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
    assert_eq!(
        names[0],
        format!("{}.map", names[0].trim_end_matches(".map")),
        "{names:?}"
    );
    assert!(!names[0].starts_with('.'), "{names:?}");
    let blocked = folder("blocked");
    std::fs::create_dir_all(&blocked)?;
    std::fs::write(blocked.join("in-the-way"), b"a file where a folder should be")?;
    let refused = MapCache::new(blocked.join("in-the-way")).put("key", &matte(2, 2)?);
    assert!(refused.is_err());
    Ok(())
}
