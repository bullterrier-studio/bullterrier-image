"""The photo pipeline: many photos x many recipes -> exported files, with caching, automatic checks and a report.

Stages, in order, for every photo:

    ingest    read the photo (orientation and colour profile applied, see `imageio`)
    analyse   find the subject and the depth once per photo, each if any recipe needs it; the results are cached on
              disk (so is what the finder finds in each piece, for a recipe that finds the subject piece by piece)
    style     run each recipe on the photo
    check     automatic quality checks (see `checks`); problems become warnings in the report
    export    write every format and size
    report    `manifest.json` (machine-readable) and `index.html` (a quick look) in the output folder

One photo or recipe failing never stops the others, and work that is already up to date is skipped on the next run,
so a folder of photos can be processed again and again as photos and recipes are added.

A pipeline is described by a small project file, `bt-image.toml` (`init_project` writes a starter):

    photos  = ["photos/*.jpg", "photos/*.png"]    # globs, relative to this file
    recipes = ["recipes"]                         # recipe files, or folders of them
    output  = "out"

    [export]
    formats = ["jpg"]       # jpg, png or webp
    sizes   = [2048, 1080]  # longest side in px; empty = the size the recipe renders at
    quality = 92            # jpg only: png and webp are lossless
    steps   = false         # true: also save the picture after every step, to see what each one did

    [checks]
    subject_share = [0.02, 0.97]   # a cut-out outside this share of the photo is flagged
"""

from collections.abc import Callable, Mapping
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Literal

from .core import native, registry
from .core.bridge import core_finder
from .core.finder import Finder
from .core.regions import FOUND_MAPS
from .core.steps import recipe_dict
from .recipe import Recipe, default_finder

_PROJECT_FILE = "bt-image.toml"
# The project file's tables and the `Project` fields each one holds (top-level keys map to fields of their own name).
_TABLES = {"export": ("formats", "sizes", "quality", "steps"), "checks": ("subject_share",)}
_DEFAULT_PHOTOS = ["photos/*.jpg", "photos/*.jpeg", "photos/*.png", "photos/*.webp"]


@dataclass(frozen=True)
class Project:
    """A pipeline as `bt-image.toml` describes it (see the module documentation; `load_project` reads one).

    Every value is checked when the project is made, however it is made (a file, code, `dataclasses.replace`), by the
    core; a bad one raises `ProjectError` naming the setting and the value.
    photos: the photo files, at least one, no two with the same file name without its extension (each gets the output
        folder of that name). recipes: recipe files, or `Recipe` objects built in code, at least one. output: the folder
        everything is written to.
    formats: how stills are saved, one or more of jpg, png, webp. sizes: longest sides in px (each >= 16) of smaller
        copies; empty = as rendered. quality: 1..100, for jpg only (png and webp are lossless).
    subject_share: (low, high), within 0..1: a cut-out covering less or more of the photo is flagged.
    steps: also save the picture after every step.
    """

    photos: tuple[Path, ...]
    recipes: tuple[Path | Recipe, ...]  # files, or Recipe objects built in code
    output: Path
    formats: tuple[Literal["jpg", "png", "webp"], ...] = ("jpg",)
    sizes: tuple[int, ...] = ()  # largest first, so equal settings written in another order are the same settings
    quality: int = 92
    subject_share: tuple[float, float] = (0.02, 0.97)
    steps: bool = False  # also save the picture after every step of every recipe

    def __post_init__(self) -> None:
        recipes = tuple(r if isinstance(r, Recipe) else Path(r) for r in self.recipes)
        given = {
            "photos": [str(p) for p in self.photos],
            # the core holds a recipe built in code to nothing here: a placeholder object stands for it
            "recipes": [{} if isinstance(r, Recipe) else str(r) for r in recipes],
            "output": str(self.output),
            "formats": list(self.formats),
            "sizes": list(self.sizes),
            "quality": self.quality,
            "subject_share": list(self.subject_share),
            "steps": self.steps,
        }
        checked = native.check_project(given)
        for name, value in {
            "photos": tuple(Path(p) for p in self.photos),
            "recipes": recipes,
            "output": Path(self.output),
            "formats": tuple(checked["formats"]),
            "sizes": tuple(checked["sizes"]),
            "quality": checked["quality"],
            "subject_share": tuple(checked["subject_share"]),
            "steps": checked["steps"],
        }.items():
            object.__setattr__(self, name, value)

    @property
    def cache(self) -> Path:
        """The folder of remembered maps (subject mattes, depth maps), inside `output`, so a photo is analysed once
        across runs."""
        return self.output / ".cache"


