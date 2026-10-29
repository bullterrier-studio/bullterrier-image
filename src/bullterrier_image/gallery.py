"""The developer commands on a source checkout: what is generated from the effects (the gallery pictures,
`docs/effects/`, `fx.pyi`) and the files of a new effect (`new_effect`, from the template).

Everything about an effect is found by its name, in a layout that mirrors the package: its code in
`src/bullterrier_image/effects/<name>/`, its Rust kernel in `crates/core/src/effects/`, its tests in
`tests/effects/<name>/`, its example recipe in `examples/effects/` and its gallery picture in `docs/gallery/`.
`effect_files` is the one place that says so; everything else asks it. Effects carry their own documentation
(category, docstring, signature), so nothing here is written by hand and no effect needs a shared file edited. The
shared files (`docs/effects/`, `fx.pyi`) are regenerated deliberately, once, at integration (`poe regenerate`), not on
every branch: that is what keeps parallel work free of merge conflicts. These are developer commands; they need a
source checkout of bullterrier-image.
"""

import ast
import inspect
import keyword
import re
from collections.abc import Callable, Sequence
from pathlib import Path
from string import Template
from typing import NamedTuple

import numpy as np

from . import fx, imageio
from .core import registry
from .core.steps import maps_needed
from .errors import ProjectError
from .finding.depth import depth_map
from .finding.subject import subject_matte
from .ops.resample import resize
from .recipe import load_recipe, render

CHECKOUT = (
    Path(__file__).resolve().parents[2]
)  # src/bullterrier_image/gallery.py -> the source checkout bullterrier-image runs from

_SIDE = 480  # px, longest side of a gallery picture
_QUALITY = 82  # JPEG: small files, and these are thumbnails
# how each map an example recipe may need is found, by name: the real models, run once on the demo photo
_FINDERS: dict[str, Callable[[np.ndarray], np.ndarray]] = {
    "subject": subject_matte,
    "depth": depth_map,
}
_INDEX = "README.md"  # the catalogue's index page: GitHub shows it when the folder is opened
# the lowercase strict and reserved keywords of Rust (the Rust Reference, "Keywords", edition 2024): none can name a
# module, so none can name an effect
_RUST_KEYWORDS = frozenset({
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate", "do", "dyn", "else",
    "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "macro", "match", "mod",
    "move", "mut", "override", "priv", "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true",
    "try", "type", "typeof", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
})  # fmt: skip


def render_effects_pages() -> dict[str, str]:
    """The effect catalogue as pages of Markdown, by file name, to be written into `docs/effects/`: `README.md`, the
    index (every category that has effects with its count and description, then every effect alphabetically, linked
    to its place), and `<category>.md` for each category that has effects, in the order of `registry.CATEGORIES`
    (a category without effects gets no page and no link). A category page has, for each effect, its gallery picture,
    name, one-sentence summary, parameters with their defaults and a link to its example recipe, then a table of every
    parameter's type, unit, range, default and description, read from its parameter model. Reads only the effects, so
    the same effects always give the same pages."""
    grouped = {category: names for category, names in registry.categories().items() if names}
    pages = {_INDEX: _index_page(grouped)}
    pages |= {f"{category}.md": _category_page(category, names) for category, names in grouped.items()}
    return pages


class EffectFiles(NamedTuple):
    """Where everything about one effect lives in a source checkout, by its name."""

    package: Path  # src/bullterrier_image/effects/<name>/: its code, `__init__.py` and `effect.py`
    tests: Path  # tests/effects/<name>/test_<name>.py: its behaviour tests
    example: Path  # examples/effects/<name>.toml: the short recipe that shows it, rendered for its picture
    picture: Path  # docs/gallery/<name>.jpg: its gallery picture (JPEG, longest side 480 px), shown by the catalogue
    kernel: Path  # crates/core/src/effects/<name>.rs: its Rust kernel, which its Python body calls (once ported)


