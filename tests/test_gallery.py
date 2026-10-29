"""The generated effect catalogue: docs/effects/ (an index and a page per category), the gallery pictures, `poe gallery`
and `poe regenerate`.

These tests build their own effects (throwaway or in a fresh registry) and their own example recipes, so they do not
depend on which real effects exist or how they are documented.
"""

import io
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Literal

import numpy as np
import pytest
from PIL import Image, JpegImagePlugin

import bullterrier_image
from bullterrier_image import ProjectError, UnknownEffectError, gallery, imageio
from bullterrier_image.core import registry
from bullterrier_image.core.params import Color, EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.finding.depth import depth_map
from bullterrier_image.ops.resample import resize


class AmountParams(EffectParams):
    """The one parameter of a probe effect."""

    amount: float = param(0.5, unit="fraction", min=0, max=1, doc="How much of the picture a probe keeps.")


def _row(page: str, name: str) -> str:
    """The table row of effect `name` on a category page (the rows start with its picture)."""
    (row,) = [line for line in page.splitlines() if line.startswith("| <img") and f'alt="{name}"> | `{name}` |' in line]
    return row


def _unescaped_pipes(row: str) -> int:
    return len(re.findall(r"(?<!\\)\|", row))


def _slug(heading: str) -> str:
    """The anchor GitHub gives a heading: lower case, punctuation dropped, spaces to hyphens."""
    return re.sub(r"[^\w\s-]", "", heading.lower()).replace(" ", "-")


def _dangling_links(pages: dict[str, str]) -> list[str]:
    """The links between pages that lead nowhere: to a page that is not there, or to an anchor its page lacks."""
    anchors = {
        filename: {_slug(heading) for heading in re.findall(r"^#+ (.+)$", text, flags=re.M)}
        for filename, text in pages.items()
    }
    dangling = []
    for text in pages.values():
        for target in re.findall(r"\]\(([^)\s]+\.md(?:#[^)\s]*)?)\)", text):
            filename, _, anchor = target.partition("#")
            if filename not in pages or (anchor and anchor not in anchors[filename]):
                dangling.append(target)
    return dangling


def _index_rows(pages: dict[str, str]) -> dict[str, list[str]]:
    """The category table of the index: category page -> its cells (name link, count, description)."""
    rows = [
        [cell.strip() for cell in line.strip("|").split("|")]
        for line in pages["README.md"].splitlines()
        if re.match(r"\| \[\w+\]\(\w+\.md\) \|", line)
    ]
    return {re.search(r"\((\w+\.md)\)", cells[0]).group(1): cells for cells in rows}  # type: ignore[union-attr]


# --- the effects pages -------------------------------------------------------------------------------------------


def test_a_category_has_a_page_only_while_it_has_effects(fresh_registry, register_probe):
    register_probe("lamp", "light")
    pages = gallery.render_effects_pages()
    assert set(pages) == {"README.md", "light.md"}  # no page, no index row and no link for a category without effects
    assert "distortion" not in pages["README.md"].lower()
    register_probe("swirl", "distortion")
    pages = gallery.render_effects_pages()
    assert set(pages) == {"README.md", "light.md", "distortion.md"}
    assert "`swirl`" in pages["distortion.md"]
    assert "`swirl`" not in pages["light.md"]


def test_the_index_lists_every_effect_once_alphabetically_linking_to_its_category_page(fresh_registry, register_probe):
    for name, category in [("zoom", "light"), ("tint", "colour"), ("amber", "colour"), ("drift", "utilities")]:
        register_probe(name, category)
    listed = re.findall(r"^- \[`(\w+)`\]\((\S+)\)", gallery.render_effects_pages()["README.md"], flags=re.M)
    assert listed == [
        ("amber", "colour.md#amber"),
        ("drift", "utilities.md#drift"),
        ("tint", "colour.md#tint"),
        ("zoom", "light.md#zoom"),
    ]


