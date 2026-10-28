"""Command line: `bt-image render|sheet|sweep|cutout|run|steps|init|effects`.

A thin layer over the library; no logic of its own.

The parser is built from the standard library alone and each command imports the library parts it uses when it runs,
so `--help`, `--version` and a usage error answer without loading numpy, scipy, Pillow or pydantic.
"""

import argparse
import os
import re
import shutil
import sys
import textwrap
from collections.abc import Callable
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:  # annotations only; the commands import what they use when they run
    import numpy as np

    from .recipe import Finder, Recipe

_DEBUG_VARIABLE = "BT_IMAGE_DEBUG"

_EXIT_CODES = """\
exit codes:
  0    success
  1    the command failed (reason after "error:" on stderr); for `run`: a failed result, or with --strict a warning
  2    wrong usage: an unknown command or option (argparse explains)
  130  interrupted with Ctrl-C

`bt-image COMMAND --help` shows a command's options and an example. Errors go to stderr, results to stdout.
"""


def main(argv: list[str] | None = None) -> int:
    """Run the command line with `argv` (default: `sys.argv[1:]`) and return the exit code.

    Results go to stdout, progress and errors to stderr. A mistake of the user (a file that is not an image, a recipe
    that does not exist, an output folder that cannot be written) prints `error: <what is wrong>` and returns 1; Ctrl-C
    prints `interrupted` and returns 130. With `--debug` (or `BT_IMAGE_DEBUG=1`) the exception is raised instead, for
    developers. See the exit codes in `--help`.
    """
    args = _parser().parse_args(argv)
    from .errors import BullterrierImageError  # after parsing: `--help` and `--version` have exited by now

    debug = args.debug or os.environ.get(_DEBUG_VARIABLE) == "1"
    try:
        return args.handler(args)
    # Nothing wider: a plain ValueError or the like is a bug, and its traceback is what the developer needs.
    except (BullterrierImageError, OSError) as error:
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
        prog="bt-image",
        description="Style photos with composable effects, optionally on a cut-out subject.",
        epilog=_EXIT_CODES,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--version", action=_Version, nargs=0, help="show the version of bullterrier-image and exit")
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
            epilog=f"example: bt-image {name} {example}".rstrip(),
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
    _map_options(p)
    p.add_argument("-o", "--out", default="out", help="output folder (default: out)")
    p.add_argument("--format", default="png", choices=["png", "jpg", "webp"], help="image format (default: png)")

    p = command(
        "sheet",
        _sheet,
        summary="render recipes and put them side by side in one comparison image",
        example="examples/photos/bullterrier.jpeg examples/recipes -o out/sheet.png",
    )
    _photo_and_recipes(p)
    _map_options(p)
    p.add_argument("-o", "--out", default="out/sheet.png", help="output file (default: out/sheet.png)")
    p.add_argument("--columns", type=_positive_int, default=3, help="pictures per row (default: 3)")

    p = command(
        "sweep",
        _sweep,
        summary="render one parameter of a recipe across a range of values, side by side in one image",
        example="examples/photos/bullterrier.jpeg glitch rgb_split.amount 0..20",
    )
    p.add_argument("photo", help="the photo to style")
    p.add_argument("recipe", help="the recipe .toml, or the name of a built-in look, whose parameter is swept")
    p.add_argument(
        "target",
        help="the number's address, e.g. rgb_split.amount; rgb_split#2.amount for the second step of an effect used "
        "twice, pulse.gain for a step called pulse, pulse.gain.end for the end of a drive, region.near.low, "
        "frame.angle",
    )
    p.add_argument(
        "range",
        metavar="START..END",
        type=_span,
        help="the values to show, e.g. 0..20; a negative start goes after `--`: `... -o s.png -- -2..2`",
    )
    _map_options(p)
    p.add_argument("-o", "--out", default="out/sweep.png", help="output file (default: out/sweep.png)")
    p.add_argument("--count", type=int, default=5, help="how many values, 2 to 24, evenly spaced (default: 5)")
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
    _map_option(
        p,
        "an existing subject map, the photo's size, instead of finding it, to refine: subject=cutout.png (a cut-out "
        "PNG, its transparency, or a black-and-white mask, white = subject); cutout takes no other map",
    )
    _segmenter_option(p)

    p = command(
        "run",
        _run,
        summary="process every photo of a project with every recipe: cached, checked, with a report",
        example="examples/bt-image.toml --dry-run",
    )
    p.add_argument("project", nargs="?", default="bt-image.toml", help="project file (default: bt-image.toml)")
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
        example="examples/photos/bullterrier.jpeg examples/recipes/ember-glitch.toml",
    )
    p.add_argument("photo", help="the photo to run the recipe on")
    p.add_argument("recipe", help="the recipe .toml, or the name of a built-in look, to look into")
    p.add_argument("-o", "--out", help="output folder (default: out/steps/<photo>__<recipe>)")
    _map_options(p)

    p = command(
        "init",
        _init,
        summary="create a starter bt-image.toml, a photos/ folder and two recipes",
        example="my-project",
    )
    p.add_argument("directory", nargs="?", default=".", help="where to create the project (default: here)")

    p = command(
        "effects",
        _effects,
        summary="list the built-in looks and the effects, one line each",
        example="--category light",
    )
    which = p.add_mutually_exclusive_group()
    which.add_argument("effect", nargs="?", help="show only this effect, in full: every parameter and preset")
    which.add_argument(
        "--category", metavar="NAME", help="list only the effects of this category (see --list-categories)"
    )
    which.add_argument(
        "--list-categories",
        action="store_true",
        help="list the categories that have effects, with their number of effects and what each holds",
    )
    p.add_argument(
        "--all",
        action="store_true",
        help="show every effect in full (parameters and presets) instead of one line; with --category, those of it",
    )
    p.add_argument(
        "--advanced",
        action="store_true",
        help="like --all, and also list the advanced parameters (--category narrows it as well)",
    )
    return parser


