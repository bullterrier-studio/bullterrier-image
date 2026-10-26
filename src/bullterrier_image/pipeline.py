"""The photo pipeline: many photos x many recipes -> exported files, with caching, automatic checks and a report.

Stages, in order, for every photo:

    ingest    read the photo (orientation and colour profile applied, see `imageio`)
    analyse   find the subject once per photo, if any recipe needs it; the result is cached on disk (so is what the
              finder finds in each piece, for a recipe that finds the subject piece by piece)
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
    quality = 92
    steps   = false         # true: also save the picture after every step, to see what each one did

    [checks]
    subject_share = [0.02, 0.97]   # a cut-out outside this share of the photo is flagged
"""

import fnmatch
import functools
import glob
import hashlib
import html
import io
import json
import tempfile
import time
import tomllib
import zipfile
from collections.abc import Callable
from dataclasses import asdict, dataclass, field, fields, replace
from datetime import datetime
from importlib import metadata
from pathlib import Path
from typing import Annotated, Any, Literal

import numpy as np
from pydantic import AfterValidator, ConfigDict, Field, ValidationError, model_validator
from pydantic.dataclasses import dataclass as validated_dataclass

from . import finding, imageio
from .checks import matte_warnings, still_warnings
from .core.outlines import svg_bytes
from .core.params import Picture, describe_error
from .core.regions import Svg
from .core.runner import Finder, subject_from
from .errors import BullterrierImageError, ProjectError, SubjectNotFoundError
from .finding.subject import subject_finder
from .ops.resample import resize
from .recipe import Recipe, load_recipe, render, trace
from .sheet import write_stages

_PROJECT_FILE = "bt-image.toml"
# The project file's tables and the `Project` fields each one holds (top-level keys map to fields of their own name).
_TABLES = {"export": ("formats", "sizes", "quality", "steps"), "checks": ("subject_share",)}
_DEFAULT_PHOTOS = ["photos/*.jpg", "photos/*.jpeg", "photos/*.png", "photos/*.webp"]

_Share = Annotated[float, Field(strict=True, ge=0, le=1)]


def _checked_as_project[T](cls: type[T]) -> type[T]:
    """Make `cls` (a pydantic dataclass) report a failed validation as a `ProjectError` in the project's wording.

    Pydantic writes the dataclass's `__init__` itself (one written in the class body is ignored), so it is wrapped
    here; `dataclasses.replace` goes through it too, so a changed copy is checked like a new one."""
    generated = cls.__init__
    names = [f.name for f in fields(cls)]  # type: ignore[arg-type]  # a pydantic dataclass is a dataclass

    @functools.wraps(generated)
    def checked_init(self: object, *args: Any, **kwargs: Any) -> None:
        try:
            generated(self, *args, **kwargs)
        except ValidationError as error:
            raise ProjectError(describe_error(error, "", names)) from None

    cls.__init__ = checked_init  # type: ignore[method-assign]  # replacing pydantic's generated constructor on purpose
    return cls


@_checked_as_project
@validated_dataclass(frozen=True, config=ConfigDict(extra="forbid"))
class Project:
    """A pipeline as `bt-image.toml` describes it (see the module documentation; `load_project` reads one).

    Every value is checked when the project is made, however it is made (a file, code, `dataclasses.replace`); a bad
    one raises `ProjectError` naming the setting and the value.
    photos: the photo files, at least one, no two with the same file name without its extension (each gets the output
        folder of that name). recipes: recipe files, or `Recipe` objects built in code, at least one. output: the folder
        everything is written to.
    formats: how stills are saved, one or more of jpg, png, webp. sizes: longest sides in px (each >= 16) of smaller
        copies; empty = as rendered. quality: 1..100, for jpg and webp.
    subject_share: (low, high), within 0..1: a cut-out covering less or more of the photo is flagged.
    steps: also save the picture after every step.
    """

    photos: Annotated[tuple[Path, ...], Field(min_length=1)]
    recipes: Annotated[tuple[Path | Recipe, ...], Field(min_length=1)]  # files, or Recipe objects built in code
    output: Path
    formats: tuple[Literal["jpg", "png", "webp"], ...] = ("jpg",)
    # largest first, so equal settings written in another order are the same settings
    sizes: Annotated[
        tuple[Annotated[int, Field(strict=True, ge=16)], ...], AfterValidator(lambda s: tuple(sorted(s, reverse=True)))
    ] = ()
    quality: int = Field(default=92, strict=True, ge=1, le=100)
    subject_share: tuple[_Share, _Share] = (0.02, 0.97)
    steps: bool = Field(default=False, strict=True)  # also save the picture after every step of every recipe

    @model_validator(mode="after")
    def _consistent(self) -> "Project":
        """The rules across values: at least one format (checked here: a bad item would also report the length),
        a share range that is a range, and one output folder per photo."""
        if not self.formats:
            raise ValueError("formats must name at least one of jpg, png, webp")
        low, high = self.subject_share
        if not low < high:
            raise ValueError(f"subject_share must be (low, high) with low < high, got {self.subject_share}")
        stems = [p.stem for p in self.photos]
        if len(set(stems)) != len(stems):
            raise ValueError(f"two photos share a file name ({sorted({s for s in stems if stems.count(s) > 1})})")
        return self

    @property
    def cache(self) -> Path:
        """The folder of remembered subject mattes, inside `output`, so a photo is analysed once across runs."""
        return self.output / ".cache"