@dataclass(frozen=True)
class Item:
    """What happened to one photo x recipe pair.

    photo, recipe: their names. status: done, skipped (up to date), failed or would-run (a dry run). outputs: the files
    written, relative to the output folder. warnings: what the automatic checks flagged. error: why it failed. steps:
    the contact sheet of every step, relative to the output folder. seconds: how long it took. fingerprint: the hash of
    everything that shaped the result, compared on the next run.
    """

    photo: str
    recipe: str
    status: Literal["done", "skipped", "failed", "would-run"]
    outputs: tuple[str, ...] = ()
    warnings: tuple[str, ...] = ()
    error: str | None = None
    steps: str | None = None
    seconds: float = 0.0
    fingerprint: str = ""


def load_project(path: str | Path) -> Project:
    """Read a project file; globs and folders are resolved relative to the file.

    Raises ProjectError, starting with the file's name, for a file that is not TOML (with its line and column), an
    unknown key, globs that match no photo, no recipe file, or a value `Project` refuses; OSError if it cannot be read.
    """
    read = native.read_project(Path(path))
    return Project(
        photos=tuple(Path(p) for p in read["photos"]),
        recipes=tuple(Path(r) for r in read["recipes"]),
        output=Path(read["output"]),
        formats=tuple(read["formats"]),
        sizes=tuple(read["sizes"]),
        quality=read["quality"],
        subject_share=tuple(read["subject_share"]),
        steps=read["steps"],
    )


def init_project(directory: str | Path = ".") -> Path:
    """Write a starter `bt-image.toml`, a `photos/` folder and two small recipes into `directory`; returns the file.

    Raises ProjectError if the project file is there already."""
    return native.init_project(directory)


def run(
    project: Project,
    *,
    force: bool = False,
    dry_run: bool = False,
    only: str | None = None,
    log: Callable[[str], None] = lambda message: None,
    finders: Mapping[str, Finder] | None = None,
) -> list[Item]:
    """Process every photo with every recipe and write the report; returns what happened to each pair. The work is the
    core's.

    Pairs whose photo, recipe, export settings, finders and the core itself are unchanged since the last run, and whose
    files still exist, are skipped unless `force`. `only` is a file-name pattern selecting some of the photos. `dry_run`
    reports what would be done and writes nothing. Recipes are validated first, so a typo fails at once (RecipeError, as
    for a recipe without a name or whose name leaves no file name: its files are called by its name made safe); two
    recipes whose names give one file name, an `only` that matches no photo, or a recipe that cuts the picture out (a
    framing with `transparent`) exported to jpg, raise ProjectError.
    `finders` has a `Finder` by map name, finding that map of a photo for the recipes that need it. The `subject` one,
    when none is given, is `bullterrier_image.finding.subject.subject_finder()` and the `depth` one
    `bullterrier_image.finding.depth.depth_finder()`, each built only when a recipe needs its map
    (BackendUnavailableError at once if it cannot run). One finder serves every photo. A map is asked at most once per
    photo, and never again for a photo whose map is remembered in the project's cache from an earlier run by a finder
    of the same `identity`; if it raises, the recipes of that photo that need it fail, and the rest run. A finder is
    known by its `identity` alone: another identity finds the maps again and redoes the recipes that use them, the same
    identity reuses what is remembered, so give a finder whose answers could differ another identity (or delete
    `project.cache`). A damaged cache entry is found again, also with `force`. A recipe whose regions find the subject
    piece by piece (`Subject` with `per`) asks the `subject` finder about each crop of the recipe's working picture,
    once for each crop of pixels, and remembers the answer likewise. A project holds no maps of its own, so a recipe
    that uses a `Given` map needs a finder of that name (ProjectError otherwise, naming the recipe and the map), and a
    finder named for no map of the recipes is a ProjectError too. The record (`manifest.json`) is updated after every
    pair, so an interrupted run keeps what it finished.
    """
    with registry.holding() as hold:
        given = {
            "photos": [str(photo) for photo in project.photos],
            "recipes": [recipe_dict(r, hold) if isinstance(r, Recipe) else str(r) for r in project.recipes],
            "output": str(project.output),
            "formats": list(project.formats),
            "sizes": list(project.sizes),
            "quality": project.quality,
            "subject_share": list(project.subject_share),
            "steps": project.steps,
        }
        named = dict(finders or {})
        # each default is built only when a recipe needs its map, so a setting nobody uses cannot stop the project
        for name in native.project_maps(given):
            if name in FOUND_MAPS and name not in named:
                named[name] = default_finder(name)
        items = native.run_project(
            given,
            {name: (finder.identity, core_finder(finder, name)) for name, finder in named.items()},
            pictures=hold.pictures(),
            luts=hold.tables(),
            force=force,
            dry_run=dry_run,
            only=only,
            generated=datetime.now().isoformat(timespec="seconds"),  # noqa: DTZ005  (local time, for the person reading)
            log=log,
        )
    return [Item(**{**item, "outputs": tuple(item["outputs"]), "warnings": tuple(item["warnings"])}) for item in items]
