"""The photo pipeline: many photos x many recipes -> exported files, with caching, automatic checks and a report.

Stages, in order, for every photo:

    ingest    read the photo (orientation and colour profile applied, see `imageio`)
    analyse   find the subject once per photo, if any recipe needs it; the result is cached on disk
    style     run each recipe: a still image, or the frames of an animation
    check     automatic quality checks (see `checks`); problems become warnings in the report
    export    write every format and size
    report    `manifest.json` (machine-readable) and `index.html` (a quick look) in the output folder

One photo or recipe failing never stops the others, and work that is already up to date is skipped on the next run,
so a folder of photos can be processed again and again as photos and recipes are added.

A pipeline is described by a small project file, `photo-fx.toml` (`init_project` writes a starter):

    photos  = ["photos/*.jpg", "photos/*.png"]    # globs, relative to this file
    recipes = ["recipes"]                         # recipe files, or folders of them
    output  = "out"

    [export]
    formats = ["jpg"]       # jpg, png or webp, for stills (animations are always .mp4)
    sizes   = [2048, 1080]  # longest side in px; empty = the size the recipe renders at
    quality = 92
    steps   = false         # true: also save the picture after every step, to see what each one did
    steps_at = 0.5          # for animations: the moment (0..1) those pictures are taken at

    [checks]
    subject_share = [0.02, 0.97]   # a cut-out outside this share of the photo is flagged
    frozen_tail   = 0.3            # an animation frozen for more than this share of its length is flagged
"""

import fnmatch
import glob
import hashlib
import html
import json
import time
import tomllib
from collections.abc import Callable, Iterator
from dataclasses import asdict, dataclass, field, replace
from datetime import datetime
from pathlib import Path

import numpy as np

from . import imageio
from .checks import MotionMeter, matte_warnings, still_warnings
from .core.runner import subject_from
from .matte import subject_matte
from .ops.resample import resize
from .recipe import Animation, Recipe, load_recipe, render, render_frames, trace
from .sheet import contact_sheet, write_stages

_FORMATS = ("jpg", "png", "webp")
_PROJECT_FILE = "photo-fx.toml"


@dataclass(frozen=True)
class Project:
    """A pipeline as `photo-fx.toml` describes it (see the module documentation; `load_project` reads one).

    photos: the photo files. recipes: recipe files, or `Recipe` objects built in code. output: the folder everything is
    written to. formats, sizes, quality: how stills are exported (sizes = longest side in px; empty = as rendered).
    subject_share, frozen_tail: the limits of the automatic checks. steps, steps_at: also save the picture after every
    step, taken at that moment (0..1) of an animated recipe.
    """

    photos: tuple[Path, ...]
    recipes: tuple[Path | Recipe, ...]  # recipe files, or Recipe objects built in code
    output: Path
    formats: tuple[str, ...] = ("jpg",)
    sizes: tuple[int, ...] = ()
    quality: int = 92
    subject_share: tuple[float, float] = (0.02, 0.97)
    frozen_tail: float = 0.3
    steps: bool = False  # also save the picture after every step of every recipe
    steps_at: float = 0.5  # for animated recipes: the moment of the clip (0..1) those pictures are from

    @property
    def cache(self) -> Path:
        """The folder of remembered subject mattes, inside `output`, so a photo is analysed once across runs."""
        return self.output / ".cache"


@dataclass(frozen=True)
class Item:
    """What happened to one photo x recipe pair."""

    photo: str
    recipe: str
    status: str  # done | skipped | failed | would-run
    outputs: tuple[str, ...] = ()  # paths relative to the output folder
    warnings: tuple[str, ...] = ()
    error: str | None = None
    steps: str | None = None  # the contact sheet of every step, relative to the output folder
    seconds: float = 0.0
    fingerprint: str = ""


