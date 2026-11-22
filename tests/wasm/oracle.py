"""The native answers the WebAssembly package is compared with: for each case of cases.json and each example recipe
of an effect, the SHA-256 and the size of the pixels the Python package renders (or the error the package must raise:
the recipes that read a file), written with the photo (RGBA bytes) and the `subject` and `depth` maps every render is
handed.

    python tests/wasm/oracle.py pkg

Writes OUT/oracle.json (each case as it was given, with its answer), OUT/photo.rgba, OUT/subject.f32, OUT/depth.f32
and the pixels OUT/<case>.rgba of each case that may differ by a level (`level_bound`).
The pixels are those of the 8-bit PNG the package writes for the render, so the native rounding to 8 bits is the
oracle's too.
"""

import hashlib
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

from bullterrier_image import imageio, load_recipe, render

HERE = Path(__file__).parent
PHOTO = HERE.parent.parent / "examples" / "photos" / "bullterrier.jpeg"
SIDE = 480  # the photo's longest side: small, so the Node test runs in seconds
# the example recipes that read a file: a browser has no file system, so the package must refuse them as named
NEEDS_FILES = {"blend": "ImageReadError", "color_transfer": "ImageReadError", "lut": "LutError"}
# Effects that filter through the Fourier transform. rustfft runs a different SIMD routine on each architecture (NEON,
# AVX with fused multiply-add, WebAssembly SIMD), each rounding the f32 sums its own way, so a render that goes through
# one can differ from the native one by one level of one channel (measured, of 288000 bytes: on arm64 bloom,
# engraving, focus_blur, lens_blur and starburst, 1 to 4; on x86-64 Linux engraving, focus_blur, lens_blur and
# starburst, 1 to 5, and ambient_occlusion 2, gaussian_blur 2 and sharpen 3, whose example recipes blur past the reach
# the transform takes over at); only the scalar routine matches all of them, and costs these effects 1.4 to 1.6 times
# as long natively. Every other effect renders to the same bytes.
FOURIER_EFFECTS = (
    "ambient_occlusion",
    "bloom",
    "engraving",
    "focus_blur",
    "gaussian_blur",
    "lens_blur",
    "sharpen",
    "starburst",
)
EXAMPLE_SIZE = 300  # every example recipe renders at this size: small, and large enough to show a difference


def depth_map(height: int, width: int) -> np.ndarray:
    """Nearest at the bottom, farthest at the top: a stand-in for a depth estimate made elsewhere."""
    return np.repeat(np.linspace(0.0, 1.0, height, dtype=np.float32)[:, None], width, axis=1)


def subject_map(height: int, width: int) -> np.ndarray:
    """A soft ellipse in the middle: a stand-in for a cut-out made elsewhere."""
    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    distance = np.hypot((x - width / 2) / (width * 0.35), (y - height / 2) / (height * 0.4))
    return np.clip(1.5 - distance, 0.0, 1.0).astype(np.float32)


def all_cases() -> list[dict]:
    """The cases of cases.json, then each example recipe of an effect at `EXAMPLE_SIZE`."""
    cases = json.loads((HERE / "cases.json").read_text(encoding="utf-8"))
    for path in sorted((HERE.parent.parent / "examples" / "effects").glob("*.toml")):
        cases.append(
            {"name": f"example-{path.stem}", "recipe": f"../../examples/effects/{path.name}", "size": EXAMPLE_SIZE}
        )
    return cases


def level_bound(case: dict) -> int:
    """How many levels of 255 a channel of the package's render may differ from the native one: 1 for a recipe that
    uses a Fourier effect (`FOURIER_EFFECTS`), else 0 (the same bytes)."""
    if "recipe" not in case:
        return 0
    text = (HERE / case["recipe"]).read_text(encoding="utf-8")
    return int(any(f'effect = "{name}"' in text for name in FOURIER_EFFECTS))


def native_pixels(photo: np.ndarray, case: dict, maps: dict[str, np.ndarray], scratch: Path) -> tuple[int, int, bytes]:
    """(width, height, RGBA bytes) the Python package renders `case` to, through a PNG file written at `scratch`."""
    recipe = load_recipe(case["look"] if "look" in case else HERE / case["recipe"])
    imageio.save(render(photo, recipe, size=case.get("size"), maps=maps), scratch)
    with Image.open(scratch) as png:
        rgba = png.convert("RGBA")
    return rgba.width, rgba.height, rgba.tobytes()


def main(out: Path) -> None:
    """Write the oracle files into `out`."""
    out.mkdir(parents=True, exist_ok=True)
    with Image.open(PHOTO) as opened:
        full = opened.convert("RGB")
    scale = SIDE / max(full.size)
    small = full.resize((round(full.width * scale), round(full.height * scale)), Image.Resampling.LANCZOS)
    rgb = np.asarray(small, dtype=np.uint8)
    height, width = rgb.shape[:2]
    rgba = np.dstack([rgb, np.full((height, width), 255, np.uint8)])
    maps = {"subject": subject_map(height, width), "depth": depth_map(height, width)}
    (out / "photo.rgba").write_bytes(rgba.tobytes())
    for name, values in maps.items():
        (out / f"{name}.f32").write_bytes(values.astype("<f4").tobytes())
    answers = {"photo": {"width": width, "height": height}, "cases": {}}
    for case in all_cases():
        stem = case["name"].removeprefix("example-")
        answer: dict[str, str | int]
        if stem in NEEDS_FILES and case["name"].startswith("example-"):
            answer = {"error": NEEDS_FILES[stem]}
        else:
            out_width, out_height, pixels = native_pixels(rgb, case, maps, out / "native.png")
            bound = level_bound(case)
            answer = {
                "width": out_width,
                "height": out_height,
                "sha256": hashlib.sha256(pixels).hexdigest(),
                "bound": bound,
            }
            if bound:  # the Node test compares these pixel by pixel
                (out / f"{case['name']}.rgba").write_bytes(pixels)
        answers["cases"][case["name"]] = {**case, **answer}
    (out / "native.png").unlink()
    (out / "oracle.json").write_text(json.dumps(answers, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main(Path(sys.argv[1]))
