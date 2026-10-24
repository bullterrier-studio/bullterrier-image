"""Developer commands that need a source checkout of bullterrier-image: `uv run poe gallery|regenerate|new-effect`.

They write into the checkout bullterrier-image runs from (the effect packages, tests, example recipes, gallery pictures,
`fx.pyi` and the catalogue in `docs/effects/`), so they are not part of the installed `bt-image` program. Each prints
the files it wrote, one per line, to stdout; an error is `error: <reason>` on stderr and exit code 1.
"""

import argparse
import sys

from bullterrier_image import BullterrierImageError, gallery
from bullterrier_image.core import registry


def main(argv: list[str] | None = None) -> int:
    """Run the developer command in `argv` (default: `sys.argv[1:]`); return the exit code (0 done, 1 failed)."""
    parser = argparse.ArgumentParser(prog="dev.py", description="Developer commands that need a source checkout.")
    commands = parser.add_subparsers(dest="command", required=True)

    p = commands.add_parser(
        "gallery",
        help="render the missing gallery pictures, or the ones --only names (without it, also write docs/effects/)",
        epilog="example: uv run poe gallery --only grain",
    )
    p.add_argument(
        "--only",
        nargs="+",
        default=[],
        metavar="NAME",
        help="render these effects' pictures, existing or not; no pages are written",
    )
    p.add_argument(
        "--force", action="store_true", help="without --only: render every picture again, not only the missing"
    )

    commands.add_parser(
        "regenerate",
        help="after adding or changing effects: write fx.pyi and docs/effects/, render missing pictures",
        epilog="example: uv run poe regenerate",
    )

    p = commands.add_parser(
        "new-effect",
        help="create a new effect's files from the template: its code, its tests and its example recipe",
        epilog="example: uv run poe new-effect my_effect --category light",
    )
    p.add_argument("name", help="the effect's name: lowercase letters, digits and _, starting with a letter")
    p.add_argument("--category", required=True, choices=registry.CATEGORIES, help="its section of the catalogue")

    args = parser.parse_args(argv)
    try:
        if args.command == "gallery":
            written = gallery.write_docs(args.only, args.force)
        elif args.command == "regenerate":
            written = gallery.regenerate()
        else:
            written = gallery.new_effect(args.name, args.category)
    except (
        BullterrierImageError,
        ValueError,
        OSError,
    ) as error:  # ValueError: gallery's own refusals (unknown or taken name)
        print(f"error: {error}", file=sys.stderr)
        return 1
    for path in written:
        print(path)
    if args.command == "new-effect":
        print(
            f"next: fill in the effect, its tests and its example recipe, then render its picture with "
            f"`uv run poe gallery --only {args.name}`",
            file=sys.stderr,
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