@validated_dataclass(frozen=True, config=ConfigDict(extra="forbid"))
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
    path = Path(path)
    root = path.parent
    try:
        with path.open("rb") as f:
            raw = tomllib.load(f)
    except tomllib.TOMLDecodeError as error:  # its message keeps the line and column
        raise ProjectError(f"{path.name}: {error}") from None
    except UnicodeDecodeError as error:
        raise ProjectError(f"{path.name}: not UTF-8 text ({error.reason} at byte {error.start})") from None
    unknown = set(raw) - {"photos", "recipes", "output", *_TABLES}
    if unknown:
        raise ProjectError(f"{path.name}: unknown keys {sorted(unknown)}")
    settings: dict[str, Any] = {}
    for name, keys in _TABLES.items():
        table = raw.get(name, {})
        if not isinstance(table, dict):
            raise ProjectError(f"{path.name}: write `{name}` as a [{name}] table")
        if set(table) - set(keys):
            raise ProjectError(f"{path.name}: unknown keys {sorted(set(table) - set(keys))}; allowed: {sorted(keys)}")
        settings |= table

    photo_globs = raw.get("photos", _DEFAULT_PHOTOS)
    found = {
        Path(p)
        for pattern in photo_globs
        for p in glob.glob(str(root / pattern), recursive=True)  # noqa: PTH207  (skips hidden ._x.jpg copies; Path.glob does not)
    }
    photos = tuple(sorted(p for p in found if p.is_file()))
    if not photos:
        raise ProjectError(f"{path.name}: no photos match {photo_globs} (looked relative to {root.resolve()})")
    recipes: list[Path] = []
    for entry in raw.get("recipes", ["recipes"]):
        target = root / entry
        recipes += sorted(target.glob("*.toml")) if target.is_dir() else [target]
    if not recipes or not all(r.is_file() for r in recipes):
        raise ProjectError(f"{path.name}: no recipe files found in {raw.get('recipes', ['recipes'])}")
    try:
        return Project(photos=photos, recipes=tuple(recipes), output=root / raw.get("output", "out"), **settings)
    except ProjectError as error:
        raise ProjectError(f"{path.name}: {error}") from None


def init_project(directory: str | Path = ".") -> Path:
    """Write a starter `bt-image.toml`, a `photos/` folder and two small recipes into `directory`; returns the file."""
    directory = Path(directory)
    target = directory / _PROJECT_FILE
    if target.exists():
        raise FileExistsError(f"{target} already exists")
    (directory / "photos").mkdir(parents=True, exist_ok=True)
    recipes = directory / "recipes"
    if not recipes.exists():
        recipes.mkdir()
        for name, text in _STARTER_RECIPES.items():
            (recipes / name).write_text(text, encoding="utf-8")
    target.write_text(_STARTER_PROJECT, encoding="utf-8")
    return target


