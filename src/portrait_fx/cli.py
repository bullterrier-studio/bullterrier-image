"""Command line: `portrait-fx render|sheet|cutout|effects`. A thin layer over the library; no logic of its own."""

import argparse
import sys
import textwrap
from pathlib import Path

from . import effects, imageio
from .imageio import save_video
from .matte import subject_matte
from .recipe import Recipe, load_recipe, render_all, render_frames
from .sheet import contact_sheet


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="portrait-fx", description="Cut out a portrait and style it with effects.")
    commands = parser.add_subparsers(dest="command", required=True)

    p = commands.add_parser("render", help="render a photo through one or more recipes, one file each")
    _photo_and_recipes(p)
    p.add_argument("-o", "--out", default="out", help="output folder (default: out)")
    p.add_argument("--format", default="png", choices=["png", "jpg", "webp"])

    p = commands.add_parser("sheet", help="render recipes and put them side by side in one comparison image")
    _photo_and_recipes(p)
    p.add_argument("-o", "--out", default="out/sheet.png", help="output file (default: out/sheet.png)")
    p.add_argument("--columns", type=int, default=3)

    p = commands.add_parser("cutout", help="save just the person on a transparent background")
    p.add_argument("photo")
    p.add_argument("-o", "--out", default="out/cutout.png")
    p.add_argument("--shrink", type=float, default=None, help="pull the cut edge in by this many px")
    p.add_argument("--feather", type=float, default=None, help="soften the cut edge by this many px")

    p = commands.add_parser("animate", help="render an animated recipe to a video (.mp4, .mov) or a .gif")
    p.add_argument("photo")
    p.add_argument("recipe", help="a recipe .toml with an [animation] table")
    p.add_argument("-o", "--out", help="output file (default: out/<photo>__<recipe>.mp4)")
    p.add_argument(
        "--max-side", type=int, help="shrink the video so its longest side is at most this many px (e.g. 1080)"
    )

    commands.add_parser("effects", help="list the available effects and their parameters")

    args = parser.parse_args(argv)
    try:
        return {"render": _render, "sheet": _sheet, "cutout": _cutout, "animate": _animate, "effects": _effects}[
            args.command
        ](args)
    except (ValueError, RuntimeError, FileNotFoundError, NotImplementedError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


def _photo_and_recipes(p: argparse.ArgumentParser) -> None:
    p.add_argument("photo")
    p.add_argument("recipes", nargs="+", help="recipe .toml files, or folders of them")


def _load_recipes(paths: list[str]) -> list[Recipe]:
    files: list[Path] = []
    for path in map(Path, paths):
        files += sorted(path.glob("*.toml")) if path.is_dir() else [path]
    if not files:
        raise FileNotFoundError("no recipe files found")
    return [load_recipe(f) for f in files]


def _render_all(args) -> tuple[list[Recipe], list]:
    recipes = _load_recipes(args.recipes)  # validated before any slow work starts
    print(f"rendering {len(recipes)} recipe(s)...", file=sys.stderr)
    return recipes, render_all(imageio.load(args.photo), recipes)


def _render(args) -> int:
    recipes, results = _render_all(args)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for recipe, image in zip(recipes, results, strict=True):
        target = out / f"{Path(args.photo).stem}__{recipe.name}.{args.format}"
        imageio.save(image, target)
        print(target)
    return 0


def _sheet(args) -> int:
    recipes, results = _render_all(args)
    target = Path(args.out)
    target.parent.mkdir(parents=True, exist_ok=True)
    imageio.save(contact_sheet([(r.name, img) for r, img in zip(recipes, results, strict=True)], args.columns), target)
    print(target)
    return 0


def _cutout(args) -> int:
    photo = imageio.load(args.photo)
    options = {k: v for k, v in (("shrink", args.shrink), ("feather", args.feather)) if v is not None}
    alpha = subject_matte(photo, **options)
    target = Path(args.out)
    target.parent.mkdir(parents=True, exist_ok=True)
    imageio.save(photo, target, alpha=alpha)
    print(target)
    return 0


def _animate(args) -> int:
    recipe = load_recipe(args.recipe)
    if recipe.animation is None:
        raise ValueError(f"{Path(args.recipe).name} has no [animation] table, so there is nothing to animate")
    target = Path(args.out) if args.out else Path("out") / f"{Path(args.photo).stem}__{recipe.name}.mp4"
    target.parent.mkdir(parents=True, exist_ok=True)
    total = len(recipe.animation.positions())

    def with_progress(frames):
        for i, frame in enumerate(frames, start=1):
            if i % 10 == 0 or i == total:
                print(f"frame {i}/{total}", file=sys.stderr)
            yield frame

    save_video(
        with_progress(render_frames(imageio.load(args.photo), recipe)),
        target,
        fps=recipe.animation.fps,
        max_side=args.max_side,
    )
    print(target)
    return 0


def _effects(args) -> int:
    for name, doc in effects.describe().items():
        print(f"{name}\n{textwrap.indent(textwrap.fill(' '.join(doc.split()), 96), '    ')}\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
