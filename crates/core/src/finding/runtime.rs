//! The ONNX Runtime library: which one this package runs models with, where it is kept, and getting it.
//!
//! None is bundled. A release of ONNX Runtime is pinned per platform with its size and SHA-256 ([`runtimes`]): Microsoft's
//! wheel of it on `PyPI`, a zip that holds the library; [`install`] downloads the wheel (see [`download::fetch`]), takes the
//! one library file out of it and keeps that file in the cache folder, where [`library`] finds it. The library file is
//! loaded, never run, and the wheel's Python code is never used.

use std::io::Read as _;
use std::path::{Path, PathBuf};

use super::{download, onnx};
use crate::Error;

/// The release of ONNX Runtime the package pins.
pub const VERSION: &str = "1.23.2";

/// A release of ONNX Runtime for one platform.
#[derive(Clone, Copy, Debug)]
pub struct Runtime {
    /// The operating system, as `std::env::consts::OS` names it: `macos`, `linux`, `windows`.
    pub os: &'static str,
    /// The processor, as `std::env::consts::ARCH` names it: `aarch64`, `x86_64`.
    pub arch: &'static str,
    /// Where the archive, a zip, is downloaded from.
    pub url: &'static str,
    /// The SHA-256 of the archive, as 64 lowercase hex digits.
    pub sha256: &'static str,
    /// The size of the archive in bytes.
    pub size: u64,
    /// The path of the library file inside the archive, folders split by `/`; its last part is the name it is kept under.
    pub entry: &'static str,
}

impl Runtime {
    /// The name of the library file: the last part of [`entry`](Self::entry).
    #[must_use]
    pub(crate) fn file(&self) -> &'static str {
        self.entry.rsplit('/').next().unwrap_or(self.entry)
    }
}

/// The releases the package pins, one per platform ONNX Runtime publishes a wheel of this release for (Windows on arm64
/// has none: point `BT_IMAGE_ONNX_LIB` at a library there).
#[must_use]
pub fn runtimes() -> &'static [Runtime] {
    &[
        Runtime {
            os: "macos",
            arch: "aarch64",
            url: "https://files.pythonhosted.org/packages/1b/9e/f748cd64161213adeef83d0cb16cb8ace1e62fa501033acdd9f9341fff57/onnxruntime-1.23.2-cp312-cp312-macosx_13_0_arm64.whl",
            sha256: "b8f029a6b98d3cf5be564d52802bb50a8489ab73409fa9db0bf583eabb7c2321",
            size: 17_195_929,
            entry: "onnxruntime/capi/libonnxruntime.1.23.2.dylib",
        },
        Runtime {
            os: "macos",
            arch: "x86_64",
            url: "https://files.pythonhosted.org/packages/91/9d/a81aafd899b900101988ead7fb14974c8a58695338ab6a0f3d6b0100f30b/onnxruntime-1.23.2-cp312-cp312-macosx_13_0_x86_64.whl",
            sha256: "218295a8acae83905f6f1aed8cacb8e3eb3bd7513a13fe4ba3b2664a19fc4a6b",
            size: 19_157_705,
            entry: "onnxruntime/capi/libonnxruntime.1.23.2.dylib",
        },
        Runtime {
            os: "linux",
            arch: "x86_64",
            url: "https://files.pythonhosted.org/packages/ef/88/9cc25d2bafe6bc0d4d3c1db3ade98196d5b355c0b273e6a5dc09c5d5d0d5/onnxruntime-1.23.2-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl",
            sha256: "0f9b4ae77f8e3c9bee50c27bc1beede83f786fe1d52e99ac85aa8d65a01e9b77",
            size: 17_382_649,
            entry: "onnxruntime/capi/libonnxruntime.so.1.23.2",
        },
        Runtime {
            os: "linux",
            arch: "aarch64",
            url: "https://files.pythonhosted.org/packages/3c/35/4e40f2fba272a6698d62be2cd21ddc3675edfc1a4b9ddefcc4648f115315/onnxruntime-1.23.2-cp312-cp312-manylinux_2_27_aarch64.manylinux_2_28_aarch64.whl",
            sha256: "76ff670550dc23e58ea9bc53b5149b99a44e63b34b524f7b8547469aaa0dcb8c",
            size: 15_226_915,
            entry: "onnxruntime/capi/libonnxruntime.so.1.23.2",
        },
        Runtime {
            os: "windows",
            arch: "x86_64",
            url: "https://files.pythonhosted.org/packages/c0/b4/569d298f9fc4d286c11c45e85d9ffa9e877af12ace98af8cab52396e8f46/onnxruntime-1.23.2-cp312-cp312-win_amd64.whl",
            sha256: "25de5214923ce941a3523739d34a520aac30f21e631de53bba9174dc9c004435",
            size: 13_470_528,
            entry: "onnxruntime/capi/onnxruntime.dll",
        },
    ]
}