def test_the_index_gives_each_category_its_count_and_one_line_description_in_catalogue_order(
    fresh_registry, register_probe
):
    for name, category in [("drift", "utilities"), ("tint", "colour"), ("hue", "colour"), ("lamp", "light")]:
        register_probe(name, category)  # registered in another order than the catalogue's
    rows = _index_rows(gallery.render_effects_pages())
    assert list(rows) == ["colour.md", "light.md", "utilities.md"]
    assert [cells[0] for cells in rows.values()] == [
        "[Colour](colour.md)",
        "[Light](light.md)",
        "[Utilities](utilities.md)",
    ]
    assert [cells[1] for cells in rows.values()] == ["2", "1", "1"]


def test_every_category_has_a_description_in_the_index(fresh_registry, register_probe):
    categories = list(registry.CATEGORIES)
    for index, category in enumerate(categories):
        register_probe(f"probe{index}", category)
    rows = _index_rows(gallery.render_effects_pages())
    assert len(rows) == len(categories)
    descriptions = [cells[2] for cells in rows.values()]
    assert all(description for description in descriptions)
    assert len(set(descriptions)) == len(descriptions)  # one line of its own for each, none a copy of another


def test_every_real_effect_is_listed_once_in_its_own_category_page():
    pages = gallery.render_effects_pages()
    for name in registry.describe():
        # an effect's row in the catalogue table: its picture, then its name (a parameter row starts with its name)
        homes = [filename for filename, text in pages.items() if f'alt="{name}"> | `{name}` |' in text]
        assert homes == [f"{registry.category(name)}.md"], name


@pytest.mark.parametrize(
    ("pages", "dangling"),
    [
        pytest.param({"a.md": "[b](b.md#top)", "b.md": "# Top"}, [], id="good-link-and-anchor"),
        pytest.param({"a.md": "[b](b.md)"}, ["b.md"], id="page-missing"),
        pytest.param({"a.md": "[b](b.md#gone)", "b.md": "# Top"}, ["b.md#gone"], id="anchor-missing"),
    ],
)
def test_the_link_check_finds_dangling_links_and_passes_good_ones(pages, dangling):
    assert _dangling_links(pages) == dangling


def test_every_link_between_the_real_pages_resolves_to_a_page_and_a_heading():
    pages = gallery.render_effects_pages()
    assert len(re.findall(r"\]\([^)]+\.md", pages["README.md"])) > len(registry.describe())  # there are links to check
    assert _dangling_links(pages) == []


def test_rendering_the_pages_twice_gives_the_same_text(fresh_registry, register_probe):
    register_probe("lamp", "light")
    register_probe("tint", "colour")
    assert gallery.render_effects_pages() == gallery.render_effects_pages()


class RowParams(EffectParams):
    """The row probe's settings."""

    radius: float = param(24.0, unit="px", min=0, max=500, doc="Unused.")
    chars: str = param(" .:", doc="Unused.")
    ink: Color | None = param(None, doc="Unused.")


def test_an_effects_row_has_its_picture_summary_parameters_and_recipe(fresh_registry):
    @effect(category="light", params=RowParams)
    def row_probe(rgb, params):
        """Softly probes the bright parts. Then says more."""
        return rgb

    row = _row(gallery.render_effects_pages()["light.md"], "row_probe")
    assert 'src="../gallery/row_probe.jpg"' in row
    assert "| Softly probes the bright parts. |" in row  # the first sentence only
    assert "`radius=24.0` `chars=' .:'` `ink=None`" in row  # what a recipe sets, in order, with defaults
    # the recipe supplies the image and the cut-out itself
    assert "rgb" not in row
    assert "matte" not in row
    assert "[row_probe.toml](../../examples/effects/row_probe.toml)" in row
    assert _unescaped_pipes(row) == 6  # five columns