def load_project(path: str | Path) -> Project:
    """Read a project file; globs and folders are resolved relative to the file. Errors say what is wrong."""
    path = Path(path)
    root = path.parent
    with open(path, "rb") as f:
        raw = tomllib.load(f)
    unknown = set(raw) - {"photos", "recipes", "output", "export", "checks"}
    if unknown:
        raise ValueError(f"{path.name}: unknown keys {sorted(unknown)}")
    export, checks = raw.get("export", {}), raw.get("checks", {})
    for table, allowed in (
        (export, {"formats", "sizes", "quality", "steps", "steps_at"}),
        (checks, {"subject_share", "frozen_tail"}),
    ):
        if set(table) - allowed:
            raise ValueError(f"{path.name}: unknown keys {sorted(set(table) - allowed)}; allowed: {sorted(allowed)}")

    photo_globs = raw.get("photos", ["photos/*.jpg", "photos/*.jpeg", "photos/*.png", "photos/*.webp"])
    photos = tuple(
        sorted(
            {
                Path(p)
                for pattern in photo_globs
                for p in glob.glob(str(root / pattern), recursive=True)
                if Path(p).is_file()
            }
        )
    )
    if not photos:
        raise ValueError(f"{path.name}: no photos match {photo_globs} (looked relative to {root.resolve()})")
    stems = [p.stem for p in photos]
    if len(set(stems)) != len(stems):
        raise ValueError(
            f"{path.name}: two photos share a file name ({sorted({s for s in stems if stems.count(s) > 1})})"
        )

    recipes: list[Path] = []
    for entry in raw.get("recipes", ["recipes"]):
        target = root / entry
        recipes += sorted(target.glob("*.toml")) if target.is_dir() else [target]
    if not recipes or not all(r.is_file() for r in recipes):
        raise ValueError(f"{path.name}: no recipe files found in {raw.get('recipes', ['recipes'])}")

    formats = tuple(export.get("formats", ["jpg"]))
    if not formats or any(f not in _FORMATS for f in formats):
        raise ValueError(f"{path.name}: [export] formats must be a list from {_FORMATS}, got {list(formats)}")
    sizes = tuple(export.get("sizes", []))
    if any(not isinstance(s, int) or isinstance(s, bool) or s < 16 for s in sizes):
        raise ValueError(f"{path.name}: [export] sizes must be whole numbers of at least 16 (px), got {list(sizes)}")
    quality = export.get("quality", 92)
    if not isinstance(quality, int) or not 1 <= quality <= 100:
        raise ValueError(f"{path.name}: [export] quality must be 1 to 100, got {quality!r}")
    share = tuple(checks.get("subject_share", (0.02, 0.97)))
    if len(share) != 2 or not 0 <= share[0] < share[1] <= 1:
        raise ValueError(f"{path.name}: [checks] subject_share must be two numbers, low < high, within 0..1")
    frozen = checks.get("frozen_tail", 0.3)
    if not isinstance(frozen, int | float) or not 0 < frozen <= 1:
        raise ValueError(f"{path.name}: [checks] frozen_tail must be a share of the clip, above 0 and up to 1")
    steps, steps_at = export.get("steps", False), export.get("steps_at", 0.5)
    if not isinstance(steps, bool) or not isinstance(steps_at, int | float) or not 0 <= steps_at <= 1:
        raise ValueError(f"{path.name}: [export] steps must be true or false and steps_at a number from 0 to 1")
    return Project(
        photos, tuple(recipes), root / raw.get("output", "out"), formats, tuple(sorted(sizes, reverse=True)),
        quality, share, frozen, steps, float(steps_at),
    )  # fmt: skip