/// The release for this platform.
///
/// # Errors
/// [`Error::BackendUnavailable`] naming the platform when none is pinned for it, and saying to point
/// `BT_IMAGE_ONNX_LIB` at a library of your own.
pub fn here() -> Result<&'static Runtime, Error> {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    runtimes()
        .iter()
        .find(|runtime| runtime.os == os && runtime.arch == arch)
        .ok_or_else(|| {
            Error::BackendUnavailable(format!(
                "no ONNX Runtime {VERSION} is pinned for {os} on {arch}: set BT_IMAGE_ONNX_LIB to the file of a library you \
                 have (libonnxruntime.so, libonnxruntime.dylib or onnxruntime.dll)"
            ))
        })
}

/// Where `runtime`'s library is kept under the cache folder `cache`.
#[must_use]
pub fn path(cache: &Path, runtime: &Runtime) -> PathBuf {
    cache.join(format!("onnxruntime-{VERSION}")).join(runtime.file())
}

/// The library file `options` lead to: the one they name, else the one kept in the cache folder; downloaded first if it is not
/// there and `options` allow. `cache` is the folder downloads are kept in.
///
/// # Errors
/// [`Error::BackendUnavailable`] when none is named or kept and the download is not allowed (saying how to get it), or
/// as [`install`].
pub fn library(options: &onnx::Options, cache: &Path) -> Result<PathBuf, Error> {
    if let Some(named) = options.library() {
        return Ok(named.to_path_buf());
    }
    let runtime = here()?;
    let kept = path(cache, runtime);
    if kept.is_file() {
        return Ok(kept);
    }
    if options.downloads() {
        return install(runtime, cache);
    }
    Err(Error::BackendUnavailable(format!(
        "the ONNX Runtime library is not installed (it is expected at {}): run `bt-image install onnx`, or allow downloading \
         it (BT_IMAGE_ALLOW_DOWNLOAD=1, or the option of the call), or set BT_IMAGE_ONNX_LIB to the file of a library you \
         have",
        kept.display()
    )))
}

/// Downloads `runtime`'s archive (verified), takes its library out and keeps it in the cache folder `cache`; returns its
/// path. A library already kept is left as it is.
///
/// # Errors
/// [`Error::BackendUnavailable`] naming the URL and the file, when the download fails or does not verify, the archive
/// holds no such library, or the file cannot be written.
pub fn install(runtime: &Runtime, cache: &Path) -> Result<PathBuf, Error> {
    let target = path(cache, runtime);
    if target.is_file() {
        return Ok(target);
    }
    let fail = |why: String| {
        Error::BackendUnavailable(format!(
            "cannot install ONNX Runtime {VERSION} from {}: {why}; download it by hand and save its {} as {}",
            runtime.url,
            runtime.entry,
            target.display()
        ))
    };
    let folder = target.parent().map_or_else(|| cache.to_path_buf(), Path::to_path_buf);
    std::fs::create_dir_all(&folder)
        .map_err(|error| fail(format!("cannot write to {} ({error})", folder.display())))?;
    let archive = folder.join(format!(".{}.download", runtime.file()));
    download::fetch(runtime.url, runtime.sha256, runtime.size, &archive)?;
    let taken = take(runtime, &archive, &folder);
    drop(std::fs::remove_file(&archive)); // best effort: the archive is not needed once the library is out
    let temporary = taken.map_err(fail)?;
    std::fs::rename(&temporary, &target).map_err(|error| {
        drop(std::fs::remove_file(&temporary)); // best effort: a partial file is never used
        fail(format!("cannot save {} ({error})", target.display()))
    })?;
    Ok(target)
}

/// The library file of the archive at `archive`, written into `folder` under a temporary name; its path. The library
/// is read whole before anything is written, so a damaged one leaves no file.
fn take(runtime: &Runtime, archive: &Path, folder: &Path) -> Result<PathBuf, String> {
    let opened = std::fs::File::open(archive).map_err(|error| format!("cannot read the archive ({error})"))?;
    let mut zip = zip::ZipArchive::new(opened).map_err(|error| format!("the archive is damaged ({error})"))?;
    let mut entry = zip
        .by_name(runtime.entry)
        .map_err(|error| format!("the archive holds no {} ({error})", runtime.entry))?;
    let mut library = Vec::new();
    entry
        .read_to_end(&mut library)
        .map_err(|error| format!("the archive is damaged ({error})"))?;
    let temporary = folder.join(format!(".{}.{}.part", runtime.file(), std::process::id()));
    std::fs::write(&temporary, &library).map_err(|error| {
        drop(std::fs::remove_file(&temporary)); // best effort: a partial file is never used
        format!("cannot write ({error})")
    })?;
    Ok(temporary)
}