class _Version(argparse.Action):
    """`--version`: looks the version up only when asked, since finding it loads the packaging machinery."""

    def __call__(self, parser: argparse.ArgumentParser, *_args: object, **_kwargs: object) -> None:
        from . import __version__

        print(f"bt-image {__version__}")
        parser.exit()


def _positive_int(text: str) -> int:
    """An argparse type: a whole number of 1 or more."""
    if not text.isdigit() or int(text) < 1:
        raise argparse.ArgumentTypeError(f"{text!r} is not a whole number of 1 or more")
    return int(text)


def _span(text: str) -> tuple[float, float]:
    """An argparse type: `START..END`, two numbers (`0..20`, `-2..2`, `0..0.5`)."""
    number = r"(-?\d*\.?\d+)"
    found = re.fullmatch(f"{number}\\.\\.{number}", text)
    if not found:
        raise argparse.ArgumentTypeError(f"{text!r} is not a range of two numbers like 0..20")
    return float(found[1]), float(found[2])


def _progress(args: argparse.Namespace, message: str) -> None:
    """Tell the person watching what is going on (stderr), unless they asked for quiet with -q."""
    if not args.quiet:
        print(message, file=sys.stderr)


def _photo_and_recipes(p: argparse.ArgumentParser) -> None:
    p.add_argument("photo", help="the photo to style")
    p.add_argument(
        "recipes",
        nargs="+",
        help="recipe .toml files, folders of them, or names of built-in looks (see `bt-image effects`)",
    )


def _map_options(p: argparse.ArgumentParser) -> None:
    """The maps of the photo a command that renders recipes can be given, and what finds the subject otherwise."""
    _map_option(p)
    _segmenter_option(p)


def _map_spec(text: str) -> tuple[str, str]:
    """An argparse type: `NAME=PICTURE`, a map of the photo by its name and the picture that holds it."""
    name, equals, path = text.partition("=")
    if not (equals and name and path):
        raise argparse.ArgumentTypeError(f"expected NAME=PICTURE, for example subject=cutout.png; got {text!r}")
    return name, path


_RENDER_MAP_HELP = (
    "a map of the photo, the photo's size, instead of finding it: subject=cutout.png (a cut-out PNG, its "
    "transparency, or a black-and-white mask, white = subject; no model is used, except by a region that finds the "
    "subject piece by piece), depth=depth.png (greyscale, 8 or 16 bits, white = nearest, instead of the depth "
    "model), or the name of a `given` region of the recipe (a picture with transparency gives its alpha, any other "
    "its brightness); repeat for each map"
)


def _map_option(p: argparse.ArgumentParser, help: str = _RENDER_MAP_HELP) -> None:  # noqa: A002  (argparse's own name)
    p.add_argument("--map", dest="maps", action="append", default=[], type=_map_spec, metavar="NAME=PICTURE", help=help)


