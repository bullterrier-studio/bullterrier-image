"""The native answers the WebAssembly package is compared with: for each case of cases.json, the SHA-256 and the size of
the pixels the Python package renders, written with the photo (RGBA bytes) and the `subject` map the cases hand in.

    python tests/wasm/oracle.py pkg/oracle

Writes OUT/oracle.json, OUT/photo.rgba and OUT/subject.f32. The pixels are those of the 8-bit PNG the package writes
for the render, so the native rounding to 8 bits is the oracle's too.
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


def subject_map(height: int, width: int) -> np.ndarray:
    """A soft ellipse in the middle: a stand-in for a cut-out made elsewhere."""
    y, x = np.mgrid[0:height, 0:width].astype(np.float32)
    distance = np.hypot((x - width / 2) / (width * 0.35), (y - height / 2) / (height * 0.4))
    return np.clip(1.5 - distance, 0.0, 1.0).astype(np.float32)


def native_pixels(photo: np.ndarray, case: dict, subject: np.ndarray, scratch: Path) -> tuple[int, int, bytes]:
    """(width, height, RGBA bytes) the Python package renders `case` to, through a PNG file written at `scratch`."""
    recipe = load_recipe(case["look"] if "look" in case else HERE / case["recipe"])
    maps = {"subject": subject} if "subject" in case.get("maps", []) else None
    imageio.save(render(photo, recipe, size=case.get("size"), maps=maps), scratch)
    with Image.open(scratch) as png:
        rgba = png.convert("RGBA")
    return rgba.width, rgba.height, rgba.tobytes()


def main(out: Path) -> None:
    """Write the oracle files into `out`."""
    out.mkdir(parents=True, exist_ok=True)
    with Image.open(PHOTO) as opened:
        opened = opened.convert("RGB")
        scale = SIDE / max(opened.size)
        small = opened.resize((round(opened.width * scale), round(opened.height * scale)), Image.Resampling.LANCZOS)
    rgb = np.asarray(small, dtype=np.uint8)
    height, width = rgb.shape[:2]
    rgba = np.dstack([rgb, np.full((height, width), 255, np.uint8)])
    subject = subject_map(height, width)
    (out / "photo.rgba").write_bytes(rgba.tobytes())
    (out / "subject.f32").write_bytes(subject.astype("<f4").tobytes())
    answers = {"photo": {"width": width, "height": height}, "cases": {}}
    for case in json.loads((HERE / "cases.json").read_text(encoding="utf-8")):
        out_width, out_height, pixels = native_pixels(rgb, case, subject, out / "native.png")
        answers["cases"][case["name"]] = {
            "width": out_width,
            "height": out_height,
            "sha256": hashlib.sha256(pixels).hexdigest(),
        }
    (out / "native.png").unlink()
    (out / "oracle.json").write_text(json.dumps(answers, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main(Path(sys.argv[1]))