class HaloParams(EffectParams):
    """The halo probe's settings."""

    radius: float = param(24.0, unit="px", min=0, max=500, doc="Width of the halo | in px. Larger = softer.")
    mode: Literal["screen", "add"] = param("screen", doc="How the halo meets the picture: 'screen' or 'add'.")


def test_a_modelled_effect_gets_a_table_of_its_parameters(fresh_registry):
    @effect(category="light", params=HaloParams)
    def halo_probe(rgb, params):
        """Puts a halo around the bright parts."""
        return rgb

    page = gallery.render_effects_pages()["light.md"]
    assert "`radius=24.0` `mode='screen'`" in _row(page, "halo_probe")  # the short form in the effect's row
    table = page[page.index("### `halo_probe`") :]
    lines = [line for line in table.splitlines() if line.startswith("|")]
    assert lines[0] == "| Parameter | Type | Unit | Range | Default | Description |"
    assert lines[2] == "| `radius` | number | px | 0 to 500 | `24.0` | Width of the halo \\| in px. Larger = softer. |"
    assert (
        lines[3]
        == "| `mode` | one of 'screen', 'add' |  |  | `'screen'` | How the halo meets the picture: 'screen' or 'add'. |"
    )


def test_an_effect_with_presets_lists_them_under_its_parameter_table():
    page = gallery.render_effects_pages()["colour.md"]
    table = page[page.index("### `gradient_map`") :].split("\n### ")[0]
    for name, preset in registry.presets("gradient_map").items():
        assert f"| `{name}` | {preset.description} |" in table
    assert "Presets" not in page[page.index("### `equalize`") :].split("\n### ")[0]


def test_an_effect_that_sets_its_steps_opacity_says_so_under_its_parameter_table(throwaway_effects):
    @effect(category="light", params=AmountParams, opacity=0.4)
    def faint_probe(rgb, params):
        """Lightens faintly."""
        return rgb

    page = gallery.render_effects_pages()["light.md"]
    assert "A step of `faint_probe` has `opacity = 0.4` unless it sets its own." in page
    assert "unless it sets its own" not in page[page.index("### `bloom`") :].split("\n### ")[0]


class TunedParams(EffectParams):
    """The tuned probe's settings: a technique option between two look controls."""

    radius: float = param(24.0, unit="px", min=0, max=500, doc="Width of the halo.")
    edge: Literal["reflect", "wrap"] = param("reflect", advanced=True, doc="Past the border: 'reflect' or 'wrap'.")
    mode: Literal["screen", "add"] = param("screen", doc="How the halo meets the picture: 'screen' or 'add'.")
    softness: float = param(2.0, unit="px", min=0, max=10, advanced=True, doc="Blur of the mask.")


def test_advanced_parameters_leave_the_row_and_fill_a_collapsed_table_under_the_basic_one(fresh_registry):
    @effect(category="light", params=TunedParams)
    def tuned_probe(rgb, params):
        """Tunes the halo."""
        return rgb

    page = gallery.render_effects_pages()["light.md"]
    row = _row(page, "tuned_probe")
    assert "`radius=24.0` `mode='screen'` +2 advanced" in row
    assert "edge" not in row
    assert "softness" not in row
    table = page[page.index("### `tuned_probe`") :]
    basic, collapsed = table.split("<details><summary>Advanced parameters (2)</summary>")
    assert [line.split(" | ")[0] for line in basic.splitlines() if line.startswith("| `")] == ["| `radius`", "| `mode`"]
    body, after = collapsed.split("</details>")
    assert body.startswith(
        "\n\n| Parameter | Type | Unit | Range | Default | Description |\n"
    )  # blank line: GitHub renders it
    assert body.endswith("|\n\n")
    assert [line.split(" | ")[0] for line in body.splitlines() if line.startswith("| `")] == [
        "| `edge`",
        "| `softness`",
    ]
    assert "| `softness` | number | px | 0 to 10 | `2.0` | Blur of the mask. |" in body
    assert "`radius`" not in body
    assert "tuned_probe" not in after