def init_project(directory: str | Path = ".") -> Path:
    """Write a starter `photo-fx.toml`, a `photos/` folder and two small recipes into `directory`; returns the file."""
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
    find_subject: Callable[[np.ndarray], np.ndarray] = subject_matte,
) -> list[Item]:
    """Process every photo with every recipe and write the report; returns what happened to each pair.

    Pairs whose photo, recipe, export settings and the tool's own code are unchanged since the last run, and whose
    files still exist, are skipped unless `force`. `only` is a file-name pattern selecting some of the photos.
    `dry_run` reports what would be done and writes nothing. Recipes are validated first, so a typo fails at once.
    `find_subject(rgb) -> matte` finds the subject of a photo for the recipes that need it (default: `subject_matte`).
    It is asked at most once per photo, and never again for a photo whose subject is remembered in the project's
    cache from an earlier run; if it raises, the recipes of that photo that need the subject fail, and the rest run.
    The cache knows the photo, not the finder: after switching to a different `find_subject`, delete `project.cache`,
    or the subjects the earlier finder found are used again.
    """
    recipes = [(r, r if isinstance(r, Recipe) else load_recipe(r)) for r in project.recipes]
    names = [recipe.name for _, recipe in recipes]
    if len(set(names)) != len(names):
        raise ValueError(f"two recipes are called the same: {sorted({n for n in names if names.count(n) > 1})}")
    photos = [p for p in project.photos if only is None or fnmatch.fnmatch(p.name, only)]
    if not photos:
        raise ValueError(f"no photos match --only {only!r}")

    previous = {(item.photo, item.recipe): item for item in _read_manifest(project.output)}
    settings = repr(
        (
            project.formats,
            project.sizes,
            project.quality,
            project.subject_share,
            project.frozen_tail,
            project.steps,
            project.steps_at,
        )
    )
    code = _code_fingerprint()
    items: list[Item] = []
    total = len(photos) * len(recipes)
    for photo in photos:
        photo_hash = hashlib.sha256(photo.read_bytes()).hexdigest()
        state = _Photo(photo, photo_hash, project, find_subject)
        for path, recipe in recipes:
            label = f"[{len(items) + 1}/{total}] {photo.name} x {recipe.name}"
            fingerprint = _fingerprint(
                photo_hash, repr(path) if isinstance(path, Recipe) else path.read_text(encoding="utf-8"), settings, code
            )
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
                started = time.time()
                try:
                    item = _process(project, state, recipe, fingerprint)
                except Exception as error:  # one bad photo or recipe must not stop the batch
                    item = Item(
                        photo.name,
                        recipe.name,
                        "failed",
                        error=f"{type(error).__name__}: {error}",
                        fingerprint=fingerprint,
                    )
                item = replace(item, seconds=round(time.time() - started, 2))
                items.append(item)
                log(
                    f"{label}: {item.status}"
                    + (f" ({'; '.join(item.warnings)})" if item.warnings else "")
                    + (f" {item.error}" if item.error else "")
                )
    if not dry_run:
        # The record keeps earlier results for photos this run left alone (--only); the return value does not.
        seen = {(i.photo, i.recipe) for i in items}
        photo_names = {p.name for p in project.photos}
        record = items + [
            i for key, i in previous.items() if key not in seen and i.photo in photo_names and i.recipe in names
        ]
        record.sort(key=lambda i: (i.photo, names.index(i.recipe)))
        project.output.mkdir(parents=True, exist_ok=True)
        (project.output / "manifest.json").write_text(
            json.dumps(
                {
                    # local time, for the person reading the report; zone-aware would change the manifest's format
                    "generated": datetime.now().isoformat(timespec="seconds"),  # noqa: DTZ005
                    "code": code,
                    "items": [asdict(i) for i in record],
                },
                indent=2,
            ),
            encoding="utf-8",
        )
        (project.output / "index.html").write_text(_report_html(project, record), encoding="utf-8")
    return items


@dataclass
class _Photo:
    """One photo while its recipes run: read and analysed once, on first use."""

    path: Path
    hash: str
    project: Project
    find_subject: Callable[[np.ndarray], np.ndarray]
    _rgb: np.ndarray | None = field(default=None, repr=False)
    _matte: np.ndarray | None = field(default=None, repr=False)
    _matte_error: Exception | None = field(default=None, repr=False)

    def rgb(self) -> np.ndarray:
        if self._rgb is None:
            self._rgb = imageio.load(self.path)  # ingest
        return self._rgb

    def matte(self) -> np.ndarray:
        """The subject matte, from the on-disk cache when this exact photo has been analysed before."""
        if self._matte_error is not None:
            raise self._matte_error
        if self._matte is None:
            cached = self.project.cache / f"{self.hash}.npz"
            if cached.exists():
                self._matte = np.load(cached)["matte"].astype(np.float32)
            else:
                try:
                    self._matte = subject_from(self.find_subject, self.rgb())  # analyse
                except Exception as error:
                    self._matte_error = error
                    raise
                self.project.cache.mkdir(parents=True, exist_ok=True)
                np.savez_compressed(cached, matte=self._matte.astype(np.float16))
        return self._matte


