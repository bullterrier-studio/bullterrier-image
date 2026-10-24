"""Rebuild docs/images/banner.jpg, the README's header: the original photo, then what each recipe made of it.

uv run bt-image run examples/bt-image.toml && uv run python docs/make_banner.py
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

from bullterrier_image import imageio

ROOT = Path(__file__).parent.parent
RESULTS = ROOT / "examples" / "output" / "bullterrier"
LOOKS = ["neon-pulse", "glitch", "ascii-colour", "halftone-print", "ember-glitch"]
TILE_HEIGHT, GAP, MARGIN, LABEL = 620, 18, 36, 44
BACKGROUND, TEXT, MUTED = (13, 14, 18), (232, 234, 240), (130, 135, 148)


def tile(path: Path) -> Image.Image:
    """The picture at `path`, scaled to the banner's tile height with its proportions kept."""
    picture = imageio.to_pil(imageio.load(path))
    return picture.resize((round(picture.width * TILE_HEIGHT / picture.height), TILE_HEIGHT), Image.Resampling.LANCZOS)


def main() -> None:
    """Write docs/images/banner.jpg from the demo's results (run the demo pipeline first) and print its path."""
    original = tile(ROOT / "examples" / "photos" / "bullterrier.jpeg")
    results = [(name, tile(RESULTS / f"{name}.jpg")) for name in LOOKS]
    arrow = 90
    width = MARGIN * 2 + original.width + arrow + sum(t.width for _, t in results) + GAP * (len(results) - 1)
    height = MARGIN * 2 + TILE_HEIGHT + LABEL
    canvas = Image.new("RGB", (width, height), BACKGROUND)
    draw = ImageDraw.Draw(canvas)
    font = ImageFont.load_default(size=22)
    small = ImageFont.load_default(size=18)

    x, y = MARGIN, MARGIN
    canvas.paste(original, (x, y))
    draw.text((x, y + TILE_HEIGHT + 12), "original photo", fill=MUTED, font=font)
    x += original.width

    middle = y + TILE_HEIGHT // 2  # an arrow from the photo to the results
    draw.line([(x + 22, middle), (x + arrow - 26, middle)], fill=MUTED, width=4)
    draw.polygon([(x + arrow - 26, middle - 14), (x + arrow - 8, middle), (x + arrow - 26, middle + 14)], fill=MUTED)
    x += arrow

    for name, picture in results:
        canvas.paste(picture, (x, y))
        draw.text((x, y + TILE_HEIGHT + 12), name, fill=TEXT, font=font)
        x += picture.width + GAP
    draw.text(
        (width - MARGIN - 390, height - 28), "one photo, five recipes, under 30 lines each", fill=MUTED, font=small
    )
    out = ROOT / "docs" / "images" / "banner.jpg"
    canvas.save(out, quality=90, subsampling=0)
    print(out, canvas.size, f"{out.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