def test_an_effect_without_advanced_parameters_has_no_collapsed_block_and_no_count(fresh_registry):
    @effect(category="light", params=HaloParams)
    def halo_probe(rgb, params):
        """Puts a halo around the bright parts."""
        return rgb

    page = gallery.render_effects_pages()["light.md"]
    assert "<details>" not in page
    assert "advanced" not in _row(page, "halo_probe")


class PipeParams(EffectParams):
    """The pipe probe's settings."""

    chars: str = param("a|b", doc="Unused.")
    amount: float = param(0.5, unit="fraction", min=0, max=1, doc="Unused.")


def test_effects_page_escapes_table_breaking_characters(throwaway_effects):
    @effect(category="utilities", params=PipeParams)
    def pipe_probe(rgb, params):
        """Draws a | b
        across two lines. More."""
        return rgb

    row = _row(gallery.render_effects_pages()["utilities.md"], "pipe_probe")
    assert "`chars='a\\|b'`" in row
    assert "Draws a \\| b across two lines." in row
    assert _unescaped_pipes(row) == 6  # still five columns


# --- the gallery pictures ----------------------------------------------------------------------------------------

REAL_EXAMPLES = Path(__file__).parent.parent / "examples" / "effects"


@pytest.fixture
def checkout(tmp_path, photo):
    """A scratch source checkout to generate into: a copy of this bullterrier-image package (without fx.pyi, so writing
    one shows), a copy of the example recipes, a demo photo, and the empty folder the pictures go to. Nothing in the
    real
    checkout is written."""
    root = tmp_path / "checkout"
    package = root / "src" / "bullterrier_image"
    shutil.copytree(Path(bullterrier_image.__file__).parent, package, ignore=shutil.ignore_patterns("__pycache__"))
    (package / "fx.pyi").unlink()
    shutil.copytree(REAL_EXAMPLES, root / "examples" / "effects")
    (root / "examples" / "photos").mkdir()
    imageio.save(photo, root / "examples" / "photos" / "bullterrier.jpeg")
    (root / "docs" / "gallery").mkdir(parents=True)
    return root


@pytest.fixture
def probe(checkout, throwaway_effects):
    """A throwaway effect with an example recipe of its own in the scratch checkout. It needs no cut-out, so it
    renders anywhere. Its recipe renders at 300 px, to show the gallery picture is 480 px whatever the recipe says."""

    @effect(category="utilities", params=AmountParams)
    def gallery_probe(rgb, params):
        """Darkens the picture."""
        return (rgb * params.amount).astype(np.float32)

    (checkout / "examples" / "effects" / "gallery_probe.toml").write_text(
        'name = "gallery_probe"\nsize = 300\n[[step]]\neffect = "gallery_probe"\n', encoding="utf-8"
    )
    return "gallery_probe"


def _quantization(source) -> dict:
    with Image.open(source) as picture:
        assert isinstance(picture, JpegImagePlugin.JpegImageFile)
        return picture.quantization


def _jpeg_tables(quality: int) -> dict:
    """The quantization tables Pillow writes for `quality`: they identify the quality setting of a JPEG."""
    buffer = io.BytesIO()
    Image.new("RGB", (16, 16), (90, 120, 150)).save(buffer, "JPEG", quality=quality)
    return _quantization(buffer)


# --- where an effect's files are ----------------------------------------------------------------------------------


def test_an_effects_files_are_found_by_its_name_in_the_layout_that_mirrors_the_package(checkout):
    files = gallery.effect_files("lamp", checkout=checkout)
    assert files.package == checkout / "src" / "bullterrier_image" / "effects" / "lamp"
    assert files.tests == checkout / "tests" / "effects" / "lamp" / "test_lamp.py"
    assert files.example == checkout / "examples" / "effects" / "lamp.toml"
    assert files.picture == checkout / "docs" / "gallery" / "lamp.jpg"
    assert files.kernel == checkout / "crates" / "core" / "src" / "effects" / "lamp.rs"


