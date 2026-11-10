//! Running a model with ONNX Runtime, which is loaded at run time from a library file (none is bundled), and the model
//! files it runs: where they are kept, and the checksum that says a file is the model it should be.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ort::session::Session as OrtSession;
use ort::value::Tensor as OrtTensor;
use sha2::{Digest, Sha256};

use super::{Infer, Model, Tensor};
use crate::Error;

/// A model file loaded into ONNX Runtime, kept for the next photo (loading takes a second).
pub struct Session {
    /// `run` takes the session by mutable reference, so one photo at a time asks it.
    inner: Mutex<OrtSession>,
    /// The name of the model's first input, the one a photo is given as.
    input: String,
}

impl Session {
    /// The model in `file`, run by the ONNX Runtime library at `library` (the file `libonnxruntime.so`,
    /// `libonnxruntime.dylib` or `onnxruntime.dll`), on the CPU.
    ///
    /// # Errors
    /// [`Error::BackendUnavailable`] naming the file if the library cannot be loaded or ONNX Runtime cannot use the model
    /// file (it is not an ONNX model).
    pub fn open(library: &Path, file: &Path) -> Result<Self, Error> {
        let unavailable = |error: &dyn std::fmt::Display| {
            Error::BackendUnavailable(format!(
                "onnxruntime ({}) cannot use the model file {}: {error}",
                library.display(),
                file.display()
            ))
        };
        let environment = ort::environment::init_from(library)
            .map_err(|error| unavailable(&error))?
            .build()
            .map_err(|error| unavailable(&error))?;
        let session = OrtSession::builder(&environment)
            .and_then(|mut builder| builder.commit_from_file(file))
            .map_err(|error| unavailable(&error))?;
        let input = session
            .inputs()
            .first()
            .map(|input| input.name().to_owned())
            .ok_or_else(|| unavailable(&"the model has no input"))?;
        Ok(Self {
            inner: Mutex::new(session),
            input,
        })
    }
}

impl Infer for Session {
    fn infer(&self, input: &Tensor) -> Result<Tensor, Error> {
        let unavailable = |error: &dyn std::fmt::Display| {
            Error::BackendUnavailable(format!("onnxruntime could not run the model on this input: {error}"))
        };
        let shape: Vec<i64> = input
            .shape()
            .iter()
            .map(|&side| i64::try_from(side).unwrap_or(i64::MAX))
            .collect();
        let tensor = OrtTensor::from_array((shape, input.values().to_vec())).map_err(|error| unavailable(&error))?;
        let mut session = self
            .inner
            .lock()
            .map_err(|_| Error::BackendUnavailable("a model run panicked earlier".to_owned()))?;
        let outputs = session
            .run(ort::inputs![self.input.as_str() => tensor])
            .map_err(|error| unavailable(&error))?;
        let (shape, values) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|error| unavailable(&error))?;
        Tensor::new(
            shape.iter().map(|&side| usize::try_from(side).unwrap_or(0)).collect(),
            values.to_vec(),
        )
    }
}

