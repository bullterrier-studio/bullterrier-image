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

import numpy as np
import pytest
from PIL import Image

import photo_fx
from photo_fx import effects, fx, gallery, imageio
from photo_fx.core.registry import effect


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


def test_an_effects_row_has_its_picture_summary_parameters_and_recipe(fresh_registry):
    @effect(category="light")
    def row_probe(rgb, radius: float = 24.0, chars: str = " .:", fg: str | None = None, matte=None):
        """Softly probes the bright parts. Then says more."""
        return rgb

    row = _row(gallery.render_effects_page(), "row_probe")
    assert 'src="gallery/row_probe.jpg"' in row
    assert "| Softly probes the bright parts. |" in row  # the first sentence only
    assert "`radius=24.0` `chars=' .:'` `fg=None`" in row  # what a recipe sets, in order, with defaults
    # the recipe supplies the image and the cut-out itself
    assert "rgb" not in row
    assert "matte" not in row
    assert "[row_probe.toml](../examples/effects/row_probe.toml)" in row
    assert _unescaped_pipes(row) == 6  # five columns


def test_effects_page_escapes_table_breaking_characters(throwaway_effects):
    @effect(category="utilities")
    def pipe_probe(rgb, chars: str = "a|b", amount: float = 0.5):
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
    """A scratch source checkout to generate into: a copy of the real example recipes, a demo photo, and the empty
    folders the generated files go to. Nothing in the real checkout is written."""
    root = tmp_path / "checkout"
    shutil.copytree(REAL_EXAMPLES, root / "examples" / "effects")
    (root / "examples" / "photos").mkdir()
    imageio.save(photo, root / "examples" / "photos" / "bullterrier.jpeg")
    (root / "docs" / "gallery").mkdir(parents=True)
    (root / "src" / "photo_fx").mkdir(parents=True)
    return root


@pytest.fixture
def probe(checkout, throwaway_effects):
    """A throwaway effect with an example recipe of its own in the scratch checkout. It needs no cut-out, so it
    renders anywhere. Its recipe renders at 300 px, to show the gallery picture is 480 px whatever the recipe says."""

    @effect(category="utilities")
    def gallery_probe(rgb, amount: float = 0.5):
        """Darkens the picture."""
        return (rgb * amount).astype(np.float32)

    (checkout / "examples" / "effects" / "gallery_probe.toml").write_text(
        'name = "gallery_probe"\nsize = 300\n[[step]]\neffect = "gallery_probe"\n'
    )
    return "gallery_probe"


def _quantization(source) -> dict:
    with Image.open(source) as picture:
        return picture.quantization


def _jpeg_tables(quality: int) -> dict:
    """The quantization tables Pillow writes for `quality`: they identify the quality setting of a JPEG."""
    buffer = io.BytesIO()
    Image.new("RGB", (16, 16), (90, 120, 150)).save(buffer, "JPEG", quality=quality)
    return _quantization(buffer)


def test_gallery_renders_missing_pictures_only_unless_forced(tmp_path, checkout, probe):
    out = tmp_path / "out"
    assert gallery.render_gallery(out, only=[probe], checkout=checkout) == [probe]
    first = (out / f"{probe}.jpg").read_bytes()
    (out / f"{probe}.jpg").write_bytes(b"placeholder")
    assert gallery.render_gallery(out, only=[probe], checkout=checkout) == []  # it exists, so it is left alone
    assert (out / f"{probe}.jpg").read_bytes() == b"placeholder"
    assert gallery.render_gallery(out, only=[probe], force=True, checkout=checkout) == [probe]
    assert (out / f"{probe}.jpg").read_bytes() == first  # rendered again, and rendering is deterministic


def test_gallery_pictures_are_jpegs_480_px_on_the_longest_side_at_quality_82(tmp_path, checkout, probe):
    gallery.render_gallery(tmp_path, only=[probe], checkout=checkout)
    with Image.open(tmp_path / f"{probe}.jpg") as picture:
        # from a 300 px recipe
        assert picture.format == "JPEG"
        assert max(picture.size) == 480
    assert _jpeg_tables(82) != _jpeg_tables(90)  # the tables do tell qualities apart
    assert _quantization(tmp_path / f"{probe}.jpg") == _jpeg_tables(82)