def test_every_row_of_the_real_pages_links_to_its_effects_own_picture_and_recipe():
    pages, docs = gallery.render_effects_pages(), gallery.CHECKOUT / "docs" / "effects"
    for name in registry.describe():
        files, row = gallery.effect_files(name), _row(pages[f"{registry.category(name)}.md"], name)
        (picture,) = re.findall(r'src="([^"]+)"', row)
        (recipe,) = re.findall(r"\]\(([^)]+)\)", row)
        assert (docs / picture).resolve() == files.picture.resolve(), name
        assert (docs / recipe).resolve() == files.example.resolve(), name


# --- rendering ---------------------------------------------------------------------------------------------------


def test_gallery_renders_missing_pictures_only_unless_forced(checkout, probe):
    picture = gallery.effect_files(probe, checkout=checkout).picture
    assert gallery.render_gallery(only=[probe], checkout=checkout) == [probe]
    first = picture.read_bytes()
    picture.write_bytes(b"placeholder")
    assert gallery.render_gallery(only=[probe], checkout=checkout) == []  # it exists, so it is left alone
    assert picture.read_bytes() == b"placeholder"
    assert gallery.render_gallery(only=[probe], force=True, checkout=checkout) == [probe]
    assert picture.read_bytes() == first  # rendered again, and rendering is deterministic


def test_gallery_pictures_are_jpegs_480_px_on_the_longest_side_at_quality_82(checkout, probe):
    gallery.render_gallery(only=[probe], checkout=checkout)
    with Image.open(gallery.effect_files(probe, checkout=checkout).picture) as picture:
        # from a 300 px recipe
        assert picture.format == "JPEG"
        assert max(picture.size) == 480
    assert _jpeg_tables(82) != _jpeg_tables(90)  # the tables do tell qualities apart
    assert _quantization(gallery.effect_files(probe, checkout=checkout).picture) == _jpeg_tables(82)


def test_gallery_pictures_show_the_checkouts_demo_photo_through_the_example_recipe(checkout, probe, photo):
    gallery.render_gallery(only=[probe], checkout=checkout)
    picture = imageio.load(gallery.effect_files(probe, checkout=checkout).picture)
    expected = resize(photo * 0.5, 480)  # the probe halves the brightness; the demo photo is `photo`
    # JPEG and resampling only
    assert picture.shape == expected.shape
    assert np.abs(picture - expected).mean() < 0.02


def test_gallery_pictures_of_a_recipe_that_takes_depth_show_the_depth_models_map(
    checkout, depth_probe, depth_echo_model
):
    depth_echo_model(0)  # a stand-in for the depth model: near where the demo photo is red
    (checkout / "examples" / "effects" / "depth_probe.toml").write_text(
        'name = "depth_probe"\nsize = 128\n[[step]]\neffect = "depth_probe"\n', encoding="utf-8"
    )
    gallery.render_gallery(only=["depth_probe"], checkout=checkout)
    picture = imageio.load(gallery.effect_files("depth_probe", checkout=checkout).picture)
    expected = resize(depth_map(imageio.load(checkout / "examples" / "photos" / "bullterrier.jpeg")), 480)
    assert picture.shape[:2] == expected.shape
    assert np.abs(picture[..., 0] - expected).mean() < 0.02  # JPEG and resampling only


def test_gallery_rejects_an_unknown_effect_before_rendering(checkout, probe):
    with pytest.raises(UnknownEffectError, match="nope") as raised:
        gallery.render_gallery(only=[probe, "nope"], checkout=checkout)
    # the name, and the choices
    assert "'nope'" in str(raised.value)
    assert "gallery_probe" in str(raised.value)
    assert not gallery.effect_files(probe, checkout=checkout).picture.exists()


