# Subject and depth

Models, backends, your own finders and maps. Basics are in the [README](../README.md).

**Subject** (cut-outs, `mask = "background"`, anything needing a silhouette) has two backends:

| Backend | Where | Notes |
|---|---|---|
| `onnx`: [IS-Net](https://github.com/xuebinqin/DIS) (general use, Apache-2.0) on ONNX Runtime | macOS, Linux, Windows | Default everywhere: same cut on every platform. People, pets, objects. Few seconds per photo on laptop CPU. |
| `vision`: Apple Vision subject segmentation | macOS | Only when asked (`--segmenter vision`, `BT_IMAGE_SEGMENTER=vision`). Built in: no download, Neural Engine. Cut differs from portable model. |

Portable model (178 MB) not in package. First cut-out that needs it downloads it once into cache folder
(`~/Library/Caches/bt-image` macOS, `$XDG_CACHE_HOME/bt-image` or `~/.cache/bt-image` Linux, `%LOCALAPPDATA%\bt-image\Cache`
Windows), checks SHA-256 against the one pinned in code, keeps it. Nothing downloads at import. Offline or download blocked:
fetch the file the error names, save where it says, or set `BT_IMAGE_SUBJECT_MODEL=/models/isnet-general-use.onnx` (used as
is, never downloaded).

Pick backend: `--segmenter onnx|vision` (Python: `finders={"subject": subject_finder("vision")}` from
`bullterrier_image.finding.subject`) or `BT_IMAGE_SEGMENTER=vision` for every run. Option beats variable. No model needed if
matte already exists: `--map subject=cutout.png` (transparent PNG = its alpha, black-and-white mask: white = subject, photo's
size); Python: `maps={"subject": imageio.load_map("cutout.png")}`. New backend = new file in
[`finding/`](../src/bullterrier_image/finding), nothing else changes. Python newer than 3.14: ONNX Runtime has no release yet,
so it installs without it; `onnx` backend then says it is missing, `--map subject=FILE` (or `--segmenter vision` on macOS)
still works.

**Own finder** = `Finder`: function photo (float32 `(h, w, 3)`, 0..1) -> matte (`(h, w)`, 0..1), plus identity naming its
answers. `bt-image run` remembers subjects by identity: change it whenever answers could change (other model, prompt, option).

```python
from functools import partial
from bullterrier_image import Finder, render

render(photo, look, finders={"subject": Finder("grounded-sam dog v1", partial(grounded, prompt="dog"))})
```

**Depth** = second map, found like subject. A recipe that takes depth (an effect, or a region made from it) runs
[Depth Anything V2 Small](https://github.com/DepthAnything/Depth-Anything-V2) (Apache-2.0) on ONNX Runtime, every platform.
Weights (99 MB, fp32 ONNX export of `onnx-community/depth-anything-v2-small`) download on first use into the same cache
folder, SHA-256 checked. Offline: `BT_IMAGE_DEPTH_MODEL` points at a copy (used as is). Result is relative depth, scaled per
photo: nearest pixel 1, farthest 0, flat answer all 0. One value is not the same distance in two photos. To scale photos
alike: `inverse_depth(rgb)` (larger = nearer, model's own scale), scale with one range of your own, hand in as
`maps={"depth": ...}`. Own map replaces model: `--map depth=depth.png` (8 or 16 bit greyscale, photo's size, white =
nearest). Own finder: `finders={"depth": Finder("my-depth v1", find)}`, `find` returns `(h, w)` 0..1, 1 = nearest.
`pipeline.run` remembers each photo's depth by that identity, apart from subject.

**Maps of your own.** Subject and depth are two *maps*: one value per pixel, 0..1, photo's size, known by name. A recipe names
its own with a `given` region (`format = 3`) and uses it like any selection. Caller hands it in by name: picture
(`--map star=star.png`; transparency gives its alpha, otherwise brightness), array, or a `Finder`:

```toml
format = 3

[region.star]
kind = "given"

[[step]]
effect = "bloom"
mask = "star"
```

```python
render(photo, look, maps={"star": star_alpha})  # an array of the photo's size, float32 0..1
render(photo, look, finders={"star": Finder("star v1", find)})  # or found once per photo
```

A map moves with the photo through framing (flip, turn, crop, border; a border is 0 in every map): it only has to match the
photo as given. Name no recipe declares (`maps={"strar": ...}`) -> refused, valid names listed. Recipe uses a map nobody gave
-> `MapNotFoundError` (`SubjectNotFoundError`, `DepthNotFoundError` for subject, depth). A project (`bt-image run`) holds no
maps of its own: a recipe with a `given` region needs `finders=` in `bullterrier_image.run`.

**Glyph effects** (`ascii`, `evaporate`, `disperse`) draw with bundled monospaced font (JetBrains Mono, SIL OFL 1.1, licence in
the wheel), not a system font: same look on every machine.
