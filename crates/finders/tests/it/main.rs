//! The integration tests of the finders crate: one binary, a module per area (the layout rule of the core's `tests/it`).

mod identity;
mod segmenter;
#[cfg(target_os = "macos")]
mod vision;
