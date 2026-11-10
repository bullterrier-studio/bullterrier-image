//! The maps a pipeline remembers (a subject's matte, a depth) so a photo is analysed once across runs.
//!
//! A file is the magic `BTIMGMAP`, the format version (u32), the height (u32), the width (u32), then the values as `f32`,
//! little-endian. It is minimal on purpose: a file that is not exactly that, of the size asked for, is not found (and the map is
//! made again), never an error and never migrated.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Error, Layer};

/// The first bytes of a file.
const MAGIC: &[u8; 8] = b"BTIMGMAP"; // constant: the format's own name
/// The version of the format: a file of another version is not found.
const VERSION: u32 = 1; // constant: bump it when the layout changes
/// A name no other `put` of this process shares.
static NEXT: AtomicU64 = AtomicU64::new(0);
/// Magic, version, height and width.
const HEADER: usize = 8 + 3 * 4; // constant: the layout above

/// A folder of remembered maps, each under the key it was put with.
#[derive(Clone, Debug)]
pub struct MapCache {
    folder: PathBuf,
}

impl MapCache {
    /// The cache in `folder`, made on the first [`put`](Self::put).
    #[must_use]
    pub const fn new(folder: PathBuf) -> Self {
        Self { folder }
    }

    fn file(&self, key: &str) -> Option<PathBuf> {
        let plain = !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        (plain && !key.starts_with('.')).then(|| self.folder.join(format!("{key}.map")))
    }

    /// The map remembered under `key`, if there is one that is exactly what [`put`](Self::put) writes, of `height` x
    /// `width`, and every value is a map's (finite, within 0..1).
    #[must_use]
    pub fn get(&self, key: &str, height: usize, width: usize) -> Option<Layer> {
        let bytes = std::fs::read(self.file(key)?).ok()?;
        let (head, body) = bytes.split_at_checked(HEADER)?;
        let number = |at: usize| u32::from_le_bytes([head[at], head[at + 1], head[at + 2], head[at + 3]]);
        if &head[..8] != MAGIC
            || number(8) != VERSION
            || usize::try_from(number(12)).ok()? != height
            || usize::try_from(number(16)).ok()? != width
            || body.len() != height.checked_mul(width)?.checked_mul(4)?
        {
            return None;
        }
        let values = body
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| f32::from_le_bytes(*chunk))
            .collect();
        Layer::new(height, width, values).ok()
    }

    /// Remembers `map` under `key` (a name of letters, digits, `-`, `_` and `.`, not starting with a dot), replacing what
    /// was there. The file is written whole beside its place and moved there, so a reader never sees half of it.
    ///
    /// # Errors
    /// [`Error::ImageWrite`] naming the folder if `key` is not such a name or the folder cannot be written. A map that
    /// cannot be remembered is the disk's trouble, not the map's: the caller goes on with it.
    pub fn put(&self, key: &str, map: &Layer) -> Result<(), Error> {
        let unwritable = |error: std::io::Error| {
            Error::ImageWrite(format!("cannot remember a map in {}: {error}", self.folder.display()))
        };
        let target = self
            .file(key)
            .ok_or_else(|| Error::ImageWrite(format!("{key:?} is no name for a remembered map")))?;
        let mut bytes = Vec::with_capacity(HEADER + 4 * map.values().len());
        bytes.extend_from_slice(MAGIC);
        for number in [
            VERSION,
            u32::try_from(map.height()).unwrap_or(u32::MAX),
            u32::try_from(map.width()).unwrap_or(u32::MAX),
        ] {
            bytes.extend_from_slice(&number.to_le_bytes());
        }
        for value in map.values() {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        std::fs::create_dir_all(&self.folder).map_err(unwritable)?;
        let temporary = self.folder.join(format!(
            ".{key}.{}-{}.part",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let written = std::fs::write(&temporary, bytes).and_then(|()| std::fs::rename(&temporary, &target));
        if written.is_err() {
            drop(std::fs::remove_file(&temporary)); // best effort: a stray temporary holds nothing a reader takes
        }
        written.map_err(unwritable)
    }
}