def effect_files(name: str, *, checkout: Path = CHECKOUT) -> EffectFiles:
    """Where the files of an effect called `name` live in `checkout`, whether or not they exist (yet)."""
    return EffectFiles(
        package=_layout(checkout).effects / name,
        tests=checkout / "tests" / "effects" / name / f"test_{name}.py",
        example=checkout / "examples" / "effects" / f"{name}.toml",
        picture=checkout / "docs" / "gallery" / f"{name}.jpg",
        kernel=checkout / "crates" / "core" / "src" / "effects" / f"{name}.rs",
    )


def render_gallery(only: Sequence[str] = (), force: bool = False, *, checkout: Path = CHECKOUT) -> list[str]:
    """Render the example recipe of each effect (all, or those named in `only`) on the demo photo into its gallery
    picture (see `effect_files`), longest side 480 px. Pictures that already exist are kept unless `force` is set, so
    nothing changes that does not need to. Returns the names rendered. `checkout` is the bullterrier-image source
    checkout whose example recipes, demo photo (`examples/photos/bullterrier.jpeg`) and pictures are used.

    Everything is checked before anything is rendered: an unknown name raises `UnknownEffectError` listing the
    effects, a missing example recipe or a `checkout` that is no source checkout `ProjectError`. Example recipes that
    cut out the subject need a segmenter (see `bullterrier_image.finding.subject`), those that take depth the depth
    model (`bullterrier_image.finding.depth`); each runs at most once, on the demo photo.
    """
    names = _chosen(only, checkout)
    todo = [name for name in names if force or not effect_files(name, checkout=checkout).picture.exists()]
    if not todo:
        return []
    photo, maps = imageio.load(_layout(checkout).photo), dict[str, np.ndarray]()
    for name in todo:
        files = effect_files(name, checkout=checkout)
        recipe = load_recipe(files.example)
        for needed in maps_needed(recipe) - maps.keys():
            maps[needed] = _FINDERS[needed](photo)  # once for all the recipes that need it
        picture = render(photo, recipe, maps=maps)
        files.picture.parent.mkdir(parents=True, exist_ok=True)
        imageio.save(resize(picture, _SIDE), files.picture, quality=_QUALITY)
    return todo


def write_docs(only: Sequence[str] = (), force: bool = False, *, checkout: Path = CHECKOUT) -> list[Path]:
    """Render gallery pictures in `checkout` (see `render_gallery`), and, unless `only` names effects, write the
    catalogue pages (`render_effects_pages`) into `docs/effects/` first.

    Without `only`, the pages are written, so they are up to date even where pictures cannot be rendered; Markdown
    pages there that the catalogue no longer has (a category that lost its effects) are removed and other files are
    left alone; then the missing pictures (all of them with `force`) are rendered. Returns the pages, then the pictures.

    With `only`, those effects' pictures are rendered, whether they exist or not (naming an effect means "render it",
    so a picture is never kept by surprise after its recipe changed), and nothing else is written: no page, no
    `fx.pyi`, no other file, so an agent adding one effect leaves the shared generated files to the integrator
    (`regenerate`). Returns the pictures.
    """
    _chosen(only, checkout)  # a bad name fails before anything is written
    written = [] if only else _write_pages(checkout)
    rendered = render_gallery(only, force or bool(only), checkout=checkout)
    return [*written, *(effect_files(name, checkout=checkout).picture for name in rendered)]


def _write_pages(checkout: Path) -> list[Path]:
    """Write the catalogue into `docs/effects/` of `checkout`, dropping pages it no longer has; the files written."""
    folder, pages = _layout(checkout).pages, render_effects_pages()
    folder.mkdir(parents=True, exist_ok=True)
    for stale in folder.glob("*.md"):
        if stale.name not in pages:
            stale.unlink()
    for filename, text in pages.items():
        (folder / filename).write_text(text, encoding="utf-8")
    return [folder / filename for filename in pages]


def regenerate(*, checkout: Path = CHECKOUT) -> list[Path]:
    """The one integration step after effects were added or changed: write `src/bullterrier_image/fx.pyi` (for editors)
    in `checkout`, then `write_docs`. Returns the files written. Every effect's example recipe is checked first, so a
    checkout that lacks one gets `ProjectError` and no file is written."""
    _chosen((), checkout)
    stub = _layout(checkout).stub
    stub.write_text(stub_text(), encoding="utf-8")
    return [stub, *write_docs(checkout=checkout)]