/// The SHA-256 of the file at `path`, as 64 lowercase hex digits.
///
/// # Errors
/// [`Error::BackendUnavailable`] naming the file if it cannot be read.
pub fn sha256_of(path: &Path) -> Result<String, Error> {
    let unreadable = |error: std::io::Error| {
        Error::BackendUnavailable(format!("cannot read the model file {}: {error}", path.display()))
    };
    let mut file = std::fs::File::open(path).map_err(unreadable)?;
    let mut digest = Sha256::new();
    let mut chunk = vec![0_u8; 1 << 20]; // constant: bytes read at a time, memory use only
    loop {
        let read = file.read(&mut chunk).map_err(unreadable)?;
        if read == 0 {
            break;
        }
        digest.update(&chunk[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// Whether the file at `path` is `model`: its size and SHA-256 are the catalogue's.
///
/// # Errors
/// [`Error::BackendUnavailable`] naming the file if it cannot be read.
pub fn is_model(model: &Model, path: &Path) -> Result<bool, Error> {
    let size = std::fs::metadata(path)
        .map_err(|error| Error::BackendUnavailable(format!("cannot read the model file {}: {error}", path.display())))?
        .len();
    Ok(size == model.size && sha256_of(path)? == model.sha256)
}

/// The folder downloaded files are kept in, by the platform's convention: macOS `~/Library/Caches/bt-image`, Windows
/// `%LOCALAPPDATA%\bt-image\Cache`, elsewhere `$XDG_CACHE_HOME/bt-image` or `~/.cache/bt-image`.
///
/// # Errors
/// [`Error::BackendUnavailable`] if the home folder is not known (no `HOME`, no `USERPROFILE`).
pub fn cache_dir() -> Result<PathBuf, Error> {
    let os = if cfg!(target_os = "macos") {
        Os::Mac
    } else if cfg!(windows) {
        Os::Windows
    } else {
        Os::Other
    };
    cache_dir_for(os, |name| {
        std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    })
}

/// The platform families that keep a cache in different places.
#[derive(Clone, Copy)]
enum Os {
    Mac,
    Windows,
    Other,
}

/// [`cache_dir`] for `os`, reading the environment variables through `var`.
fn cache_dir_for(os: Os, var: impl Fn(&str) -> Option<PathBuf>) -> Result<PathBuf, Error> {
    const APP: &str = "bt-image";
    let home = || {
        var("HOME")
            .or_else(|| var("USERPROFILE"))
            .ok_or_else(|| Error::BackendUnavailable("the home folder is not known: set HOME".to_owned()))
    };
    Ok(match os {
        Os::Mac => home()?.join("Library").join("Caches").join(APP),
        Os::Windows => match var("LOCALAPPDATA") {
            Some(local) => local.join(APP).join("Cache"),
            None => home()?.join("AppData").join("Local").join(APP).join("Cache"),
        },
        Os::Other => match var("XDG_CACHE_HOME") {
            Some(cache) => cache.join(APP),
            None => home()?.join(".cache").join(APP),
        },
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<PathBuf> {
        let map: HashMap<String, PathBuf> = pairs.iter().map(|(k, v)| ((*k).to_owned(), PathBuf::from(v))).collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn the_cache_follows_each_platforms_convention() -> Result<(), Error> {
        let home = [("HOME", "/h")];
        assert_eq!(
            cache_dir_for(Os::Mac, env(&home))?,
            PathBuf::from("/h/Library/Caches/bt-image")
        );
        assert_eq!(
            cache_dir_for(Os::Other, env(&home))?,
            PathBuf::from("/h/.cache/bt-image")
        );
        assert_eq!(
            cache_dir_for(Os::Other, env(&[("HOME", "/h"), ("XDG_CACHE_HOME", "/x")]))?,
            PathBuf::from("/x/bt-image")
        );
        let windows = [("USERPROFILE", "C:/u")];
        assert_eq!(
            cache_dir_for(Os::Windows, env(&windows))?,
            PathBuf::from("C:/u/AppData/Local/bt-image/Cache")
        );
        assert_eq!(
            cache_dir_for(Os::Windows, env(&[("LOCALAPPDATA", "D:/l")]))?,
            PathBuf::from("D:/l/bt-image/Cache")
        );
        Ok(())
    }

    #[test]
    fn a_cache_with_no_home_is_refused_saying_what_to_set() {
        let refused = cache_dir_for(Os::Other, env(&[]));
        assert!(matches!(refused, Err(Error::BackendUnavailable(message)) if message.contains("HOME")));
    }

    #[test]
    fn the_checksum_of_a_file_is_its_sha256() -> Result<(), Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(format!("bt-image-sha-{}", std::process::id()));
        std::fs::write(&path, b"abc")?;
        // the SHA-256 of "abc", from FIPS 180-4's own example
        let digest = sha256_of(&path)?;
        std::fs::remove_file(&path)?;
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        Ok(())
    }
}