def run(
    project: Project,
    *,
    force: bool = False,
    dry_run: bool = False,
    only: str | None = None,
    log: Callable[[str], None] = lambda message: None,
    find_subject: Finder | None = None,
) -> list[Item]:
    """Process every photo with every recipe and write the report; returns what happened to each pair.

    Pairs whose photo, recipe, export settings and the tool's own code are unchanged since the last run, and whose
    files still exist, are skipped unless `force`. `only` is a file-name pattern selecting some of the photos.
    `dry_run` reports what would be done and writes nothing. Recipes are validated first, so a typo fails at once
    (RecipeError); two recipes with one name, an `only` that matches no photo, or a recipe that cuts the picture out
    (a framing with `transparent`) exported to jpg, raise ProjectError.
    `find_subject` finds the subject of a photo for the recipes that need it; None means
    `bullterrier_image.finding.subject.subject_finder()` (the segmenter the environment chooses, else the portable
    model), built only when a recipe needs the subject (BackendUnavailableError at once if it cannot run). It is asked
    at most once per photo, and never again for a photo whose subject is remembered in the project's cache from an
    earlier run by a finder of the same `identity`; if it raises, the recipes of that photo that need the subject fail,
    and the rest run. A finder is known by its `identity` alone: another identity finds the subjects again and redoes
    the recipes that use them, the same identity reuses what is remembered, so give a finder whose answers could differ
    another identity (or delete `project.cache`). A damaged cache entry is found again, also with `force`. A recipe
    whose regions find the subject piece by piece (`Subject` with `per`) asks it about each crop of the recipe's working
    picture, once for each crop of pixels, and remembers the answer likewise, whatever recipe cut the crop.
    The record (`manifest.json`) is updated after every pair, so an interrupted run keeps what it finished.
    """
    recipes = [(r, r if isinstance(r, Recipe) else load_recipe(r)) for r in project.recipes]
    names = [recipe.name for _, recipe in recipes]
    if len(set(names)) != len(names):
        raise ProjectError(f"two recipes are called the same: {sorted({n for n in names if names.count(n) > 1})}")
    for _, recipe in recipes:
        if recipe.frame.has_alpha and "jpg" in project.formats:
            raise ProjectError(
                f"recipe {recipe.name!r} cuts the picture out (transparent = {recipe.frame.transparent!r}), which a "
                "JPEG cannot hold, but the project exports 'jpg': use png or webp in `formats`"
            )
    photos = [p for p in project.photos if only is None or fnmatch.fnmatch(p.name, only)]
    if not photos:
        raise ProjectError(f"no photos match --only {only!r}")

    previous = {(item.photo, item.recipe): item for item in _read_manifest(project.output)}
    settings = repr(
        (
            project.formats,
            project.sizes,
            project.quality,
            project.subject_share,
            project.steps,
        )
    )
    code = _fingerprint(_source_digest(Path(__file__).parent), _library_versions())
    # the default finder is built only when a recipe needs the subject: a setting nobody uses cannot stop the project.
    # What a remembered subject depends on besides the photo: the finder's identity, the subject-finding code (the edge
    # clean-up is part of what is remembered) and the libraries.
    finder, subject_key = None, ""
    if any(r.needs_matte or r.needs_finder for _, r in recipes):
        finder = subject_finder() if find_subject is None else find_subject
        subject_key = _fingerprint(finder.identity, _source_digest(Path(finding.__file__).parent), _library_versions())
    items: list[Item] = []
    total = len(photos) * len(recipes)
    for photo in photos:
        photo_hash = hashlib.sha256(photo.read_bytes()).hexdigest()
        state = _Photo(photo, _fingerprint(photo_hash, subject_key), project, finder)
        for path, recipe in recipes:
            label = f"[{len(items) + 1}/{total}] {photo.name} x {recipe.name}"
            recipe_text = repr(path) if isinstance(path, Recipe) else path.read_text(encoding="utf-8")
            # a recipe that uses the subject is redone when another finder would find it; one that uses a picture is
            # redone when that picture's pixels (or an svg file's bytes) change, whether the recipe is a file or an
            # object (repr: no pixels)
            parts = (
                photo_hash,
                recipe_text,
                *_picture_digests(recipe),
                *_svg_digests(recipe),
                settings,
                code,
                *((subject_key,) if recipe.needs_matte or recipe.needs_finder else ()),
            )
            fingerprint = _fingerprint(*parts)
            old = previous.get((photo.name, recipe.name))
            if (
                not force
                and old
                and old.status in ("done", "skipped")
                and old.fingerprint == fingerprint
                and all((project.output / o).exists() for o in old.outputs)
            ):
                items.append(replace(old, status="skipped", seconds=0.0))
                log(f"{label}: up to date")
            elif dry_run:
                items.append(Item(photo.name, recipe.name, "would-run", fingerprint=fingerprint))
                log(f"{label}: would run")
            else:
                started = time.perf_counter()
                try:
                    item = _process(project, state, recipe, fingerprint)
                # one bad photo or recipe must not stop the batch: whatever a photo, a recipe, the finder or
                # the disk can raise fails this pair (a photo too large for memory included); a bug still stops it
                except (BullterrierImageError, OSError, ValueError, RuntimeError, MemoryError) as error:
                    item = Item(
                        photo.name,
                        recipe.name,
                        "failed",
                        error=f"{type(error).__name__}: {error}",
                        fingerprint=fingerprint,
                    )
                item = replace(item, seconds=round(time.perf_counter() - started, 2))
                items.append(item)
                _write_record(project, items, previous, names, code)
                log(
                    f"{label}: {item.status}"
                    + (f" ({'; '.join(item.warnings)})" if item.warnings else "")
                    + (f" {item.error}" if item.error else "")
                )
    if not dry_run:
        record = _write_record(project, items, previous, names, code)
        _write_text(project.output / "index.html", _report_html(project, record))
    return items