def test_gallery_needs_an_example_recipe_for_each_effect_it_renders(checkout, probe):
    @effect(category="utilities", params=AmountParams)
    def recipe_less(rgb, params):
        """Has no example recipe."""
        return rgb

    with pytest.raises(ProjectError, match=r"recipe_less\.toml"):
        gallery.render_gallery(only=[probe, "recipe_less"], checkout=checkout)
    assert not gallery.effect_files(probe, checkout=checkout).picture.exists()


def test_regenerate_checks_the_checkout_before_writing_anything(checkout):
    gallery.effect_files("grain", checkout=checkout).example.unlink()  # one effect without its example recipe
    with pytest.raises(ProjectError, match="grain"):
        gallery.regenerate(checkout=checkout)
    assert not (checkout / "src" / "bullterrier_image" / "fx.pyi").exists()
    assert not (checkout / "docs" / "effects").exists()


def test_gallery_outside_a_source_checkout_says_it_needs_one(tmp_path):
    with pytest.raises(ProjectError, match="source checkout"):
        gallery.render_gallery(only=["grain"], checkout=tmp_path / "not-a-checkout")
    assert not (tmp_path / "not-a-checkout").exists()


# --- writing everything: `poe gallery` and `poe regenerate` (tools/dev.py) --------------------------------------------


# The commands always work on the checkout it runs from. To run them without touching the real one, each test
# runs the script in a fresh interpreter from a copy of the package placed in the scratch checkout's src/: imported
# from there, bullterrier-image takes the scratch checkout for its own.

DEV_SCRIPT = Path(__file__).resolve().parents[1] / "tools" / "dev.py"


@pytest.fixture
def docs(checkout):
    """The scratch checkout with every effect's gallery picture already there (so none renders)."""
    for name in registry.describe():
        gallery.effect_files(name, checkout=checkout).picture.write_bytes(b"existing")
    return checkout


def dev_command(checkout: Path, *args: str) -> subprocess.CompletedProcess:
    """`tools/dev.py ARGS`, run from the copy of the package in `checkout`."""
    environment = {**os.environ, "PYTHONPATH": str(checkout / "src")}
    return subprocess.run(
        [sys.executable, str(DEV_SCRIPT), *args], cwd=checkout, env=environment, capture_output=True, encoding="utf-8"
    )


def _page_files(root: Path) -> dict[str, Path]:
    """The generated catalogue pages in a checkout, by file name, in the order `render_effects_pages` gives them."""
    return {filename: root / "docs" / "effects" / filename for filename in gallery.render_effects_pages()}


def test_regenerate_writes_the_stub_and_the_pages_and_keeps_existing_pictures(docs):
    done = dev_command(docs, "regenerate")
    assert done.returncode == 0, done.stderr
    stub, pages = docs / "src" / "bullterrier_image" / "fx.pyi", _page_files(docs)
    assert done.stdout.split() == [str(stub), *map(str, pages.values())]  # into the scratch checkout, nowhere else
    assert stub.read_text(encoding="utf-8") == gallery.stub_text()
    for filename, text in gallery.render_effects_pages().items():
        assert pages[filename].read_text(encoding="utf-8") == text
    pictures = [gallery.effect_files(name, checkout=docs).picture for name in registry.describe()]
    assert {picture.read_bytes() for picture in pictures} == {b"existing"}


def test_the_gallery_command_writes_the_pages_too(docs):
    done = dev_command(docs, "gallery")
    assert done.returncode == 0, done.stderr
    pages = _page_files(docs)
    assert done.stdout.split() == [str(path) for path in pages.values()]
    for filename, text in gallery.render_effects_pages().items():
        assert pages[filename].read_text(encoding="utf-8") == text
    assert not (docs / "src" / "bullterrier_image" / "fx.pyi").exists()