def stub_text() -> str:
    """The text of `fx.pyi`: one typed, documented function per effect, for editors, each typed as having `presets`
    too."""
    lines = [
        '"""Generated by `poe regenerate`; do not edit by hand."""',
        "",
        "import collections.abc",
        "import os",
        "from typing import Any, Literal, Protocol, Union",
        "",
        "import numpy",
        "",
        "from .core.params import Drive, Lut, Picture",
        "from .recipe import Step",
        "",
        "class _Effect[**P](Protocol):",
        '    """An effect\'s step builder: call it for a step; `presets` is {preset: {parameter: value}}."""',
        "",
        "    presets: dict[str, dict[str, Any]]",
        "    def __call__(self, *args: P.args, **kwargs: P.kwargs) -> Step: ...",
        "",
        "def _effect[**P](function: collections.abc.Callable[P, Step]) -> _Effect[P]: ...",
        "",
    ]
    for name in sorted(registry.describe()):
        # Python before 3.14 writes `X | None` of a Literal as Optional[X]; a stub is written the short way
        written = inspect.signature(getattr(fx, name)).replace(return_annotation=inspect.Signature.empty)
        signature = re.sub(r"Optional\[(Literal\[[^\]]*\])\]", r"\1 | None", str(written))
        signature = signature.replace("bullterrier_image.core.params.Picture", "Picture")
        signature = signature.replace("bullterrier_image.core.params.Lut", "Lut")
        signature = signature.replace("bullterrier_image.core.params.Drive", "Drive")
        lines += ["@_effect", f"def {name}{signature} -> Step:"]
        doc = registry.documentation(name).replace("\\", "\\\\").replace('"""', "'''")
        lines += [f'    """{doc}"""', "    ...", ""]
    return "\n".join(lines)


def new_effect(name: str, category: str, *, checkout: Path = CHECKOUT) -> list[Path]:
    """Write the files of a new effect called `name`, in `category` (one of `registry.CATEGORIES`), into `checkout`,
    each made from the template in `src/bullterrier_image/effects/_template/` and placed where `effect_files` says: its
    package (`__init__.py`, and `effect.py` with a parameter model with one documented example field and a body that
    calls the core), its Rust kernel (`Params` with the same field, and `render`), its tests (`test_<name>.py`) and its
    example recipe. Returns the files written, sorted.

    The effect is registered whenever bullterrier_image is imported; once the extension is rebuilt with its kernel
    (`uv sync`, which a Rust file's change triggers), it passes the contract tests and its own as written. Replacing the
    example with the real effect is the author's part. Its gallery picture is rendered later, from its example recipe,
    by `poe gallery --only <name>` (or at integration). Raises `ProjectError`, writing nothing, when `name` is not a
    lowercase Python identifier (a letter, then letters, digits and _) or is a Python or Rust keyword (the kernel is a
    Rust module of that name), is a name the template's `effect.py` already uses (its imports) or one `fx` defines for
    itself (so `fx.<name>` would not be the effect), the category is unknown, an effect of that name exists or any of
    its files, or `checkout` holds no template.
    """
    if not re.fullmatch(r"[a-z][a-z0-9_]*", name) or keyword.iskeyword(name):
        raise ProjectError(
            f"{name!r} cannot name an effect: use lowercase letters, digits and _, starting with a letter, and no "
            f"Python keyword"
        )
    if name in _RUST_KEYWORDS:
        raise ProjectError(
            f"{name!r} cannot name an effect: it is a Rust keyword, and the effect's kernel is a Rust module"
        )
    if category not in registry.CATEGORIES:
        raise ProjectError(f"unknown category {category!r}; choose one of: {', '.join(registry.CATEGORIES)}")
    if name in registry.describe():
        raise ProjectError(f"there is already an effect called {name!r}")
    layout, files = _layout(checkout), effect_files(name, checkout=checkout)
    for taken in files:
        if taken.exists():
            raise ProjectError(f"{taken} already exists: choose another name")
    if not (layout.template / "effect.py.tmpl").is_file():
        raise ProjectError(
            f"no effect template in {layout.template}: this needs a source checkout of bullterrier-image"
        )
    if name in _names_in_the_template_effect(layout.template):
        raise ProjectError(f"{name!r} cannot name an effect: the template's effect.py already uses {name!r} for itself")
    if name in _names_fx_defines():
        raise ProjectError(
            f"{name!r} cannot name an effect: fx defines {name!r} for itself, so fx.{name} would not build the effect"
        )
    targets = {  # template file -> where its copy goes
        "__init__.py.tmpl": files.package / "__init__.py",
        "effect.py.tmpl": files.package / "effect.py",
        "test_effect.py.tmpl": files.tests,
        "example.toml.tmpl": files.example,
        "kernel.rs.tmpl": files.kernel,
    }
    words = _words(name, category)
    for template, target in targets.items():
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(
            Template((layout.template / template).read_text(encoding="utf-8")).substitute(words), encoding="utf-8"
        )
    return sorted(targets.values())


