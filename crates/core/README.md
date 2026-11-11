# bullterrier-image

The Rust core of [bullterrier-image](https://github.com/bullterrier-studio/bullterrier-image): composable photo effects
as pure functions on float32 RGB pictures, a recipe engine that runs them, and the files, finders and project pipeline
around them. The Python package of the same name is a thin interface over this crate.

A **look** is a `Recipe`, an ordered list of `Step`s; a `Photo` renders it. The same inputs and seed give the same
picture, at any thread count.

```rust
use bullterrier_image::{Options, Photo, Picture, Presets, Recipe, Step};

fn main() -> Result<(), bullterrier_image::Error> {
    let pixels = vec![0.5_f32; 4 * 4 * 3]; // 4 x 4 pixels, RGB, 0..1
    let photo = Photo::new(Picture::new(4, 4, &pixels)?);
    let look = Recipe::new([Step::new("grade").param("exposure", 0.5)]);
    let picture = photo.render(&look, &Presets::shipped()?, Options::default())?;
    println!("{} x {}", picture.width(), picture.height());
    Ok(())
}
```

Effects, their parameters and their documentation are listed by `bullterrier_image::effects()` and
`bullterrier_image::documentation`.

## Feature `onnx`

Off by default. It adds the subject and depth finders that run ONNX models through a runtime library the user has or
fetches at run time (`bt-image install onnx`); nothing is bundled, and it pulls an HTTP client.

## Identity

`pipeline::core_identity()` names this build, so a cache of results can tell when the core changed. In a checkout of the
repository it covers the sources, the data, the manifests and the workspace's `Cargo.lock`. A build from crates.io covers
the crate's own sources, data and manifest: the versions of its dependencies come from your lock file, which a build
script cannot see.

MIT licensed. The bundled font, JetBrains Mono, is under the SIL Open Font License 1.1 (`data/fonts/OFL.txt`).