def _segmenter_option(p: argparse.ArgumentParser) -> None:
    p.add_argument(
        "--segmenter",
        type=_segmenter,
        help="what finds the subject: vision (Apple Vision, macOS only) or onnx (a portable model, downloaded on "
        "first use); default: $BT_IMAGE_SEGMENTER, else vision on macOS and onnx elsewhere. A segmenter named here is "
        "checked at once, even for a look that needs no subject",
    )


def _segmenter(name: str) -> str:
    """An argparse type: a segmenter name `bullterrier_image.finding.subject` knows, checked only when the option is
    given (the list lives with the subject finder, which is slow to import)."""
    from .finding.subject import SEGMENTERS

    if name not in SEGMENTERS:
        raise argparse.ArgumentTypeError(f"invalid choice: {name!r} (choose from {', '.join(SEGMENTERS)})")
    return name


def _maps(args: argparse.Namespace) -> "tuple[dict[str, np.ndarray], dict[str, Finder]]":
    """The maps as the command line gives them: each `--map NAME=PICTURE` read, and the finder --segmenter names for
    the subject (none: the library's default, built only if a recipe asks for it)."""
    from . import imageio
    from .errors import RecipeError
    from .finding.subject import subject_finder

    maps: dict[str, np.ndarray] = {}
    for name, path in args.maps:
        if name in maps:
            raise RecipeError(f"--map {name} is given twice")
        maps[name] = imageio.load_map(path)
    return maps, {} if args.segmenter is None else {"subject": subject_finder(args.segmenter)}


def _load_recipes(paths: list[str]) -> "list[Recipe]":
    from .core import looks
    from .errors import RecipeError
    from .recipe import load_recipe

    shipped = looks.read()
    files: list[str | Path] = []
    for text in paths:
        path = Path(text)
        if text in shipped:
            files.append(text)
        else:
            files += sorted(path.glob("*.toml")) if path.is_dir() else [path]
    if not files:
        raise RecipeError(f"no recipe files found in {', '.join(paths)}")
    return [load_recipe(f) for f in files]


def _render_all(args: argparse.Namespace) -> "tuple[list[Recipe], list[np.ndarray]]":
    recipes = _load_recipes(args.recipes)  # validated before any slow work starts
    _progress(args, f"rendering {len(recipes)} recipe(s)...")
    return recipes, _render_recipes(args, recipes)


def _render_recipes(args: argparse.Namespace, recipes: "list[Recipe]") -> "list[np.ndarray]":
    """The photo of `args` through each recipe, each map (the --map file, else found once) shared by them all."""
    from . import imageio
    from .recipe import render_all

    maps, finders = _maps(args)
    return render_all(imageio.load(args.photo), recipes, maps=maps, finders=finders)


def _render(args: argparse.Namespace) -> int:
    from . import imageio
    from .core.steps import file_name

    recipes, results = _render_all(args)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for recipe, image in zip(recipes, results, strict=True):
        target = out / f"{Path(args.photo).stem}__{file_name(recipe)}.{args.format}"
        imageio.save(image, target)
        print(target)
    return 0


def _sheet(args: argparse.Namespace) -> int:
    from . import imageio
    from .core.steps import file_name
    from .sheet import contact_sheet

    recipes, results = _render_all(args)
    target = Path(args.out)
    target.parent.mkdir(parents=True, exist_ok=True)
    tiles = [(file_name(r), img) for r, img in zip(recipes, results, strict=True)]  # labelled as `render` names files
    imageio.save(contact_sheet(tiles, args.columns), target)
    print(target)
    return 0


def _sweep(args: argparse.Namespace) -> int:
    from . import imageio
    from .recipe import load_recipe
    from .sheet import contact_sheet, sweep

    start, end = args.range
    tiles = sweep(load_recipe(args.recipe), args.target, start, end, args.count)  # all validated before any work
    _progress(args, f"rendering {len(tiles)} value(s) of {args.target}...")
    results = _render_recipes(args, [recipe for _, recipe in tiles])
    target = Path(args.out)
    target.parent.mkdir(parents=True, exist_ok=True)
    imageio.save(
        contact_sheet([(label, image) for (label, _), image in zip(tiles, results, strict=True)], args.columns), target
    )
    print(target)
    return 0