def _process(project: Project, photo: _Photo, recipe: Recipe, fingerprint: str) -> Item:
    warnings: list[str] = []
    matte = None
    if recipe.needs_matte:
        matte = photo.matte()
        warnings += matte_warnings(matte, project.subject_share)
    folder = project.output / photo.path.stem
    folder.mkdir(parents=True, exist_ok=True)
    stages = trace(photo.rgb(), recipe, matte, at=project.steps_at) if project.steps else None
    if recipe.animation is not None:
        outputs = _export_animation(project, photo, recipe, recipe.animation, matte, folder, warnings)
    else:
        image = stages[-1].image if stages else render(photo.rgb(), recipe, matte)  # style (a trace ends in the render)
        warnings += still_warnings(image)  # check
        outputs = _export_still(project, image, recipe.name, folder)  # export
    sheet = None
    if stages:
        sheet = write_stages(stages, folder / "steps" / recipe.name, project.quality)
        outputs.append(sheet)
    relative = tuple(str(Path(o).relative_to(project.output)) for o in outputs)
    sheet_path = str(sheet.relative_to(project.output)) if sheet else None
    return Item(
        photo.path.name, recipe.name, "done", relative, tuple(warnings), steps=sheet_path, fingerprint=fingerprint
    )


def _sizes(project: Project, native: int) -> list[int | None]:
    """The sizes to export: None = as rendered. Sizes at or above what was rendered mean 'as rendered'."""
    wanted = {None if size >= native else size for size in project.sizes} or {None}
    return sorted(wanted, key=lambda size: (size is None, size), reverse=True)


def _export_still(project: Project, image: np.ndarray, name: str, folder: Path) -> list[Path]:
    outputs = []
    for size in _sizes(project, max(image.shape[:2])):
        picture = image if size is None else resize(image, size)
        for fmt in project.formats:
            target = folder / (f"{name}.{fmt}" if size is None else f"{name}_{size}.{fmt}")
            imageio.save(picture, target, quality=project.quality)
            outputs.append(target)
    return outputs


_FILMSTRIP_FRAMES = 6  # how many frames of an animation go into its preview picture


def _export_animation(
    project: Project,
    photo: _Photo,
    recipe: Recipe,
    animation: Animation,
    matte: np.ndarray | None,
    folder: Path,
    warnings: list[str],
) -> list[Path]:
    """Render the clip of `recipe` (whose timeline is `animation`) to video, plus a filmstrip picture of a few of its
    frames, and check its motion."""
    meter = MotionMeter()
    fps = animation.fps
    total = len(animation.frames())
    keep = {int(i) for i in np.linspace(0, total - 1, _FILMSTRIP_FRAMES).round()}
    strip: list[tuple[str, np.ndarray]] = []

    def frames() -> Iterator[np.ndarray]:
        for number, frame in enumerate(render_frames(photo.rgb(), recipe, matte)):  # style
            meter.add(frame)  # check
            if number in keep:
                strip.append((f"t = {number / fps:.1f} s", frame))
            yield frame

    native = folder / f"{recipe.name}.mp4"
    imageio.save_video(frames(), native, fps=fps)  # export
    warnings += meter.warnings(project.frozen_tail)
    filmstrip = folder / f"{recipe.name}.frames.png"  # a picture of the animation, for places a video cannot go
    imageio.save(contact_sheet(strip, columns=len(strip), tile=340), filmstrip)
    outputs = [native, filmstrip]
    # (an empty clip leaves no measured size; `save_video` refuses one first, but then there is nothing to shrink)
    for size in _sizes(project, max(meter.shape) if meter.shape is not None else 0):
        if size is not None:
            smaller = folder / f"{recipe.name}_{size}.mp4"
            imageio.resize_video(native, smaller, size)
            outputs.append(smaller)
    return outputs