def _write_record(
    project: Project, items: list[Item], previous: dict[tuple[str, str], Item], names: list[str], code: str
) -> list[Item]:
    """Write `manifest.json`: this run's `items`, and the `previous` results of the pairs it has not reached or left
    alone (--only), for the project's photos and its recipes (`names`, in their order). Returns the record."""
    seen = {(i.photo, i.recipe) for i in items}
    photo_names = {p.name for p in project.photos}
    record = items + [
        i for key, i in previous.items() if key not in seen and i.photo in photo_names and i.recipe in names
    ]
    record.sort(key=lambda i: (i.photo, names.index(i.recipe)))
    manifest = {
        # local time, for the person reading the report; zone-aware would change the manifest's format
        "generated": datetime.now().isoformat(timespec="seconds"),  # noqa: DTZ005
        "code": code,
        "items": [asdict(i) for i in record],
    }
    _write_text(project.output / "manifest.json", json.dumps(manifest, indent=2, ensure_ascii=False))
    return record


@dataclass
class _Photo:
    """One photo while its recipes run: read and analysed once, on first use. `subject_key` names its remembered
    subject: the photo's content and what finds the subject. `finder` is None when no recipe needs the subject."""

    path: Path
    subject_key: str
    project: Project
    finder: Finder | None
    _rgb: np.ndarray | None = field(default=None, repr=False)
    _matte: np.ndarray | None = field(default=None, repr=False)
    _matte_error: Exception | None = field(default=None, repr=False)

    def rgb(self) -> np.ndarray:
        if self._rgb is None:
            self._rgb = imageio.load(self.path)  # ingest
        return self._rgb

    def matte(self) -> np.ndarray:
        """The subject matte, from the on-disk cache when this photo has been analysed by the same finder before."""
        if self._matte_error is not None:
            raise self._matte_error
        if self._matte is None:
            cached = _inside(self.project.output, self.project.cache / f"{self.subject_key}.npz")
            self._matte = _remembered(cached, self.rgb().shape[:2])
            if self._matte is None:
                try:
                    # analyse (`run` gave a finder: a recipe needs the subject)
                    self._matte = subject_from(self.finder, self.rgb())  # type: ignore[arg-type]  # see above
                except Exception as error:  # remembered, so the photo's other recipes fail without asking again
                    self._matte_error = error
                    raise
                packed = io.BytesIO()
                np.savez_compressed(packed, matte=self._matte.astype(np.float16))
                _write_atomically(cached, packed.getvalue())
        return self._matte

    def find_in_crop(self, crop: np.ndarray) -> np.ndarray:
        """The subject matte of `crop`, a part of one of this photo's working pictures (a recipe's `Subject` with `per`
        asks about each piece), from the on-disk cache when the same finder has been asked about the same pixels before:
        a repeat run asks the model nothing. A crop in which nothing stands out is remembered as an empty matte, so
        the finder is not asked again about it either. The key is the finder, the photo and the crop's pixels, whatever
        recipe, framing or size made it; the answer is kept in 16 bits, so the first run and every later one see the
        same matte."""
        pixels = np.ascontiguousarray(crop)
        key = _fingerprint(self.subject_key, str(pixels.shape), hashlib.sha256(pixels).hexdigest())
        cached = _inside(self.project.output, self.project.cache / f"crop-{key}.npz")
        remembered = _remembered(cached, pixels.shape[:2])
        if remembered is not None:
            return remembered
        try:
            matte = np.asarray(self.finder.find(pixels), np.float32)  # type: ignore[union-attr]  # a recipe needs it
        except SubjectNotFoundError:
            matte = np.zeros(pixels.shape[:2], np.float32)
        packed = io.BytesIO()
        np.savez_compressed(packed, matte=matte.astype(np.float16))
        _write_atomically(cached, packed.getvalue())
        return matte.astype(np.float16).astype(np.float32)


