"""The generated effect catalogue: docs/effects.md, the gallery pictures, `photo-fx gallery` and `photo-fx regenerate`.

These tests build their own effects (throwaway or in a fresh registry) and their own example recipes, so they do not
depend on which real effects exist or how they are documented.
"""

import io
import re

import numpy as np
import pytest
from PIL import Image

from photo_fx import effects, fx, gallery
from photo_fx.cli import main
from photo_fx.effects._registry import effect


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
    assert "rgb" not in row and "matte" not in row  # the recipe supplies the image and the cut-out itself
    assert "[row_probe.toml](../examples/effects/row_probe.toml)" in row
    assert _unescaped_pipes(row) == 6  # five columns


def test_effects_page_escapes_table_breaking_characters(throwaway_effects):
    @effect(category="utilities")
    def pipe_probe(rgb, chars: str = "a|b", amount: float = 0.5):
        """Draws a | b
        across two lines. More."""
        return rgb

    row = _row(gallery.render_effects_page(), "pipe_probe")
    assert "`chars='a\\|b'`" in row and "Draws a \\| b across two lines." in row
    assert _unescaped_pipes(row) == 6  # still five columns


# --- the gallery pictures ----------------------------------------------------------------------------------------


@pytest.fixture
def probe(tmp_path, monkeypatch, throwaway_effects):
    """A throwaway effect with an example recipe of its own in a scratch examples folder. It needs no cut-out, so it
    renders anywhere. Its recipe renders at 300 px, to show the gallery picture is 480 px whatever the recipe says."""

    @effect(category="utilities")
    def gallery_probe(rgb, amount: float = 0.5):
        """Darkens the picture."""
        return (rgb * amount).astype(np.float32)

    examples = tmp_path / "examples"
    examples.mkdir()
    (examples / "gallery_probe.toml").write_text(
        'name = "gallery_probe"\nsize = 300\n[[step]]\neffect = "gallery_probe"\n'
    )
    monkeypatch.setattr(gallery, "EXAMPLES", examples)
    return "gallery_probe"


def _quantization(source) -> dict:
    with Image.open(source) as picture:
        return picture.quantization


def _jpeg_tables(quality: int) -> dict:
    """The quantization tables Pillow writes for `quality`: they identify the quality setting of a JPEG."""
    buffer = io.BytesIO()
    Image.new("RGB", (16, 16), (90, 120, 150)).save(buffer, "JPEG", quality=quality)
    return _quantization(buffer)


def test_gallery_renders_missing_pictures_only_unless_forced(tmp_path, probe):
    out = tmp_path / "out"
    assert gallery.render_gallery(out, only=[probe]) == [probe]
    first = (out / f"{probe}.jpg").read_bytes()
    (out / f"{probe}.jpg").write_bytes(b"placeholder")
    assert gallery.render_gallery(out, only=[probe]) == []  # it exists, so it is left alone
    assert (out / f"{probe}.jpg").read_bytes() == b"placeholder"
    assert gallery.render_gallery(out, only=[probe], force=True) == [probe]
    assert (out / f"{probe}.jpg").read_bytes() == first  # rendered again, and rendering is deterministic


def test_gallery_pictures_are_jpegs_480_px_on_the_longest_side_at_quality_82(tmp_path, probe):
    gallery.render_gallery(tmp_path, only=[probe])
    with Image.open(tmp_path / f"{probe}.jpg") as picture:
        assert picture.format == "JPEG" and max(picture.size) == 480  # from a 300 px recipe
    assert _jpeg_tables(82) != _jpeg_tables(90)  # the tables do tell qualities apart
    assert _quantization(tmp_path / f"{probe}.jpg") == _jpeg_tables(82)


def test_gallery_rejects_an_unknown_effect_before_rendering(tmp_path, probe):
    with pytest.raises(ValueError, match=r"unknown effect 'nope'.*available: .*gallery_probe"):
        gallery.render_gallery(tmp_path / "out", only=[probe, "nope"])
    assert not (tmp_path / "out").exists()


def test_gallery_needs_an_example_recipe_for_each_effect_it_renders(tmp_path, probe):
    @effect(category="utilities")
    def recipe_less(rgb, amount: float = 0.5):
        """Has no example recipe."""
        return rgb

    with pytest.raises(FileNotFoundError, match=r"recipe_less\.toml"):
        gallery.render_gallery(tmp_path / "out", only=[probe, "recipe_less"])
    assert not (tmp_path / "out").exists()


# --- writing everything: `photo-fx gallery` and `photo-fx regenerate` --------------------------------------------


@pytest.fixture
def docs(tmp_path, monkeypatch):
    """Point the generated files at a scratch folder, with every gallery picture already there (so none renders)."""
    monkeypatch.setattr(gallery, "GALLERY", tmp_path / "gallery")
    monkeypatch.setattr(gallery, "EFFECTS_PAGE", tmp_path / "effects.md")
    monkeypatch.setattr(gallery, "STUB", tmp_path / "fx.pyi")
    (tmp_path / "gallery").mkdir()
    for name in effects.describe():
        (tmp_path / "gallery" / f"{name}.jpg").write_bytes(b"existing")
    return tmp_path


def test_regenerate_writes_the_stub_and_the_page_and_keeps_existing_pictures(docs, capsys):
    assert main(["regenerate"]) == 0
    assert (docs / "fx.pyi").read_text() == fx.stub_text()
    assert (docs / "effects.md").read_text() == gallery.render_effects_page()
    assert {p.read_bytes() for p in (docs / "gallery").iterdir()} == {b"existing"}
    assert capsys.readouterr().out.split() == [str(docs / "fx.pyi"), str(docs / "effects.md")]


def test_the_gallery_command_writes_the_page_too(docs, capsys):
    assert main(["gallery"]) == 0
    assert (docs / "effects.md").read_text() == gallery.render_effects_page()
    assert not (docs / "fx.pyi").exists()


def test_the_gallery_command_renders_an_existing_picture_again_only_with_force(probe, docs, capsys):
    picture = docs / "gallery" / f"{probe}.jpg"
    assert picture.read_bytes() == b"existing"
    assert main(["gallery", "--only", probe]) == 0
    assert picture.read_bytes() == b"existing"  # kept
    assert main(["gallery", "--only", probe, "--force"]) == 0
    with Image.open(picture) as rendered:
        assert rendered.format == "JPEG"  # rendered again
    assert str(picture) in capsys.readouterr().out.split()


def test_the_gallery_command_reports_an_unknown_effect_and_writes_nothing(probe, docs, capsys):
    assert main(["gallery", "--only", probe, "nope"]) == 1
    assert "unknown effect 'nope'" in capsys.readouterr().err
    assert not (docs / "effects.md").exists()
