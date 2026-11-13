"""The photo pipeline driven from Python: build a look with `fx`, see what every step does, run it on a photo.

    uv run python examples/python_api.py

Everything here is also available from the command line (`bt-image run`, `bt-image steps`); this is the same
machinery for when a look is easier to write as code than as a recipe file.
"""

from pathlib import Path

import numpy as np

from bullterrier_image import Band, Project, Recipe, fx, imageio, region_map, run, trace

HERE = Path(__file__).parent
PHOTO = HERE / "photos" / "bullterrier.jpeg"

# A look is a list of steps. Each step is checked the moment you write it: try misspelling a parameter,
# e.g. fx.rgb_split(amnt=8), and Python tells you at once what the valid ones are.
TOXIC_NEON = Recipe(
    [
        fx.sharpen(radius=18.75, amount=0.6),
        fx.gradient_map(stops=["#021a0b", "#00ff88", "#d8ffe8"]),
        fx.gradient(colors=["#000000"], mask="background"),  # only the background; the subject is found automatically
        fx.slice_shift(preset="glitch", bands=6, amount=24, band_min_height=4.8, band_max_height=28.8, seed=3),
        fx.rgb_split(angle=0, amount=3.75, edge="wrap"),
        fx.bloom(radius=11.25, amount=0.7, threshold=0.55, octaves=1, opacity=0.9),  # mixed in at 90%
    ],
    name="toxic-neon",
    size=1024,
)

# A region is a map you can ask for, used by a step or not: how much each pixel is chosen, float32 (h, w) in 0..1.
BRIGHTS = Recipe([fx.grade()], regions={"brights": Band(of="light", low=0.7, high=1.0, feather=0.1)})


def main(output: Path = HERE / "output" / "python-api") -> None:
    """Show every step of the look on the demo photo, then run it through the pipeline; everything goes in `output`."""
    photo = imageio.load(PHOTO)

    # See what every step does: the photo, the subject cut-out, and the picture after each step.
    stages = trace(photo, TOXIC_NEON)
    for stage in stages:
        print(" ", stage.label)
    (output / "steps").mkdir(parents=True, exist_ok=True)
    for number, stage in enumerate(stages):  # `bt-image steps` also lays them all out on one sheet
        imageio.save(stage.image, output / "steps" / f"{number:02d}.{'png' if stage.image.shape[2] == 4 else 'jpg'}")

    # One region of a look as an array, at the size a render of it works at (here 480 px), and what it chooses.
    brights = region_map(photo, BRIGHTS, "brights", size=480)
    imageio.save(np.repeat(brights[..., None], 3, axis=2), output / "brights.jpg")
    print(f"  region 'brights': {brights.shape}, {brights.mean():.0%} of the picture chosen")

    # Or run the look through the whole pipeline, like a recipe file: cached, checked, exported, with a report.
    items = run(Project(photos=(PHOTO,), recipes=(TOXIC_NEON,), output=output / "pipeline", formats=("jpg",)))
    for item in items:
        print(f"{item.photo} x {item.recipe}: {item.status}", *item.warnings)
    print("report:", output / "pipeline" / "index.html")


if __name__ == "__main__":
    main()