def _remembered(cached: Path, shape: tuple[int, ...]) -> np.ndarray | None:
    """The matte stored in `cached` if it is there, readable and of the photo's `shape`; None otherwise, and a
    damaged entry (cut short, not an archive, the wrong size) is deleted, so it is found again and replaced."""
    try:
        # opened here, not by np.load, which leaves its own file open when the archive is damaged; no pickles: a
        # planted file cannot run code
        with cached.open("rb") as file, np.load(file) as archive:
            matte = archive["matte"]
    except FileNotFoundError:
        return None
    except (OSError, ValueError, KeyError, EOFError, zipfile.BadZipFile):
        matte = None
    if matte is None or matte.shape != shape:
        cached.unlink(missing_ok=True)
        return None
    return matte.astype(np.float32)


def _write_atomically(target: Path, data: bytes) -> None:
    """Write `data` to `target` whole or not at all: into a temporary file beside it, then renamed over it (a rename
    within one folder is atomic), so a crash or Ctrl-C never leaves half a file for the next run to trust."""
    target.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=target.parent, prefix=f".{target.name}.", delete=False) as file:
        temporary = Path(file.name)
    try:
        temporary.write_bytes(data)
        temporary.replace(target)
    except BaseException:
        temporary.unlink(missing_ok=True)
        raise


def _write_text(target: Path, text: str) -> None:
    """Write `text` to `target` as UTF-8, whole or not at all, whatever the locale.

    `text` holds file names. Under a POSIX locale that cannot decode a name's bytes (the C locale with UTF-8 mode off,
    say) Python keeps those bytes as surrogates, which strict UTF-8 refuses; `surrogateescape` writes them back as the
    bytes the name has on disk. Those are UTF-8 on any current system, so the file shows, and links to, the name the
    file system holds."""
    _write_atomically(target, text.encode("utf-8", "surrogateescape"))


def _process(project: Project, photo: _Photo, recipe: Recipe, fingerprint: str) -> Item:
    warnings: list[str] = []
    matte = None
    if recipe.needs_matte:
        matte = photo.matte()
        warnings += matte_warnings(matte, project.subject_share)
    folder = _inside(project.output, project.output / photo.path.stem)
    folder.mkdir(parents=True, exist_ok=True)
    # the photo's own finder of pieces: its answers depend on the photo and the finder, as `subject_key` does
    find = Finder(photo.subject_key, photo.find_in_crop) if recipe.needs_finder else None
    stages = trace(photo.rgb(), recipe, matte=matte, find_subject=find) if project.steps else None
    # style (a trace ends in the render)
    image = stages[-1].image if stages else render(photo.rgb(), recipe, matte=matte, find_subject=find)
    warnings += still_warnings(image)  # check
    outputs = _export_still(project, image, recipe.name, folder)  # export
    sheet = None
    if stages:
        sheet = write_stages(stages, _inside(project.output, folder / "steps" / recipe.name), project.quality)
        outputs.append(sheet)
    # relative and with `/` on every system: they are the report's links (a `\` breaks an href) and the record's paths
    relative = tuple(Path(o).relative_to(project.output).as_posix() for o in outputs)
    sheet_path = sheet.relative_to(project.output).as_posix() if sheet else None
    return Item(
        photo.path.name, recipe.name, "done", relative, tuple(warnings), steps=sheet_path, fingerprint=fingerprint
    )


