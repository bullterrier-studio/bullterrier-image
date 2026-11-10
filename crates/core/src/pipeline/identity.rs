//! What a remembered result depends on: the fingerprint of the parts of a key, and the identity of the core that made it.

use sha2::{Digest, Sha256};

include!(concat!(env!("OUT_DIR"), "/identity.rs"));

/// The SHA-256 of `parts` joined by a NUL, as 64 lowercase hex digits: the key of a result or a map is a fingerprint of
/// everything that shaped it, so a part that changes gives another key.
#[must_use]
pub fn fingerprint(parts: &[&str]) -> String {
    format!("{:x}", Sha256::digest(parts.join("\0").as_bytes()))
}

/// The identity of this build of the core: the crate's version and a hash of every file that can change what it makes (its
/// sources, the presets and looks that ship with it, its manifest and the workspace's lock file: the code of the crates it
/// builds with), taken when it was built. A result made by a core with another identity is not taken for this one's: a
/// change to any effect, to a shipped preset or look, or to a library the core uses changes it.
#[must_use]
pub fn core_identity() -> String {
    fingerprint(&[env!("CARGO_PKG_VERSION"), SOURCE_DIGEST])
}

/// The files [`core_identity`] covers, by path relative to the core's folder, in order.
#[must_use]
pub fn identity_files() -> &'static [&'static str] {
    SOURCE_FILES
}
