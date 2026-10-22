"""The generated effect catalogue: docs/effects.md, the gallery pictures, `photo-fx gallery` and `photo-fx regenerate`.

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

import photo_fx
from photo_fx import effects, fx, gallery, imageio
from photo_fx.core.params import Color, EffectParams, param
from photo_fx.core.registry import effect


class AmountParams(EffectParams):
    """The one parameter of a probe effect."""

    amount: float = param(0.5, unit="fraction", min=0, max=1, doc="How much of the picture a probe keeps.")


def _sections(page: str) -> dict[str, str]:
    """The page cut at its `## ` headings: heading -> the text under it."""
    parts = re.split(r"^## (.+)$", page, flags=re.M)
    return dict(zip(parts[1::2], parts[2::2], strict=True))


def _row(page: str, name: str) -> str:
    (row,) = [line for line in page.splitlines() if line.startswith("|") and f"`{name}`" in line]
    return row


def _unescaped_pipes(row: str) -> int:
    return len(re.findall(r"(?<!\\)\|", row))


# --- the effects page --------------------------------------------------------------------------------------------


def test_the_effects_page_has_one_section_per_used_category_in_catalogue_order(fresh_registry, register_probe):
    for name, category in [("drift", "animation"), ("tint", "colour"), ("lamp", "light")]:
        register_probe(name, category)  # registered in another order than the catalogue's
    assert list(_sections(gallery.render_effects_page())) == ["Colour", "Light", "Animation"]


def test_a_category_appears_on_the_page_once_it_has_an_effect(fresh_registry, register_probe):
    register_probe("lamp", "light")
    assert "Distortion" not in _sections(gallery.render_effects_page())  # no empty table
    register_probe("swirl", "distortion")
    assert "`swirl`" in _sections(gallery.render_effects_page())["Distortion"]


def test_every_real_effect_is_listed_once_in_its_own_category_section():
    sections = _sections(gallery.render_effects_page())
    for name in effects.describe():
        homes = [heading for heading, text in sections.items() if f"`{name}`" in text]
        assert homes == [effects.category(name).capitalize()], name


class RowParams(EffectParams):
    """The row probe's settings."""

    radius: float = param(24.0, unit="px", min=0, max=500, doc="Unused.")
    chars: str = param(" .:", doc="Unused.")
    ink: Color | None = param(None, doc="Unused.")


def test_an_effects_row_has_its_picture_summary_parameters_and_recipe(fresh_registry):
    @effect(category="light", params=RowParams)
    def row_probe(rgb, params, *, matte=None):
        """Softly probes the bright parts. Then says more."""
        return rgb

    row = _row(gallery.render_effects_page(), "row_probe")
    assert 'src="gallery/row_probe.jpg"' in row
    assert "| Softly probes the bright parts. |" in row  # the first sentence only
    assert "`radius=24.0` `chars=' .:'` `ink=None`" in row  # what a recipe sets, in order, with defaults
    # the recipe supplies the image and the cut-out itself
    assert "rgb" not in row
    assert "matte" not in row
    assert "[row_probe.toml](../examples/effects/row_probe.toml)" in row
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

    page = gallery.render_effects_page()
    section = _sections(page)["Light"]
    assert "`radius=24.0` `mode='screen'`" in _row(page, "halo_probe")  # the short form in the effect's row
    table = section[section.index("### `halo_probe`") :]
    lines = [line for line in table.splitlines() if line.startswith("|")]
    assert lines[0] == "| Parameter | Type | Unit | Range | Default | Description |"
    assert lines[2] == "| `radius` | number | px | 0 to 500 | `24.0` | Width of the halo \\| in px. Larger = softer. |"
    assert (
        lines[3]
        == "| `mode` | one of 'screen', 'add' |  |  | `'screen'` | How the halo meets the picture: 'screen' or 'add'. |"
    )


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

    row = _row(gallery.render_effects_page(), "pipe_probe")
    assert "`chars='a\\|b'`" in row
    assert "Draws a \\| b across two lines." in row
    assert _unescaped_pipes(row) == 6  # still five columns


# --- the gallery pictures ----------------------------------------------------------------------------------------

REAL_EXAMPLES = Path(__file__).parent.parent / "examples" / "effects"


@pytest.fixture
def checkout(tmp_path, photo):
    """A scratch source checkout to generate into: a copy of this photo-fx package (without fx.pyi, so writing one
    shows), a copy of the example recipes, a demo photo, and the empty folder the pictures go to. Nothing in the real
    checkout is written."""
    root = tmp_path / "checkout"
    package = root / "src" / "photo_fx"
    shutil.copytree(Path(photo_fx.__file__).parent, package, ignore=shutil.ignore_patterns("__pycache__"))
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
        'name = "gallery_probe"\nsize = 300\n[[step]]\neffect = "gallery_probe"\n'
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
    assert files.package == checkout / "src" / "photo_fx" / "effects" / "lamp"
    assert files.tests == checkout / "tests" / "effects" / "lamp" / "test_lamp.py"
    assert files.example == checkout / "examples" / "effects" / "lamp.toml"
    assert files.picture == checkout / "docs" / "gallery" / "lamp.jpg"


