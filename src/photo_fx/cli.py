"""Command line: `photo-fx render|sheet|cutout|animate|run|steps|init|effects`.

A thin layer over the library; no logic of its own.

The parser is built from the standard library alone and each command imports the library parts it uses when it runs,
so `--help`, `--version` and a usage error answer without loading numpy, scipy, Pillow or pydantic.
"""

import argparse
import os
import sys
import textwrap
from collections.abc import Callable, Iterable, Iterator
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:  # annotations only; the commands import what they use when they run
    import numpy as np

    from .recipe import Recipe, SubjectFinder

_DEBUG_VARIABLE = "PHOTO_FX_DEBUG"

_EXIT_CODES = """\
exit codes:
  0    success
  1    the command failed (reason after "error:" on stderr); for `run`: a failed result, or with --strict a warning
  2    wrong usage: an unknown command or option (argparse explains)
  130  interrupted with Ctrl-C

`photo-fx COMMAND --help` shows a command's options and an example. Errors go to stderr, results to stdout.
"""


def main(argv: list[str] | None = None) -> int:
    """Run the command line with `argv` (default: `sys.argv[1:]`) and return the exit code.

    Results go to stdout, progress and errors to stderr. A mistake of the user (a file that is not an image, a recipe
    that does not exist, an output folder that cannot be written) prints `error: <what is wrong>` and returns 1; Ctrl-C
    prints `interrupted` and returns 130. With `--debug` (or `PHOTO_FX_DEBUG=1`) the exception is raised instead, for
    developers. See the exit codes in `--help`.
    """
    args = _parser().parse_args(argv)
    from .errors import PhotoFxError  # after parsing: `--help` and `--version` have exited by now

    debug = args.debug or os.environ.get(_DEBUG_VARIABLE) == "1"
    try:
        return args.handler(args)
    # Nothing wider: a plain ValueError or the like is a bug, and its traceback is what the developer needs.
    except (PhotoFxError, OSError) as error:
        if debug:
            raise
        print(f"error: {error}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        if debug:
            raise
        print("interrupted", file=sys.stderr)
        return 130


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="photo-fx",
        description="Style photos with composable effects, optionally on a cut-out subject, and animate them.",
        epilog=_EXIT_CODES,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--version", action=_Version, nargs=0, help="show the version of photo-fx and exit")
    commands = parser.add_subparsers(dest="command", required=True)

    # Every subcommand takes --debug and -q, so they are spelt once, as a parent of each.
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument(
        "--debug", action="store_true", help=f"show the full traceback of an error (also: {_DEBUG_VARIABLE}=1)"
    )
    common.add_argument("-q", "--quiet", action="store_true", help="hide the progress lines (errors still show)")

    def command(
        name: str, handler: Callable[[argparse.Namespace], int], *, summary: str, example: str
    ) -> argparse.ArgumentParser:
        p = commands.add_parser(
            name,
            help=summary,
            description=summary,
            epilog=f"example: photo-fx {name} {example}".rstrip(),
            parents=[common],
            formatter_class=argparse.RawDescriptionHelpFormatter,
        )
        p.set_defaults(handler=handler)
        return p

    p = command(
        "render",
        _render,
        summary="render a photo through one or more recipes, one file each",
        example="examples/photos/bullterrier.jpeg examples/recipes -o out",
    )
    _photo_and_recipes(p)
    _subject_options(p)
    p.add_argument("-o", "--out", default="out", help="output folder (default: out)")
    p.add_argument("--format", default="png", choices=["png", "jpg", "webp"], help="image format (default: png)")

    p = command(
        "sheet",
        _sheet,
        summary="render recipes and put them side by side in one comparison image",
        example="examples/photos/bullterrier.jpeg examples/recipes -o out/sheet.png",
    )
    _photo_and_recipes(p)
    _subject_options(p)
    p.add_argument("-o", "--out", default="out/sheet.png", help="output file (default: out/sheet.png)")
    p.add_argument("--columns", type=_positive_int, default=3, help="pictures per row (default: 3)")

    p = command(
        "cutout",
        _cutout,
        summary="save just the subject on a transparent background",
        example="examples/photos/bullterrier.jpeg -o out/cutout.png --feather 2",
    )
    p.add_argument("photo", help="the photo to cut the subject out of")
    p.add_argument("-o", "--out", default="out/cutout.png", help="output file, a PNG (default: out/cutout.png)")
    p.add_argument("--shrink", type=float, default=None, help="pull the cut edge in by this many px")
    p.add_argument("--feather", type=float, default=None, help="soften the cut edge by this many px")
    _subject_options(p)

    p = command(
        "animate",
        _animate,
        summary="render an animated recipe to a video (.mp4, .mov) or a .gif",
        example="examples/photos/bullterrier.jpeg examples/recipes/06-evaporate.toml -o out/evaporate.mp4",
    )
    p.add_argument("photo", help="the photo to animate")
    p.add_argument("recipe", help="a recipe .toml with an [animation] table")
    p.add_argument("-o", "--out", help="output file (default: out/<photo>__<recipe>.mp4)")
    p.add_argument(
        "--max-side", type=int, help="shrink the video so its longest side is at most this many px (e.g. 1080)"
    )
    _subject_options(p)

    p = command(
        "run",
        _run,
        summary="process every photo of a project with every recipe: cached, checked, with a report",
        example="examples/photo-fx.toml --dry-run",
    )
    p.add_argument("project", nargs="?", default="photo-fx.toml", help="project file (default: photo-fx.toml)")
    p.add_argument("--force", action="store_true", help="redo everything, even what is up to date")
    p.add_argument("--dry-run", action="store_true", help="show what would be done and write nothing")
    p.add_argument("--only", metavar="PATTERN", help="only the photos whose file name matches, e.g. 'dog*.jpg'")
    p.add_argument("--strict", action="store_true", help="exit with an error if any result has a warning")

    p.add_argument("--steps", action="store_true", help="also save the picture after every step of every recipe")
    _segmenter_option(p)

    p = command(
        "steps",
        _steps,
        summary="show what every step of a recipe does to a photo: one picture per step",
        example="examples/photos/bullterrier.jpeg examples/recipes/05-ember-glitch.toml",
    )
    p.add_argument("photo", help="the photo to run the recipe on")
    p.add_argument("recipe", help="the recipe .toml to look into")
    p.add_argument("-o", "--out", help="output folder (default: out/steps/<photo>__<recipe>)")
    p.add_argument(
        "--at", type=float, default=0.5, help="for an animated recipe: the moment of the clip, 0..1 (default 0.5)"
    )
    _subject_options(p)

    p = command(
        "init",
        _init,
        summary="create a starter photo-fx.toml, a photos/ folder and two recipes",
        example="my-project",
    )
    p.add_argument("directory", nargs="?", default=".", help="where to create the project (default: here)")

    p = command(
        "effects",
        _effects,
        summary="list the available effects and their parameters",
        example="--category light",
    )
    which = p.add_mutually_exclusive_group()
    which.add_argument(
        "--category", metavar="NAME", help="list only the effects of this category (see --list-categories)"
    )
    which.add_argument(
        "--list-categories",
        action="store_true",
        help="list the categories that have effects, with their number of effects and what each holds",
    )
    return parser


class _Version(argparse.Action):
    """`--version`: looks the version up only when asked, since finding it loads the packaging machinery."""

    def __call__(self, parser: argparse.ArgumentParser, *_args: object, **_kwargs: object) -> None:
        from . import __version__

        print(f"photo-fx {__version__}")
        parser.exit()


def _positive_int(text: str) -> int:
    """An argparse type: a whole number of 1 or more."""
    if not text.isdigit() or int(text) < 1:
        raise argparse.ArgumentTypeError(f"{text!r} is not a whole number of 1 or more")
    return int(text)


def _progress(args: argparse.Namespace, message: str) -> None:
    """Tell the person watching what is going on (stderr), unless they asked for quiet with -q."""
    if not args.quiet:
        print(message, file=sys.stderr)


def _photo_and_recipes(p: argparse.ArgumentParser) -> None:
    p.add_argument("photo", help="the photo to style")
    p.add_argument("recipes", nargs="+", help="recipe .toml files, or folders of them")


def _subject_options(p: argparse.ArgumentParser) -> None:
    p.add_argument(
        "--matte",
        metavar="PICTURE",
        help="the subject's matte, the photo's size: a cut-out PNG (its transparency) or a black-and-white mask "
        "(white = subject); no model is used",
    )
    _segmenter_option(p)


def _segmenter_option(p: argparse.ArgumentParser) -> None:
    p.add_argument(
        "--segmenter",
        type=_segmenter,
        help="what finds the subject: vision (Apple Vision, macOS) or onnx (a portable model, downloaded on first "
        "use); default: $PHOTO_FX_SEGMENTER, else vision where available",
    )


def _segmenter(name: str) -> str:
    """An argparse type: a segmenter name `photo_fx.matte` knows, checked only when the option is given (the list
    lives with the subject finder, which is slow to import)."""
    from .matte import SEGMENTERS

    if name not in SEGMENTERS:
        raise argparse.ArgumentTypeError(f"invalid choice: {name!r} (choose from {', '.join(SEGMENTERS)})")
    return name


def _subject(args: argparse.Namespace) -> "tuple[np.ndarray | None, SubjectFinder]":
    """The subject as the command line gives it: a matte file (--matte), and how to find it otherwise (--segmenter)."""
    from functools import partial

    from . import imageio
    from .matte import subject_matte

    matte = imageio.load_matte(args.matte) if args.matte else None
    return matte, partial(subject_matte, segmenter=args.segmenter)


def _load_recipes(paths: list[str]) -> "list[Recipe]":
    from .errors import RecipeError
    from .recipe import load_recipe

    files: list[Path] = []
    for path in map(Path, paths):
        files += sorted(path.glob("*.toml")) if path.is_dir() else [path]
    if not files:
        raise RecipeError(f"no recipe files found in {', '.join(paths)}")
    return [load_recipe(f) for f in files]


def _render_all(args: argparse.Namespace) -> "tuple[list[Recipe], list[np.ndarray]]":
    from . import imageio
    from .recipe import render_all

    recipes = _load_recipes(args.recipes)  # validated before any slow work starts
    _progress(args, f"rendering {len(recipes)} recipe(s)...")
    matte, find = _subject(args)
    if matte is not None:
        given = matte
        find = lambda photo: given  # noqa: E731  (render_all asks with the photo as given, which the matte fits)
    return recipes, render_all(imageio.load(args.photo), recipes, find_subject=find)


def _render(args: argparse.Namespace) -> int:
    from . import imageio

    recipes, results = _render_all(args)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for recipe, image in zip(recipes, results, strict=True):
        target = out / f"{Path(args.photo).stem}__{recipe.name}.{args.format}"
        imageio.save(image, target)
        print(target)
    return 0


def _sheet(args: argparse.Namespace) -> int:
    from . import imageio
    from .sheet import contact_sheet

    recipes, results = _render_all(args)
    target = Path(args.out)
    target.parent.mkdir(parents=True, exist_ok=True)
    imageio.save(contact_sheet([(r.name, img) for r, img in zip(recipes, results, strict=True)], args.columns), target)
    print(target)
    return 0


def _cutout(args: argparse.Namespace) -> int:
    from . import imageio
    from .errors import SubjectNotFoundError
    from .matte import refine_edge

    photo = imageio.load(args.photo)
    matte, find = _subject(args)
    if matte is None:
        options = {k: v for k, v in (("shrink", args.shrink), ("feather", args.feather)) if v is not None}
        alpha = find(photo, **options)
    elif matte.shape != photo.shape[:2]:
        (mh, mw), (ph, pw) = matte.shape, photo.shape[:2]
        raise SubjectNotFoundError(f"the matte is {mw}x{mh} px but the photo is {pw}x{ph}")
    else:  # a matte given is used as it is, unless an edge clean-up is asked for
        alpha = refine_edge(matte, args.shrink or 0.0, args.feather or 0.0)
    target = Path(args.out)
    target.parent.mkdir(parents=True, exist_ok=True)
    imageio.save(photo, target, alpha=alpha)
    print(target)
    return 0


def _animate(args: argparse.Namespace) -> int:
    from . import imageio
    from .errors import RecipeError
    from .recipe import load_recipe, render_frames

    recipe = load_recipe(args.recipe)
    if recipe.animation is None:
        raise RecipeError(f"{Path(args.recipe).name} has no [animation] table, so there is nothing to animate")
    target = Path(args.out) if args.out else Path("out") / f"{Path(args.photo).stem}__{recipe.name}.mp4"
    target.parent.mkdir(parents=True, exist_ok=True)
    total = len(recipe.animation.positions())

    def with_progress(frames: "Iterable[np.ndarray]") -> "Iterator[np.ndarray]":
        for i, frame in enumerate(frames, start=1):
            if i % 10 == 0 or i == total:
                _progress(args, f"frame {i}/{total}")
            yield frame

    matte, find = _subject(args)
    imageio.save_video(
        with_progress(render_frames(imageio.load(args.photo), recipe, matte, find_subject=find)),
        target,
        fps=recipe.animation.fps,
        max_side=args.max_side,
    )
    print(target)
    return 0


def _run(args: argparse.Namespace) -> int:
    from dataclasses import replace

    from .pipeline import load_project, run

    project = load_project(args.project)
    if args.steps:
        project = replace(project, steps=True)
    items = run(
        project,
        force=args.force,
        dry_run=args.dry_run,
        only=args.only,
        log=lambda message: _progress(args, message),
        subject=args.segmenter,
    )
    failed = [i for i in items if i.status == "failed"]
    flagged = [i for i in items if i.warnings]
    if failed or flagged:  # -q hides a clean summary, never a problem
        print(f"{len(items)} results: {len(failed)} failed, {len(flagged)} with warnings", file=sys.stderr)
    else:
        _progress(args, f"{len(items)} results: 0 failed, 0 with warnings")
    if not args.dry_run:
        print(project.output / "index.html")
    return 1 if failed or (args.strict and flagged) else 0


def _steps(args: argparse.Namespace) -> int:
    from . import imageio
    from .recipe import load_recipe, trace
    from .sheet import write_stages

    recipe = load_recipe(args.recipe)
    folder = Path(args.out) if args.out else Path("out") / "steps" / f"{Path(args.photo).stem}__{recipe.name}"
    matte, find = _subject(args)
    stages = trace(imageio.load(args.photo), recipe, matte, at=args.at, find_subject=find)
    for stage in stages:
        _progress(args, f"  {stage.label}")
    print(write_stages(stages, folder))
    return 0


def _init(args: argparse.Namespace) -> int:
    from .pipeline import init_project

    print(init_project(args.directory))
    return 0


def _effects(args: argparse.Namespace) -> int:
    from .core import registry
    from .errors import RecipeError

    used = {category: names for category, names in registry.categories().items() if names}
    if args.list_categories:
        width = max((len(category) for category in used), default=0)
        for category, names in used.items():
            print(f"{category:<{width}}  {len(names):>3}  {registry.CATEGORY_DESCRIPTIONS[category]}")
        return 0
    if args.category is None:
        names = sorted(registry.describe())
    elif args.category in used:
        names = used[args.category]
    else:
        raise RecipeError(f"no effects in category {args.category!r}; categories with effects: {', '.join(used)}")
    for name in names:  # line by line, so the parameter list stays a list
        print(f"{name}\n{textwrap.indent(registry.documentation(name), '    ')}\n")
    return 0
