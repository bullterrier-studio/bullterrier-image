//! Writing a file whole or not at all.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// A name no other write of this process shares.
static NEXT: AtomicU64 = AtomicU64::new(0);

/// Writes `bytes` to `target` whole or not at all: into a temporary file beside it (its folder is made if missing), then
/// moved over it (a move within one folder is atomic), so a crash or Ctrl-C never leaves half a file for the next run to
/// trust. A temporary left by a failed write is removed.
pub(crate) fn write_whole(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let folder = target.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(folder)?;
    let name = target
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let temporary = folder.join(format!(
        ".{name}.{}-{}.part",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let written = std::fs::write(&temporary, bytes).and_then(|()| std::fs::rename(&temporary, target));
    if written.is_err() {
        drop(std::fs::remove_file(&temporary)); // best effort: a stray temporary holds nothing a reader takes
    }
    written
}
