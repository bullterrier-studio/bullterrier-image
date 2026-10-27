"""The photo pipeline driven from Python: build a look with `fx`, see what every step does, run it on a photo.

    uv run python examples/python_api.py

Everything here is also available from the command line (`bt-image run`, `bt-image steps`); this is the same
machinery for when a look is easier to write as code than as a recipe file.
"""

from pathlib import Path

from bullterrier_image import Project, Recipe, fx, imageio, run, trace
from bullterrier_image.sheet import write_stages

HERE = Path(__file__).parent
PHOTO = HERE / "photos" / "bullterrier.jpeg"

# A look is a list of steps. Each step is checked the moment you write it: try misspelling a parameter,
# e.g. fx.rgb_split(amnt=8), and Python tells you at once what the valid ones are.
TOXIC_NEON = Recipe(
    "toxic-neon",
    size=1024,
    steps=[
        fx.sharpen(radius=40, amount=0.6),
        fx.gradient_map(stops=["#021a0b", "#00ff88", "#d8ffe8"]),
        fx.gradient(colors=["#000000"], mask="background"),  # only the background; the subject is found automatically
        fx.slice_shift(preset="glitch", bands=6, amount=51.2, band_min_height=10.24, band_max_height=61.44, seed=3),
        fx.rgb_split(angle=0, amount=8, edge="wrap"),
        fx.bloom(radius=24, amount=0.7, threshold=0.55, octaves=1, opacity=0.9),  # mixed in at 90%
    ],
)


def main(output: Path = HERE / "output" / "python-api") -> None:
    """Show every step of the look on the demo photo, then run it through the pipeline; everything goes in `output`."""
    photo = imageio.load(PHOTO)

    # See what every step does: the photo, the subject cut-out, and the picture after each step.
    stages = trace(photo, TOXIC_NEON)
    for stage in stages:
        print(" ", stage.label)
    print("every step on one sheet:", write_stages(stages, output / "steps"))

    # Or run the look through the whole pipeline, like a recipe file: cached, checked, exported, with a report.
    items = run(Project(photos=(PHOTO,), recipes=(TOXIC_NEON,), output=output / "pipeline", formats=("jpg",)))
    for item in items:
        print(f"{item.photo} x {item.recipe}: {item.status}", *item.warnings)
    print("report:", output / "pipeline" / "index.html")


if __name__ == "__main__":
    main()