def _words(name: str, category: str) -> dict[str, str]:
    """What the template's placeholders stand for: $name, $Name (the name in CamelCase, for the model), $category."""
    return {"name": name, "Name": "".join(part.capitalize() for part in name.split("_")), "category": category}


def _names_in_the_template_effect(template: Path) -> set[str]:
    """The module-level names the template's effect.py binds for itself (its imports, mainly), read from the template,
    so the list follows the template: an effect of one of these names would replace it or be replaced by it."""
    placeholder = "zzplaceholder"  # stands in for the name, so the names made from it can be told apart
    source = Template((template / "effect.py.tmpl").read_text(encoding="utf-8")).substitute(
        _words(placeholder, registry.CATEGORIES[0])
    )
    names: set[str] = set()
    for node in ast.parse(source).body:
        if isinstance(node, ast.Import | ast.ImportFrom):
            names |= {(alias.asname or alias.name).split(".")[0] for alias in node.names}
        elif isinstance(node, ast.ClassDef | ast.FunctionDef):
            names.add(node.name)
        elif isinstance(node, ast.Assign):
            names |= {target.id for target in node.targets if isinstance(target, ast.Name)}
    return {name for name in names if placeholder not in name.lower()}


def _names_fx_defines() -> set[str]:
    """The public names of the `fx` module itself: `fx.<name>` finds those before it would look for an effect."""
    return {name for name in vars(fx) if not name.startswith("_")}


class _Layout(NamedTuple):
    """Where the files shared by all effects live in a source checkout."""

    effects: Path  # src/bullterrier_image/effects, one folder per effect: its presence makes a folder a source checkout
    template: Path  # the files a new effect's folder is made from (string.Template: $name, $Name, $category)
    photo: Path  # the demo photo every example recipe is shown on
    pages: Path  # docs/effects/: the generated catalogue, an index and a page per category
    stub: Path  # fx.pyi, next to fx.py


def _layout(checkout: Path) -> _Layout:
    package = checkout / "src" / "bullterrier_image"
    return _Layout(
        effects=package / "effects",
        template=package / "effects" / "_template",
        photo=checkout / "examples" / "photos" / "bullterrier.jpeg",
        pages=checkout / "docs" / "effects",
        stub=package / "fx.pyi",
    )


def _chosen(only: Sequence[str], checkout: Path) -> list[str]:
    """The effects to work on, checked: each is a known effect with its example recipe in `checkout`."""
    names = list(only) or sorted(registry.describe())
    for name in names:
        registry.get(name)  # UnknownEffectError, listing the effects, for a name that is none
    if not _layout(checkout).effects.is_dir():
        raise ProjectError(
            f"{checkout} has no src/bullterrier_image/effects: this needs a source checkout of bullterrier-image"
        )
    missing = [
        str(path) for path in (effect_files(name, checkout=checkout).example for name in names) if not path.is_file()
    ]
    if missing:
        raise ProjectError(f"no example recipe: add {', '.join(missing)}")
    return names


