//! Apple Vision's subject segmentation: on macOS 14 and later through `objc2`, elsewhere a [`Vision`] that cannot be made.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::Vision;
#[cfg(target_os = "macos")]
pub(crate) use macos::supported;

#[cfg(not(target_os = "macos"))]
mod elsewhere {
    use bullterrier_image::{Error, Finder, Layer, Picture};

    /// The subject finder of Apple Vision: macOS 14 and later only.
    #[derive(Clone, Copy, Debug)]
    pub struct Vision;

    impl Vision {
        /// The Vision finder.
        ///
        /// # Errors
        /// [`Error::BackendUnavailable`] naming the fix: this is not macOS.
        pub fn new() -> Result<Self, Error> {
            Err(Error::BackendUnavailable(
                "Apple Vision is macOS only: choose the portable model instead (segmenter 'onnx', or \\
                 BT_IMAGE_SEGMENTER=onnx), or give the subject map yourself (--map subject=FILE)"
                    .to_owned(),
            ))
        }
    }

    impl Vision {
        /// There is no macOS here, so no version: never reached, since `new` fails.
        #[must_use]
        pub fn macos_version() -> String {
            String::new()
        }

        /// This finder with the edge clean-up set to `shrink` and `feather` px.
        #[must_use]
        pub const fn with_edge(self, _shrink: f64, _feather: f64) -> Self {
            self
        }
    }

    impl Finder for Vision {
        fn find(&self, _picture: &Picture<'_>) -> Result<Layer, Error> {
            Err(Self::new()
                .err()
                .unwrap_or_else(|| Error::BackendUnavailable("Apple Vision is macOS only".to_owned())))
        }
    }

    pub(crate) const fn supported() -> bool {
        false
    }
}

#[cfg(not(target_os = "macos"))]
pub use elsewhere::Vision;
#[cfg(not(target_os = "macos"))]
pub(crate) use elsewhere::supported;