def _fingerprint(*parts: str) -> str:
    return hashlib.sha256("\0".join(parts).encode()).hexdigest()


def _code_fingerprint() -> str:
    """A hash of the tool's own source, so changing an effect makes the next run redo what it produced."""
    digest = hashlib.sha256()
    for source in sorted(Path(__file__).parent.rglob("*.py")):
        digest.update(source.read_bytes())
    return digest.hexdigest()


def _read_manifest(output: Path) -> list[Item]:
    try:
        raw = json.loads((output / "manifest.json").read_text(encoding="utf-8"))
        return [Item(**{**i, "outputs": tuple(i["outputs"]), "warnings": tuple(i["warnings"])}) for i in raw["items"]]
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
    """One result: the video or picture, its name and status, and anything the checks or the run complained about."""
    videos = [o for o in item.outputs if o.endswith(".mp4")]
    shown = html.escape((videos or item.outputs or [""])[0])
    if videos:
        media = f'<video src="{shown}" controls loop muted playsinline></video>'
    elif item.outputs:
        media = f'<a href="{shown}"><img src="{shown}" loading="lazy"></a>'
    else:
        media = ""
    notes = "".join(f'<p class="warn">{html.escape(w)}</p>' for w in item.warnings)
    if item.error:
        notes += f'<p class="error">{html.escape(item.error)}</p>'
    link = f' <a href="{html.escape(item.steps)}">steps</a>' if item.steps else ""
    for strip in (o for o in item.outputs if o.endswith(".frames.png")):
        link += f' <a href="{html.escape(strip)}">frames</a>'
    caption = f"<b>{html.escape(item.recipe)}</b> <span>{item.status}</span>{link}{notes}"
    return f'<figure class="{item.status}">{media}<figcaption>{caption}</figcaption></figure>'


_PAGE = """<!doctype html>
<meta charset="utf-8"><title>photo-fx report</title>
<style>
 body{{margin:0;padding:24px;background:#101114;color:#d8dae0;font:15px/1.4 system-ui,sans-serif}}
 h1{{font-size:20px;margin:0 0 4px}} p.summary{{margin:0 0 24px;color:#8b8f9a}}
 h2{{font-size:16px;display:flex;align-items:center;gap:10px;margin:28px 0 12px}}
 img.src{{height:36px;border-radius:4px}}
 .grid{{display:grid;grid-template-columns:repeat(auto-fill,minmax(260px,1fr));gap:14px}}
 figure{{margin:0;background:#181a20;border-radius:8px;overflow:hidden;border:1px solid #23262e}}
 figure.failed{{border-color:#a33}} figure img,figure video{{width:100%;display:block;background:#000}}
 figcaption{{padding:10px 12px}} figcaption span{{color:#8b8f9a;font-size:12px;margin-left:6px}}
 figcaption a{{color:#7fb2ff;font-size:12px;margin-left:8px}}
 .warn{{margin:6px 0 0;color:#e0b050;font-size:13px}} .error{{margin:6px 0 0;color:#f07070;font-size:13px}}
</style>
<h1>photo-fx report</h1><p class="summary">{summary}</p>{body}
"""

_STARTER_PROJECT = """# photo-fx project: `photo-fx run` processes every photo with every recipe.
photos  = ["photos/*.jpg", "photos/*.jpeg", "photos/*.png"]   # globs, relative to this file
recipes = ["recipes"]                                         # recipe files, or folders of them
output  = "out"                                               # results, plus index.html to look through them

[export]
formats = ["jpg"]       # jpg, png or webp for stills (animations are always .mp4)
sizes   = []            # longest side(s) in px, e.g. [2048, 1080]; empty = the size each recipe renders at
quality = 92

[checks]                # problems are reported as warnings; they never stop an export
subject_share = [0.02, 0.97]   # a cut-out outside this share of the photo is flagged
frozen_tail   = 0.3            # an animation frozen for more than this share of its length is flagged
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
    "cutout-black.toml": """# Just the subject, on pure black (macOS: uses Apple Vision to find the subject).
name = "cutout-black"
size = 1600

[[step]]
effect = "fill"
color = "#000000"
mask = "background"
""",
}
