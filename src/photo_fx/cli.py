"""Command line: `photo-fx render|sheet|cutout|animate|run|steps|init|effects|gallery|regenerate|new-effect`.

A thin layer over the library; no logic of its own.
"""

import argparse
import sys
import textwrap
from collections.abc import Iterable, Iterator
from dataclasses import replace
from functools import partial
from pathlib import Path

import numpy as np

from . import __version__, gallery, imageio
from .core import registry
from .imageio import save_video
from .matte import SEGMENTERS, refine_edge, subject_matte
from .pipeline import init_project, load_project, run
from .recipe import Recipe, SubjectFinder, load_recipe, render_all, render_frames, trace
from .sheet import contact_sheet, write_stages


def main(argv: list[str] | None = None) -> int:
    """Run the command line with `argv` (default: `sys.argv[1:]`) and return the exit code.

    0 on success; 1 when a command fails (the reason is printed to stderr as `error: ...`, never as a traceback), when
    `run` has a failed result, or with `run --strict` a warning. A usage error exits through argparse with code 2.
    """
    parser = argparse.ArgumentParser(
        prog="photo-fx",
        description="Style photos with composable effects, optionally on a cut-out subject, and animate them.",
    )
    parser.add_argument("--version", action="version", version=f"photo-fx {__version__}")
    commands = parser.add_subparsers(dest="command", required=True)

    p = commands.add_parser("render", help="render a photo through one or more recipes, one file each")
    _photo_and_recipes(p)
    _subject_options(p)
    p.add_argument("-o", "--out", default="out", help="output folder (default: out)")
    p.add_argument("--format", default="png", choices=["png", "jpg", "webp"])

    p = commands.add_parser("sheet", help="render recipes and put them side by side in one comparison image")
    _photo_and_recipes(p)
    _subject_options(p)
    p.add_argument("-o", "--out", default="out/sheet.png", help="output file (default: out/sheet.png)")
    p.add_argument("--columns", type=int, default=3)

    p = commands.add_parser("cutout", help="save just the subject on a transparent background")
    p.add_argument("photo")
    p.add_argument("-o", "--out", default="out/cutout.png")
    p.add_argument("--shrink", type=float, default=None, help="pull the cut edge in by this many px")
    p.add_argument("--feather", type=float, default=None, help="soften the cut edge by this many px")
    _subject_options(p)

    p = commands.add_parser("animate", help="render an animated recipe to a video (.mp4, .mov) or a .gif")
    p.add_argument("photo")
    p.add_argument("recipe", help="a recipe .toml with an [animation] table")
    p.add_argument("-o", "--out", help="output file (default: out/<photo>__<recipe>.mp4)")
    p.add_argument(
        "--max-side", type=int, help="shrink the video so its longest side is at most this many px (e.g. 1080)"
    )
    _subject_options(p)

    p = commands.add_parser(
        "run", help="process every photo of a project with every recipe: cached, checked, with a report"
    )
    p.add_argument("project", nargs="?", default="photo-fx.toml", help="project file (default: photo-fx.toml)")
    p.add_argument("--force", action="store_true", help="redo everything, even what is up to date")
    p.add_argument("--dry-run", action="store_true", help="show what would be done and write nothing")
    p.add_argument("--only", metavar="PATTERN", help="only the photos whose file name matches, e.g. 'dog*.jpg'")
    p.add_argument("--strict", action="store_true", help="exit with an error if any result has a warning")

    p.add_argument("--steps", action="store_true", help="also save the picture after every step of every recipe")
    _segmenter_option(p, " (subjects found before are remembered in the output's .cache: delete it after switching)")

    p = commands.add_parser("steps", help="show what every step of a recipe does to a photo: one picture per step")
    p.add_argument("photo")
    p.add_argument("recipe")
    p.add_argument("-o", "--out", help="output folder (default: out/steps/<photo>__<recipe>)")
    p.add_argument(
        "--at", type=float, default=0.5, help="for an animated recipe: the moment of the clip, 0..1 (default 0.5)"
    )
    _subject_options(p)

    p = commands.add_parser("init", help="create a starter photo-fx.toml, a photos/ folder and two recipes")
    p.add_argument("directory", nargs="?", default=".")

    commands.add_parser("effects", help="list the available effects and their parameters")

    p = commands.add_parser(
        "gallery",
        help="render each effect's missing gallery picture from its example recipe and write docs/effects.md",
    )
    p.add_argument("--only", nargs="+", default=[], metavar="NAME", help="only these effects")
    p.add_argument("--force", action="store_true", help="render again even if the picture exists")

    commands.add_parser(
        "regenerate", help="after adding or changing effects: write fx.pyi and docs/effects.md, render missing pictures"
    )

    p = commands.add_parser(
        "new-effect", help="create a new effect's files from the template: its code, its tests and its example recipe"
    )
    p.add_argument("name", help="the effect's name: lowercase letters, digits and _, starting with a letter")
    p.add_argument("--category", required=True, choices=registry.CATEGORIES, help="its section of the catalogue")

    args = parser.parse_args(argv)
    try:
        return {
            "render": _render,
            "sheet": _sheet,
            "cutout": _cutout,
            "animate": _animate,
            "run": _run,
            "steps": _steps,
            "init": _init,
            "effects": _effects,
            "gallery": lambda args: _print_paths(gallery.write_docs(args.only, args.force)),
            "regenerate": lambda args: _print_paths(gallery.regenerate()),
            "new-effect": _new_effect,
        }[args.command](args)
    except (ValueError, RuntimeError, FileNotFoundError, FileExistsError, NotImplementedError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


def _photo_and_recipes(p: argparse.ArgumentParser) -> None:
    p.add_argument("photo")
    p.add_argument("recipes", nargs="+", help="recipe .toml files, or folders of them")


def _subject_options(p: argparse.ArgumentParser) -> None:
    p.add_argument(
        "--matte",
        metavar="PICTURE",
        help="the subject's matte, the photo's size: a cut-out PNG (its transparency) or a black-and-white mask "
        "(white = subject); no model is used",
    )
    _segmenter_option(p, "")


def _segmenter_option(p: argparse.ArgumentParser, note: str) -> None:
    p.add_argument(
        "--segmenter",
        choices=SEGMENTERS,
        help="what finds the subject: vision (Apple Vision, macOS) or onnx (a portable model, downloaded on first "
        f"use); default: $PHOTO_FX_SEGMENTER, else vision where available{note}",
    )


def _subject(args: argparse.Namespace) -> tuple[np.ndarray | None, SubjectFinder]:
    """The subject as the command line gives it: a matte file (--matte), and how to find it otherwise (--segmenter)."""
    matte = imageio.load_matte(args.matte) if args.matte else None
    return matte, partial(subject_matte, segmenter=args.segmenter)


def _load_recipes(paths: list[str]) -> list[Recipe]:
    files: list[Path] = []
    for path in map(Path, paths):
        files += sorted(path.glob("*.toml")) if path.is_dir() else [path]
    if not files:
        raise FileNotFoundError("no recipe files found")
    return [load_recipe(f) for f in files]


def _render_all(args: argparse.Namespace) -> tuple[list[Recipe], list[np.ndarray]]:
    recipes = _load_recipes(args.recipes)  # validated before any slow work starts
    print(f"rendering {len(recipes)} recipe(s)...", file=sys.stderr)
    matte, find = _subject(args)
    if matte is not None:
        given = matte
        find = lambda photo: given  # noqa: E731  (render_all asks with the photo as given, which the matte fits)
    return recipes, render_all(imageio.load(args.photo), recipes, find_subject=find)


def _render(args: argparse.Namespace) -> int:
    recipes, results = _render_all(args)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for recipe, image in zip(recipes, results, strict=True):
        target = out / f"{Path(args.photo).stem}__{recipe.name}.{args.format}"
        imageio.save(image, target)
        print(target)
    return 0


def _sheet(args: argparse.Namespace) -> int:
    recipes, results = _render_all(args)
    target = Path(args.out)
    target.parent.mkdir(parents=True, exist_ok=True)
    imageio.save(contact_sheet([(r.name, img) for r, img in zip(recipes, results, strict=True)], args.columns), target)
    print(target)
    return 0


def _cutout(args: argparse.Namespace) -> int:
    photo = imageio.load(args.photo)
    matte, find = _subject(args)
    if matte is None:
        options = {k: v for k, v in (("shrink", args.shrink), ("feather", args.feather)) if v is not None}
        alpha = find(photo, **options)
    elif matte.shape != photo.shape[:2]:
        (mh, mw), (ph, pw) = matte.shape, photo.shape[:2]
        raise ValueError(f"the matte is {mw}x{mh} px but the photo is {pw}x{ph}")
    else:  # a matte given is used as it is, unless an edge clean-up is asked for
        alpha = refine_edge(matte, args.shrink or 0.0, args.feather or 0.0)
    target = Path(args.out)
    target.parent.mkdir(parents=True, exist_ok=True)
    imageio.save(photo, target, alpha=alpha)
    print(target)
    return 0


def _animate(args: argparse.Namespace) -> int:
    recipe = load_recipe(args.recipe)
    if recipe.animation is None:
        raise ValueError(f"{Path(args.recipe).name} has no [animation] table, so there is nothing to animate")
    target = Path(args.out) if args.out else Path("out") / f"{Path(args.photo).stem}__{recipe.name}.mp4"
    target.parent.mkdir(parents=True, exist_ok=True)
    total = len(recipe.animation.positions())

    def with_progress(frames: Iterable[np.ndarray]) -> Iterator[np.ndarray]:
        for i, frame in enumerate(frames, start=1):
            if i % 10 == 0 or i == total:
                print(f"frame {i}/{total}", file=sys.stderr)
            yield frame

    matte, find = _subject(args)
    save_video(
        with_progress(render_frames(imageio.load(args.photo), recipe, matte, find_subject=find)),
        target,
        fps=recipe.animation.fps,
        max_side=args.max_side,
    )
    print(target)
    return 0


def _run(args: argparse.Namespace) -> int:
    project = load_project(args.project)
    if args.steps:
        project = replace(project, steps=True)
    items = run(
        project,
        force=args.force,
        dry_run=args.dry_run,
        only=args.only,
        log=lambda message: print(message, file=sys.stderr),
        find_subject=partial(subject_matte, segmenter=args.segmenter),
    )
    failed = [i for i in items if i.status == "failed"]
    flagged = [i for i in items if i.warnings]
    print(f"{len(items)} results: {len(failed)} failed, {len(flagged)} with warnings", file=sys.stderr)
    if not args.dry_run:
        print(project.output / "index.html")
    return 1 if failed or (args.strict and flagged) else 0


def _steps(args: argparse.Namespace) -> int:
    recipe = load_recipe(args.recipe)
    folder = Path(args.out) if args.out else Path("out") / "steps" / f"{Path(args.photo).stem}__{recipe.name}"
    matte, find = _subject(args)
    stages = trace(imageio.load(args.photo), recipe, matte, at=args.at, find_subject=find)
    for stage in stages:
        print(f"  {stage.label}", file=sys.stderr)
    print(write_stages(stages, folder))
    return 0


def _init(args: argparse.Namespace) -> int:
    print(init_project(args.directory))
    return 0


def _new_effect(args: argparse.Namespace) -> int:
    _print_paths(gallery.new_effect(args.name, args.category))
    print(
        f"next: fill in the effect, its tests and its example recipe, then render its picture with "
        f"`photo-fx gallery --only {args.name}`",
        file=sys.stderr,
    )
    return 0


def _print_paths(paths: list[Path]) -> int:
    for path in paths:
        print(path)
    return 0


def _effects(args: argparse.Namespace) -> int:
    for name, doc in registry.describe().items():  # line by line, so the parameter list stays a list
        print(f"{name}\n{textwrap.indent(doc, '    ')}\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
