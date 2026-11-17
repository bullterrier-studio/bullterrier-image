//! Saving a picture into a folder that may not exist yet.

use std::path::Path;

use crate::files::{WriteOptions, write_with};
use crate::{Error, Image};

/// `image` written to `target` (its folder made if missing), JPEG at `quality`.
///
/// # Errors
/// [`Error::ImageWrite`] naming the folder if it cannot be made; the errors of [`write_with`].
pub(super) fn save_image(target: &Path, image: &Image, quality: u8) -> Result<(), Error> {
    if let Some(folder) = target.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|error| Error::ImageWrite(format!("cannot write to the folder {}: {error}", folder.display())))?;
    }
    write_with(target, image, &WriteOptions::new().with_quality(quality))
}