def test_every_row_of_the_real_page_links_to_its_effects_own_picture_and_recipe():
    page, docs = gallery.render_effects_page(), gallery.CHECKOUT / "docs"
    for name in effects.describe():
        files, row = gallery.effect_files(name), _row(page, name)
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
    expected = imageio.resize(photo * 0.5, 480)  # the probe halves the brightness; the demo photo is `photo`
    # JPEG and resampling only
    assert picture.shape == expected.shape
    assert np.abs(picture - expected).mean() < 0.02


def test_gallery_rejects_an_unknown_effect_before_rendering(checkout, probe):
    with pytest.raises(ValueError, match="nope") as raised:
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

    with pytest.raises(FileNotFoundError, match=r"recipe_less\.toml"):
        gallery.render_gallery(only=[probe, "recipe_less"], checkout=checkout)
    assert not gallery.effect_files(probe, checkout=checkout).picture.exists()


def test_regenerate_checks_the_checkout_before_writing_anything(checkout):
    gallery.effect_files("grain", checkout=checkout).example.unlink()  # one effect without its example recipe
    with pytest.raises(FileNotFoundError, match="grain"):
        gallery.regenerate(checkout=checkout)
    assert not (checkout / "src" / "photo_fx" / "fx.pyi").exists()
    assert not (checkout / "docs" / "effects.md").exists()


def test_gallery_outside_a_source_checkout_says_it_needs_one(tmp_path):
    with pytest.raises(FileNotFoundError, match="source checkout"):
        gallery.render_gallery(only=["grain"], checkout=tmp_path / "not-a-checkout")
    assert not (tmp_path / "not-a-checkout").exists()


# --- writing everything: `photo-fx gallery` and `photo-fx regenerate` --------------------------------------------


# The commands always work on the checkout photo-fx runs from. To run them without touching the real one, each test
# runs `photo-fx` in a fresh interpreter from a copy of the package placed in the scratch checkout's src/: imported
# from there, photo-fx takes the scratch checkout for its own.


@pytest.fixture
def docs(checkout):
    """The scratch checkout with every effect's gallery picture already there (so none renders)."""
    for name in effects.describe():
        gallery.effect_files(name, checkout=checkout).picture.write_bytes(b"existing")
    return checkout


def photo_fx_command(checkout: Path, *args: str) -> subprocess.CompletedProcess:
    """`photo-fx ARGS`, run from the copy of the package in `checkout`."""
    environment = {**os.environ, "PYTHONPATH": str(checkout / "src")}
    return subprocess.run(
        [sys.executable, "-m", "photo_fx.cli", *args], cwd=checkout, env=environment, capture_output=True, text=True
    )


def test_regenerate_writes_the_stub_and_the_page_and_keeps_existing_pictures(docs):
    done = photo_fx_command(docs, "regenerate")
    assert done.returncode == 0, done.stderr
    stub, page = docs / "src" / "photo_fx" / "fx.pyi", docs / "docs" / "effects.md"
    assert done.stdout.split() == [str(stub), str(page)]  # written into the scratch checkout, nowhere else
    assert stub.read_text() == fx.stub_text()
    assert page.read_text() == gallery.render_effects_page()
    pictures = [gallery.effect_files(name, checkout=docs).picture for name in effects.describe()]
    assert {picture.read_bytes() for picture in pictures} == {b"existing"}


def test_the_gallery_command_writes_the_page_too(docs):
    done = photo_fx_command(docs, "gallery")
    assert done.returncode == 0, done.stderr
    assert done.stdout.split() == [str(docs / "docs" / "effects.md")]
    assert (docs / "docs" / "effects.md").read_text() == gallery.render_effects_page()
    assert not (docs / "src" / "photo_fx" / "fx.pyi").exists()


def test_the_gallery_command_renders_an_existing_picture_again_only_with_force(docs):
    picture = gallery.effect_files("grain", checkout=docs).picture  # grain's recipe needs no cut-out: renders anywhere
    assert photo_fx_command(docs, "gallery", "--only", "grain").returncode == 0
    assert picture.read_bytes() == b"existing"  # kept
    done = photo_fx_command(docs, "gallery", "--only", "grain", "--force")
    assert done.returncode == 0, done.stderr
    with Image.open(picture) as rendered:
        # rendered again
        assert rendered.format == "JPEG"
        assert max(rendered.size) == 480
    assert str(picture) in done.stdout.split()


def test_the_gallery_command_reports_an_unknown_effect_and_writes_nothing(docs):
    done = photo_fx_command(docs, "gallery", "--only", "grain", "nope")
    assert done.returncode == 1
    assert "'nope'" in done.stderr
    assert not (docs / "docs" / "effects.md").exists()
