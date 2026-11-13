# Subject and depth

Models, backends, your own finders and maps. Basics are in the [README](../README.md).

**Subject** (cut-outs, `mask = "background"`, anything needing a silhouette) has three backends:

| Backend | Where | Notes |
|---|---|---|
| `vision`: Apple Vision subject segmentation | macOS | Default on macOS. Built in: no download, Neural Engine. Cleaner cut than the portable model. |
| `onnx`: [IS-Net](https://github.com/xuebinqin/DIS) (general use, Apache-2.0) on ONNX Runtime | macOS, Linux, Windows | Default off macOS. People, pets, objects. Few seconds per photo on laptop CPU. Softer at fine details. |
| `sam2`: [SAM 2.1 Hiera Large](https://github.com/facebookresearch/sam2) (Apache-2.0 weights and code) on ONNX Runtime | macOS, Linux, Windows | Never a default. Finds what you click, not what stands out: a building, one of three people, a lantern. Needs `points` or a `box`. About 6 s per photo and 2.6 GB on a laptop CPU. |

Portable model (178 MB) not in package. First cut-out off macOS (or with `--segmenter onnx`) that needs it downloads it once into cache folder
(`~/Library/Caches/bt-image` macOS, `$XDG_CACHE_HOME/bt-image` or `~/.cache/bt-image` Linux, `%LOCALAPPDATA%\bt-image\Cache`
Windows), checks SHA-256 against the one pinned in code, keeps it. Nothing downloads at import. Offline or download blocked:
fetch the file the error names, save where it says, or set `BT_IMAGE_SUBJECT_MODEL=/models/isnet-general-use.onnx` (used as
is, never downloaded).

Pick backend: `--segmenter onnx|vision` (Python: `finders={"subject": subject_finder("vision")}` from
`bullterrier_image.finding.subject`) or `BT_IMAGE_SEGMENTER=onnx` for every run. Option beats variable. No model needed if
matte already exists: `--map subject=cutout.png` (transparent PNG = its alpha, black-and-white mask: white = subject, photo's
size); Python: `maps={"subject": imageio.load_map("cutout.png")}`. New backend = new file in
[`finding/`](../src/bullterrier_image/finding), nothing else changes. Python newer than 3.14: ONNX Runtime has no release yet,
so it installs without it; `onnx` backend then says it is missing, `--map subject=FILE` (or `--segmenter vision` on macOS)
still works.

**Pointing at it (`sam2`).** Clicks are shares of the photo (0,0 top left, 1,1 bottom right), positive (on the thing) or
negative (on something to leave out), plus optionally one box. No clicks and no box is an error naming `points`: it does
not guess. Any other segmenter given clicks is an error too.

```python
from bullterrier_image.finding.subject import subject_finder, subject_matte

lantern = subject_matte(photo, segmenter="sam2", points=[(0.26, 0.14, True)])
render(
    photo,
    look,
    finders={
        "subject": subject_finder("sam2", points=[(0.5, 0.6, True), (0.1, 0.7, False)], box=(0.2, 0.05, 0.85, 0.93))
    },
)
```

```
bt-image cutout photo.jpg --segmenter sam2 --point 0.26,0.14            # positive click
bt-image render photo.jpg look.toml --segmenter sam2 --point 0.5,0.6 --avoid 0.1,0.7 --box 0.2,0.05,0.85,0.93
```

The model picks the best of its candidate masks; its logits are stretched to the photo and made a probability (sigmoid), then
the edge is cleaned like any subject (`shrink`, `feather`). Nothing at the clicks is `SubjectNotFoundError`. The clicks belong
to one photo, so they are not in a recipe: they go with the finder, and its identity holds them exactly, so `bt-image run`
and `pipeline.run` find the subject again when a click moves and not otherwise. `bt-image run` takes no clicks (one set for
every photo makes no sense). With a `subject` region using `per`, the finder sees crops: do not use `sam2` there.
Models (890 MB encoder, 22 MB decoder, each with a weights file beside it) download like the others, or
`bt-image install onnx --prompted`; there is no local-file variable (it is two files): offline, save the files the error names
into the cache folder.

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

**Glyph effects** (`ascii`, `disperse`) draw with bundled monospaced font (JetBrains Mono, SIL OFL 1.1, licence in
the wheel), not a system font: same look on every machine.