def test_regenerating_again_changes_no_file_and_removes_the_page_of_a_category_that_lost_its_effects(docs):
    assert dev_command(docs, "regenerate").returncode == 0
    written = {path: path.read_bytes() for path in (docs / "docs" / "effects").iterdir()}
    stale = docs / "docs" / "effects" / "no_such_category.md"  # what a category that has no effects any more leaves
    stale.write_text("# Gone\n", encoding="utf-8")
    assert dev_command(docs, "regenerate").returncode == 0
    assert not stale.exists()
    assert {path: path.read_bytes() for path in (docs / "docs" / "effects").iterdir()} == written


def test_the_gallery_command_keeps_what_is_not_a_generated_page(docs):
    note = docs / "docs" / "effects" / "notes.txt"
    note.parent.mkdir()
    note.write_text("mine", encoding="utf-8")
    assert dev_command(docs, "gallery").returncode == 0
    assert note.read_text(encoding="utf-8") == "mine"


def test_the_gallery_command_renders_a_named_picture_again_even_when_it_exists(docs):
    """Naming an effect with --only means "render it": after its recipe or defaults changed, the old picture is not
    kept by surprise."""
    picture = gallery.effect_files("grain", checkout=docs).picture  # grain's recipe needs no cut-out: renders anywhere
    assert picture.read_bytes() == b"existing"
    done = dev_command(docs, "gallery", "--only", "grain")
    assert done.returncode == 0, done.stderr
    with Image.open(picture) as rendered:
        # rendered again
        assert rendered.format == "JPEG"
        assert max(rendered.size) == 480
    assert str(picture) in done.stdout.split()


def _files(root: Path) -> dict[str, bytes]:
    """Every file under `root` (relative path -> bytes), bytecode caches aside."""
    return {
        path.relative_to(root).as_posix(): path.read_bytes()
        for path in root.rglob("*")
        if path.is_file() and "__pycache__" not in path.parts
    }


def _catalogue_pages() -> set[str]:
    return {f"docs/effects/{filename}" for filename in gallery.render_effects_pages()}


GALLERY_RUNS = {  # command arguments -> the files it must add or change in the checkout (none else)
    "--only renders a missing picture and nothing else": (["--only", "grain"], {"docs/gallery/grain.jpg"}, True),
    "--only renders an existing picture again and nothing else": (
        ["--only", "grain"],
        {"docs/gallery/grain.jpg"},
        False,
    ),
    "--only names several effects": (
        ["--only", "grain", "levels"],
        {"docs/gallery/grain.jpg", "docs/gallery/levels.jpg"},
        False,
    ),
    "without --only the catalogue pages are written too": ([], _catalogue_pages(), False),
}


@pytest.mark.parametrize(("arguments", "changed", "picture_missing"), GALLERY_RUNS.values(), ids=GALLERY_RUNS.keys())
def test_the_gallery_command_writes_exactly_the_files_its_arguments_name(docs, arguments, changed, picture_missing):
    sentinels = {  # stand-ins for the generated and the hand-written files a picture run must leave alone
        "docs/effects/README.md": "old index",
        "docs/effects/light.md": "old page",
        "src/bullterrier_image/fx.pyi": "old stub",
        "README.md": "the project readme",
    }
    for relative, text in sentinels.items():
        (docs / relative).parent.mkdir(parents=True, exist_ok=True)
        (docs / relative).write_text(text, encoding="utf-8")
    if picture_missing:
        gallery.effect_files("grain", checkout=docs).picture.unlink()
    before = _files(docs)

    done = dev_command(docs, "gallery", *arguments)

    assert done.returncode == 0, done.stderr
    after = _files(docs)
    assert {path for path in before.keys() | after.keys() if before.get(path) != after.get(path)} == changed
    assert set(done.stdout.split()) == {str(docs / path) for path in changed}  # it reports what it wrote


def test_the_gallery_command_reports_an_unknown_effect_and_writes_nothing(docs):
    done = dev_command(docs, "gallery", "--only", "grain", "nope")
    assert done.returncode == 1
    assert "'nope'" in done.stderr
    assert not (docs / "docs" / "effects").exists()