def _index_page(grouped: dict[str, list[str]]) -> str:
    """The index: a table of the categories (count and description), then every effect in alphabetical order."""
    lines = [
        "# Effects",
        "",
        "Every effect, one page per category, each with a picture of it, its parameters and the example recipe",
        "that made the picture. `uv run bt-image effects --all` prints the same documentation.",
        "",
        "Generated by `poe regenerate` from the effects themselves: do not edit by hand.",
        "",
        "## Categories",
        "",
        "| Category | Effects | What it holds |",
        "| --- | ---: | --- |",
    ]
    for category, names in grouped.items():
        lines.append(
            f"| [{category.capitalize()}]({category}.md) | {len(names)} | {registry.CATEGORY_DESCRIPTIONS[category]} |"
        )
    lines += ["", "## All effects", ""]
    homes = {name: category for category, names in grouped.items() for name in names}
    lines += [f"- [`{name}`]({homes[name]}.md#{name}): {registry.summary(name)}" for name in sorted(homes)]
    return "\n".join(lines) + "\n"


def _category_page(category: str, names: list[str]) -> str:
    """The page of one category: a table of its effects, then each effect's parameter table."""
    lines = [
        f"# {category.capitalize()}",
        "",
        registry.CATEGORY_DESCRIPTIONS[category],
        "",
        f"[All effects]({_INDEX})",
        "",
        "| Example | Effect | What it does | Parameters | Recipe |",
        "| --- | --- | --- | --- | --- |",
    ]
    lines += [_row(name) for name in names]
    for name in names:
        lines += _parameter_table(name)
    return "\n".join(lines) + "\n"


def _row(name: str) -> str:
    # what a recipe sets: the parameters, not the image or `subject`, which the recipe supplies
    docs = registry.parameters(name).fields_doc()
    shown = [f"`{doc.name}={doc.default!r}`" for doc in docs if not doc.advanced]
    if hidden := sum(doc.advanced for doc in docs):
        shown.append(f"+{hidden} advanced")
    files = effect_files(name)
    page_folder = CHECKOUT / "docs" / "effects"  # links are relative to the page, docs/effects/<category>.md
    example = files.example.relative_to(page_folder, walk_up=True).as_posix()
    picture = files.picture.relative_to(page_folder, walk_up=True).as_posix()
    cells = [
        f'<img src="{picture}" width="160" alt="{name}">',
        f"`{name}`",
        registry.summary(name),
        " ".join(shown) or "none",
        f"[{files.example.name}]({example})",
    ]
    return "| " + " | ".join(_cell(text) for text in cells) + " |"


def _parameter_table(name: str) -> list[str]:
    """The lines of an effect's parameter table (name, type, unit, range, default, description), under a heading of its
    own; the advanced parameters follow in a collapsed block of the same form, then the opacity of its steps if it is
    not 1, then the effect's presets."""
    docs = registry.parameters(name).fields_doc()
    head = ["| Parameter | Type | Unit | Range | Default | Description |", "| --- | --- | --- | --- | --- | --- |"]

    def rows(advanced: bool) -> list[str]:
        cells = (
            [f"`{d.name}`", d.type, d.unit or "", d.range, f"`{d.default!r}`", d.doc]
            for d in docs
            if d.advanced == advanced
        )
        return ["| " + " | ".join(_cell(text) for text in row) + " |" for row in cells]

    lines = ["", f"### `{name}`", "", *head, *rows(False)]
    if hidden := rows(True):
        lines += [
            "",
            f"<details><summary>Advanced parameters ({len(hidden)})</summary>",
            "",
            *head,
            *hidden,
            "",
            "</details>",
        ]
    if (opacity := registry.opacity(name)) != 1:
        lines += ["", f"A step of `{name}` has `opacity = {opacity:g}` unless it sets its own."]
    if found := registry.presets(name):
        lines += [
            "",
            'Presets (`preset="name"` in a recipe step or `fx`):',
            "",
            "| Preset | Description |",
            "| --- | --- |",
        ]
        lines += [f"| {_cell(f'`{preset}`')} | {_cell(found[preset].description)} |" for preset in found]
    return lines


def _cell(text: str) -> str:
    """Text that stays inside one table cell: on one line, and with `|` escaped (also inside code)."""
    return " ".join(text.split()).replace("|", "\\|")