def _cutout(args: argparse.Namespace) -> int:
    import numpy as np

    from . import imageio
    from .errors import RecipeError, SubjectNotFoundError
    from .finding.subject import refine_edge, subject_matte

    photo = imageio.load(args.photo)
    maps, _ = _maps(args)
    if other := sorted(set(maps) - {"subject"}):
        raise RecipeError(f"cutout takes only --map subject=PICTURE, not {other[0]!r}")
    matte = maps.get("subject")
    if matte is None:
        options = {k: v for k, v in (("shrink", args.shrink), ("feather", args.feather)) if v is not None}
        alpha = subject_matte(photo, segmenter=args.segmenter, **options)
    elif matte.shape != photo.shape[:2]:
        (mh, mw), (ph, pw) = matte.shape, photo.shape[:2]
        raise SubjectNotFoundError(f"the subject map is {mw}x{mh} px but the photo is {pw}x{ph}")
    else:  # a matte given is used as it is, unless an edge clean-up is asked for
        alpha = refine_edge(matte, args.shrink or 0.0, args.feather or 0.0)
    target = Path(args.out)
    target.parent.mkdir(parents=True, exist_ok=True)
    imageio.save(np.dstack([photo, alpha]), target)
    print(target)
    return 0


def _run(args: argparse.Namespace) -> int:
    from dataclasses import replace

    from .finding.subject import subject_finder
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
        finders={} if args.segmenter is None else {"subject": subject_finder(args.segmenter)},
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
    from .core.steps import file_name
    from .recipe import load_recipe, trace
    from .sheet import write_stages

    recipe = load_recipe(args.recipe)
    folder = Path(args.out) if args.out else Path("out") / "steps" / f"{Path(args.photo).stem}__{file_name(recipe)}"
    maps, finders = _maps(args)
    stages = trace(imageio.load(args.photo), recipe, maps=maps, finders=finders)
    for stage in stages:
        _progress(args, f"  {stage.label}")
    print(write_stages(stages, folder))
    return 0


def _init(args: argparse.Namespace) -> int:
    from .pipeline import init_project

    print(init_project(args.directory))
    return 0


_LISTING_WIDTH = (60, 120)  # the narrowest and widest a short listing is wrapped to; the width of a pipe is the widest


def _effects(args: argparse.Namespace) -> int:
    from .core import looks, registry
    from .errors import RecipeError

    used = {category: names for category, names in registry.categories().items() if names}
    if args.list_categories:
        width = max((len(category) for category in used), default=0)
        for category, names in used.items():
            print(f"{category:<{width}}  {len(names):>3}  {registry.CATEGORY_DESCRIPTIONS[category]}")
        return 0
    if args.effect is not None:
        registry.get(args.effect)  # a misspelt name raises UnknownEffectError saying where the name lives
        print(f"{args.effect}\n{textwrap.indent(registry.documentation(args.effect), '    ')}")
        return 0
    if args.category is None:
        names = sorted(registry.describe())
    elif args.category in used:
        names = used[args.category]
    else:
        raise RecipeError(f"no effects in category {args.category!r}; categories with effects: {', '.join(used)}")
    if args.category is None:  # the looks, which are no effects, come first: a person looks for a style before a tool
        print("Looks (recipes of several steps; a name works where a recipe file does: `bt-image render PHOTO crt`)")
        for name, look in looks.read().items():
            print(_entry(name, look.description))
        print()
    if args.all or args.advanced:
        for name in names:  # line by line, so the parameter list stays a list
            print(f"{name}\n{textwrap.indent(registry.documentation(name, advanced=args.advanced), '    ')}\n")
        return 0
    print("Effects" if args.category is None else f"Effects in {args.category}")
    for name in names:
        found = list(registry.presets(name))
        print(_entry(name, registry.summary(name) + (f" (presets: {', '.join(found)})" if found else "")))
    print("\ndetails: bt-image effects NAME, or --all")
    return 0


def _entry(name: str, text: str) -> str:
    """`  name: text`, wrapped to the terminal's width (see `_LISTING_WIDTH`), continuation lines indented further."""
    low, high = _LISTING_WIDTH
    width = min(max(shutil.get_terminal_size((high, 24)).columns, low), high)
    return textwrap.fill(f"{name}: {text}", width, initial_indent="  ", subsequent_indent="    ")