def _inside(output: Path, target: Path) -> Path:
    """`target`, once it is known to lie inside `output`: every file the pipeline writes goes through here.

    A photo's file name becomes a folder name, and a name such as `...png` (stem `..` before Python 3.14) or a link
    inside the output folder would otherwise carry the writes elsewhere; ProjectError if it does."""
    if not target.resolve().is_relative_to(output.resolve()):
        raise ProjectError(f"{target} is outside the output folder {output}: rename the photo")
    return target


def _sizes(project: Project, native: int) -> list[int | None]:
    """The sizes to export: None = as rendered. Sizes at or above what was rendered mean 'as rendered'."""
    wanted = {None if size >= native else size for size in project.sizes} or {None}
    return sorted(wanted, key=lambda size: (size is None, size), reverse=True)


def _export_still(project: Project, image: np.ndarray, name: str, folder: Path) -> list[Path]:
    outputs = []
    for size in _sizes(project, max(image.shape[:2])):
        picture = image if size is None else resize(image, size)
        for fmt in project.formats:
            target = _inside(project.output, folder / (f"{name}.{fmt}" if size is None else f"{name}_{size}.{fmt}"))
            imageio.save(picture, target, quality=project.quality)
            outputs.append(target)
    return outputs


def _picture_digests(recipe: Recipe) -> list[str]:
    """The content digest of every picture the recipe's steps (and the looks of its `Each` steps) hold, in order."""
    return [
        v.digest for step in recipe.steps for run in step.runs for v in run.params.values() if isinstance(v, Picture)
    ]


def _svg_digests(recipe: Recipe) -> list[str]:
    """The content digest of every svg file the recipe's regions read, in order; RecipeError when one cannot be read."""
    return [
        hashlib.sha256(svg_bytes(kind.file, kind.max_kilobytes)).hexdigest()
        for kind in recipe.regions.values()
        if isinstance(kind, Svg)
    ]


def _fingerprint(*parts: str) -> str:
    return hashlib.sha256("\0".join(parts).encode()).hexdigest()


def _source_digest(package: Path) -> str:
    """A hash of every file shipped under `package` (code, presets, fonts: relative path and bytes), skipping
    `__pycache__` and dotfiles, so changing anything that shapes a result makes the next run redo what it produced."""
    digest = hashlib.sha256()
    for file in sorted(path for path in package.rglob("*") if path.is_file()):
        relative = file.relative_to(package)
        if "__pycache__" in relative.parts or any(part.startswith(".") for part in relative.parts):
            continue
        digest.update(relative.as_posix().encode() + b"\0" + file.read_bytes())
    return digest.hexdigest()


# The libraries whose releases can change the pixels (resampling, filters, the subject model), as major.minor.
_PIXEL_LIBRARIES = ("numpy", "pillow", "scipy", "onnxruntime")


def _library_versions() -> str:
    """The major.minor versions of the installed `_PIXEL_LIBRARIES`, e.g. "numpy 2.3; pillow 11.3; scipy 1.16"."""
    versions = []
    for name in _PIXEL_LIBRARIES:
        try:
            versions.append(f"{name} {'.'.join(metadata.version(name).split('.')[:2])}")
        except metadata.PackageNotFoundError:  # onnxruntime is optional
            continue
    return "; ".join(versions)


def _read_manifest(output: Path) -> list[Item]:
    try:
        raw = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
        return [Item(**item) for item in raw["items"]]  # validated: a damaged record is no record
    except (OSError, ValueError, KeyError, TypeError):
        return []  # no usable earlier run: everything is new


