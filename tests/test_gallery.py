import re
import sys

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


def test_the_effects_page_has_one_section_per_used_category_in_catalogue_order():
    headings = list(_sections(gallery.render_effects_page()))
    used = {effects.category(name).capitalize() for name in effects.describe()}
    assert set(headings) == used  # a category without effects gets no empty table
    assert headings == [c.capitalize() for c in effects.CATEGORIES if c.capitalize() in used]
    assert headings.index("Colour") < headings.index("Light") < headings.index("Animation")


def test_a_category_appears_on_the_page_once_it_has_an_effect(throwaway_effects):
    empty = next((c for c, names in effects.categories().items() if not names), None)
    if empty is None:
        pytest.skip("every category has an effect")
    assert empty.capitalize() not in _sections(gallery.render_effects_page())

    @effect(category=empty)
    def newcomer(rgb, amount: float = 0.5):
        """The first effect of its category."""
        return rgb

    assert "`newcomer`" in _sections(gallery.render_effects_page())[empty.capitalize()]


def test_every_effect_is_listed_once_in_its_own_category_section():
    sections = _sections(gallery.render_effects_page())
    for name in effects.describe():
        homes = [heading for heading, text in sections.items() if f"`{name}`" in text]
        assert homes == [effects.category(name).capitalize()], name


def test_an_effects_row_has_its_picture_summary_parameters_and_recipe():
    row = _row(gallery.render_effects_page(), "glow")
    assert 'src="gallery/glow.jpg"' in row
    assert "Soft bloom around the bright parts." in row
    assert "`radius=24.0`" in row and "`amount=0.6`" in row and "`threshold=0.55`" in row
    assert "](../examples/effects/glow.toml)" in row
    assert _unescaped_pipes(row) == 6  # five columns


def test_the_parameters_shown_are_the_ones_a_recipe_sets():
    page = gallery.render_effects_page()
    assert "`cell=10`" in _row(page, "ascii") and "`chars=' .:-=+*#%@'`" in _row(page, "ascii")
    assert "rgb=" not in page  # the image is not a parameter
    assert "matte=" not in _row(page, "evaporate")  # the recipe supplies the cut-out itself


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


def test_gallery_rejects_an_unknown_effect_before_rendering(tmp_path):
    with pytest.raises(ValueError, match=r"unknown effect 'nope'.*available: .*glow"):
        gallery.render_gallery(tmp_path / "out", only=["glow", "nope"])
    assert not (tmp_path / "out").exists()


def test_gallery_needs_an_example_recipe_for_each_effect_it_renders(tmp_path, throwaway_effects):
    @effect(category="utilities")
    def recipe_less(rgb, amount: float = 0.5):
        """Has no example recipe."""
        return rgb

    with pytest.raises(FileNotFoundError, match=r"examples/effects/recipe_less\.toml"):
        gallery.render_gallery(tmp_path / "out", only=["glow", "recipe_less"])
    assert not (tmp_path / "out").exists()


@pytest.mark.skipif(
    sys.platform != "darwin", reason="the example recipes cut out the subject, which needs Apple Vision"
)
def test_gallery_renders_missing_pictures_only_unless_forced(tmp_path):
    assert gallery.render_gallery(tmp_path, only=["glow"]) == ["glow"]
    with Image.open(tmp_path / "glow.jpg") as picture:
        assert picture.format == "JPEG" and max(picture.size) == 480
    first = (tmp_path / "glow.jpg").read_bytes()
    (tmp_path / "glow.jpg").write_bytes(b"placeholder")
    assert gallery.render_gallery(tmp_path, only=["glow"]) == []  # it exists, so it is left alone
    assert (tmp_path / "glow.jpg").read_bytes() == b"placeholder"
    assert gallery.render_gallery(tmp_path, only=["glow"], force=True) == ["glow"]
    assert (tmp_path / "glow.jpg").read_bytes() == first  # rendering is deterministic


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


def test_the_gallery_command_reports_an_unknown_effect_and_writes_nothing(docs, capsys):
    assert main(["gallery", "--only", "glow", "nope"]) == 1
    assert "unknown effect 'nope'" in capsys.readouterr().err
    assert not (docs / "effects.md").exists()