def test_gallery_pictures_show_the_checkouts_demo_photo_through_the_example_recipe(tmp_path, checkout, probe, photo):
    gallery.render_gallery(tmp_path, only=[probe], checkout=checkout)
    picture = imageio.load(tmp_path / f"{probe}.jpg")
    expected = imageio.resize(photo * 0.5, 480)  # the probe halves the brightness; the demo photo is `photo`
    # JPEG and resampling only
    assert picture.shape == expected.shape
    assert np.abs(picture - expected).mean() < 0.02


def test_gallery_rejects_an_unknown_effect_before_rendering(tmp_path, checkout, probe):
    with pytest.raises(ValueError) as raised:
        gallery.render_gallery(tmp_path / "out", only=[probe, "nope"], checkout=checkout)
    # the name, and the choices
    assert "'nope'" in str(raised.value)
    assert "gallery_probe" in str(raised.value)
    assert not (tmp_path / "out").exists()


def test_gallery_needs_an_example_recipe_for_each_effect_it_renders(tmp_path, checkout, probe):
    @effect(category="utilities")
    def recipe_less(rgb, amount: float = 0.5):
        """Has no example recipe."""
        return rgb

    with pytest.raises(FileNotFoundError, match=r"recipe_less\.toml"):
        gallery.render_gallery(tmp_path / "out", only=[probe, "recipe_less"], checkout=checkout)
    assert not (tmp_path / "out").exists()


def test_regenerate_checks_the_checkout_before_writing_anything(checkout):
    (checkout / "examples" / "effects" / "grain.toml").unlink()  # one effect without its example recipe
    with pytest.raises(FileNotFoundError, match=r"grain\.toml"):
        gallery.regenerate(checkout=checkout)
    assert not (checkout / "src" / "photo_fx" / "fx.pyi").exists()
    assert not (checkout / "docs" / "effects.md").exists()


def test_gallery_outside_a_source_checkout_says_the_examples_are_missing(tmp_path):
    with pytest.raises(FileNotFoundError, match="examples"):
        gallery.render_gallery(tmp_path / "out", only=["grain"], checkout=tmp_path / "not-a-checkout")
    assert not (tmp_path / "out").exists()


# --- writing everything: `photo-fx gallery` and `photo-fx regenerate` --------------------------------------------


# The commands always work on the checkout photo-fx runs from. To run them without touching the real one, each test
# runs `photo-fx` in a fresh interpreter from a copy of the package placed in the scratch checkout's src/: imported
# from there, photo-fx takes the scratch checkout for its own.


@pytest.fixture
def docs(checkout):
    """The scratch checkout holding a copy of this photo-fx package (without its fx.pyi, so writing one shows) and
    every gallery picture already there (so none renders)."""
    package = checkout / "src" / "photo_fx"
    shutil.rmtree(package)
    shutil.copytree(Path(photo_fx.__file__).parent, package, ignore=shutil.ignore_patterns("__pycache__"))
    (package / "fx.pyi").unlink()
    for name in effects.describe():
        (checkout / "docs" / "gallery" / f"{name}.jpg").write_bytes(b"existing")
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
    assert {p.read_bytes() for p in (docs / "docs" / "gallery").iterdir()} == {b"existing"}


def test_the_gallery_command_writes_the_page_too(docs):
    done = photo_fx_command(docs, "gallery")
    assert done.returncode == 0, done.stderr
    assert done.stdout.split() == [str(docs / "docs" / "effects.md")]
    assert (docs / "docs" / "effects.md").read_text() == gallery.render_effects_page()
    assert not (docs / "src" / "photo_fx" / "fx.pyi").exists()


def test_the_gallery_command_renders_an_existing_picture_again_only_with_force(docs):
    picture = docs / "docs" / "gallery" / "grain.jpg"  # grain's example recipe needs no cut-out, so it renders anywhere
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