def _report_html(project: Project, items: list[Item]) -> str:
    """A single static page: every result, grouped by photo, with its warnings and errors."""
    counts = {status: sum(i.status == status for i in items) for status in ("done", "skipped", "failed")}
    flagged = sum(bool(i.warnings) for i in items)
    summary = (
        f"{counts['done']} done, {counts['skipped']} up to date, {counts['failed']} failed, {flagged} with warnings"
    )
    sections = []
    for photo in dict.fromkeys(i.photo for i in items):
        source = next(p for p in project.photos if p.name == photo)
        cards = "".join(_card(item) for item in items if item.photo == photo)
        thumbnail = html.escape(_link(source, project.output))
        heading = f'<img class="src" src="{thumbnail}" alt=""> {html.escape(photo)}'
        sections.append(f'<section><h2>{heading}</h2><div class="grid">{cards}</div></section>')
    return _PAGE.format(summary=summary, body="".join(sections))


def _link(target: Path, folder: Path) -> str:
    """How a page in `folder` refers to `target`: a relative path where there is one, else (on another drive, on
    Windows) a `file:` URI."""
    try:
        return target.resolve().relative_to(folder.resolve(), walk_up=True).as_posix()
    except ValueError:
        return target.resolve().as_uri()


def _card(item: Item) -> str:
    """One result: the picture, its name and status, and anything the checks or the run complained about."""
    shown = html.escape((item.outputs or [""])[0])
    media = f'<a href="{shown}"><img src="{shown}" loading="lazy"></a>' if item.outputs else ""
    notes = "".join(f'<p class="warn">{html.escape(w)}</p>' for w in item.warnings)
    if item.error:
        notes += f'<p class="error">{html.escape(item.error)}</p>'
    link = f' <a href="{html.escape(item.steps)}">steps</a>' if item.steps else ""
    caption = f"<b>{html.escape(item.recipe)}</b> <span>{item.status}</span>{link}{notes}"
    return f'<figure class="{item.status}">{media}<figcaption>{caption}</figcaption></figure>'


_PAGE = """<!doctype html>
<meta charset="utf-8"><title>bullterrier-image report</title>
<style>
 body{{margin:0;padding:24px;background:#101114;color:#d8dae0;font:15px/1.4 system-ui,sans-serif}}
 h1{{font-size:20px;margin:0 0 4px}} p.summary{{margin:0 0 24px;color:#8b8f9a}}
 h2{{font-size:16px;display:flex;align-items:center;gap:10px;margin:28px 0 12px}}
 img.src{{height:36px;border-radius:4px}}
 .grid{{display:grid;grid-template-columns:repeat(auto-fill,minmax(260px,1fr));gap:14px}}
 figure{{margin:0;background:#181a20;border-radius:8px;overflow:hidden;border:1px solid #23262e}}
 figure.failed{{border-color:#a33}} figure img{{width:100%;display:block;background:#000}}
 figcaption{{padding:10px 12px}} figcaption span{{color:#8b8f9a;font-size:12px;margin-left:6px}}
 figcaption a{{color:#7fb2ff;font-size:12px;margin-left:8px}}
 .warn{{margin:6px 0 0;color:#e0b050;font-size:13px}} .error{{margin:6px 0 0;color:#f07070;font-size:13px}}
</style>
<h1>bullterrier-image report</h1><p class="summary">{summary}</p>{body}
"""

_STARTER_PROJECT = """# bullterrier-image project: `bt-image run` processes every photo with every recipe.
photos  = ["photos/*.jpg", "photos/*.jpeg", "photos/*.png"]   # globs, relative to this file
recipes = ["recipes"]                                         # recipe files, or folders of them
output  = "out"                                               # results, plus index.html to look through them

[export]
formats = ["jpg"]       # jpg, png or webp
sizes   = []            # longest side(s) in px, e.g. [2048, 1080]; empty = the size each recipe renders at
quality = 92

[checks]                # problems are reported as warnings; they never stop an export
subject_share = [0.02, 0.97]   # a cut-out outside this share of the photo is flagged
"""

_STARTER_RECIPES = {
    "glitch.toml": """# Digital damage: colour channels pulled apart, bands knocked sideways, film grain.
name = "glitch"
size = 1600

[[step]]
effect = "glitch"
split = 9
bands = 8
shift = 0.05
seed = 4

[[step]]
effect = "grain"
amount = 0.07
size = 1.6
""",
    "cutout-black.toml": """# Just the subject, on pure black (the portable model finds it; downloaded on first use).
name = "cutout-black"
size = 1600

[[step]]
effect = "fill"
color = "#000000"
mask = "background"
""",
}
