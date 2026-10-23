# Changelog

All notable changes to photo-fx are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the version numbers follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0, a minor release
may change the public API.

## [Unreleased]

The first release, 0.1.0.

### Added

- **Effects**, 13 of them, each a pure function on a float RGB picture with a Pydantic parameter model (a description,
  unit, range and default for every field): colour `grade`, `gradient_map`, `hue_shift` and `split_tone`; light
  `clarity` and `glow`; texture `grain`; stylised `halftone`; text `ascii`; `glitch`; subject `fill` (masked to the
  background, this removes it); animation `disperse` and `evaporate`.
- **Recipes**: an ordered list of steps in TOML or Python (`fx.<effect>(...)`), each step optionally masked to the
  subject or background and blended at an opacity. Misspelt names and out-of-range values fail when a recipe loads,
  naming the file, the step, the field and the value.
- **Animation**: any numeric parameter can change over a clip, with named or cubic-bezier easing and holds at both
  ends; output as `.mp4` (H.264, BT.709) or `.gif`, plus a filmstrip PNG.
- **Subject finding**: Apple Vision on macOS, or IS-Net on ONNX Runtime anywhere (the model is downloaded on first
  use and checked against a pinned SHA-256), or a matte file you already have (`--matte`).
- **Pipeline** (`photo-fx run`): many photos times many recipes, cached on disk and redone only when something
  changed, with automatic checks (subject coverage, blank results, frozen animations), several output sizes and
  formats, a `manifest.json` and a browsable `index.html`.
- **Command line**: `render`, `sheet`, `cutout`, `animate`, `run`, `steps` (the picture after every step), `init`
  and `effects` (every parameter, documented).
- **Image files**: any format and mode Pillow reads, EXIF rotation and ICC profiles applied, converted to sRGB;
  saved stills embed an sRGB profile, carry no EXIF or GPS data from the input, and are byte-identical for the same
  picture.
- **Plugins**: third-party packages add effects through the `photo_fx.effects` entry point, written against
  `photo_fx.plugin`.
- **Errors**: every mistake a caller, recipe or photo can make raises a `PhotoFxError` subclass; the command line
  prints one line for it, never a traceback.
- Python 3.12, 3.13 and 3.14 on macOS, Linux and Windows.

[Unreleased]: https://github.com/jcakstins/photo-fx/commits/main
