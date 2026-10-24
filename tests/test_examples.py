import importlib.util
import re
from pathlib import Path

import pytest

from bullterrier_image import Recipe, gallery, load_project, load_recipe, render
from bullterrier_image.core import registry

EXAMPLES = Path(__file__).parent.parent / "examples"


@pytest.mark.parametrize("name", sorted(registry.describe()))
def test_every_effect_has_a_short_example_recipe_that_shows_it(name):
    path = gallery.effect_files(name).example
    assert path.is_file(), f"add {path.relative_to(EXAMPLES.parent)}: a short recipe showing {name}"
    recipe = load_recipe(path)
    assert recipe.name == name
    assert recipe.size == 480
    assert 1 <= len(recipe.steps) <= 3
    assert name in [step.effect for step in recipe.steps]


def test_every_example_recipe_belongs_to_an_effect():
    orphans = [p.name for p in (EXAMPLES / "effects").glob("*.toml") if p.stem not in registry.describe()]
    assert not orphans, f"example recipes for effects that do not exist: {orphans}"


def test_the_demo_project_loads_and_every_recipe_in_it_is_valid():
    project = load_project(EXAMPLES / "bt-image.toml")
    assert [p.name for p in project.photos] == ["bullterrier.jpeg"]
    files = [r for r in project.recipes if isinstance(r, Path)]
    assert len(files) == len(project.recipes)  # a project file names recipe files, never Recipe objects
    recipes = [load_recipe(f) for f in files]
    assert len({r.name for r in recipes}) == len(recipes)
    assert any(r.needs_matte for r in recipes)


def _is_a_combination(recipe: Recipe) -> bool:
    """A demo recipe combines effects: three different ones at least."""
    return len({step.effect for step in recipe.steps}) >= 3


@pytest.mark.parametrize("path", sorted((EXAMPLES / "recipes").glob("*.toml")), ids=lambda path: path.stem)
def test_every_demo_recipe_combines_effects_and_renders(path, photo, matte):
    recipe = load_recipe(path)
    assert _is_a_combination(recipe), f"{path.name} uses fewer than three different effects"
    assert max(render(photo, recipe, matte).shape[:2]) == recipe.size  # rendered at the recipe's size


def test_the_avatar_recipe_crops_a_square_around_the_subject_and_cuts_a_circle(photo, matte):
    recipe = load_recipe(EXAMPLES / "transparent" / "avatar.toml")
    out = render(photo, recipe, matte)
    assert out.shape == (512, 512, 4)  # the recipe's size, as a square
    assert (out[0, 0, 3], out[256, 256, 3]) == (0, 1)  # clear in the corner, opaque in the middle


def test_a_recipe_of_two_effects_is_not_a_combination(tmp_path):
    path = tmp_path / "two.toml"
    path.write_text(
        'name = "two"\n[[step]]\neffect = "grain"\n[[step]]\neffect = "grain"\n[[step]]\neffect = "glow"\n',
        encoding="utf-8",
    )
    assert not _is_a_combination(load_recipe(path))


def test_the_text_effects_in_the_demo_recipes_use_the_same_character_size_as_the_text_they_move():
    """disperse/evaporate move whole characters, so their cell must equal the ascii step's cell."""
    checked = []
    examples = [gallery.effect_files(name).example for name in registry.describe()]
    for path in [*(EXAMPLES / "recipes").glob("*.toml"), *examples]:
        steps = {s.effect: s.params for s in load_recipe(path).steps}
        for mover in ("disperse", "evaporate"):
            if mover in steps and "ascii" in steps:
                assert steps[mover]["cell"] == steps["ascii"]["cell"], path.name
                checked.append(path.name)
    assert checked, "no demo recipe moves ascii text, so nothing was checked"


def test_the_python_example_runs(tmp_path, capsys, echo_model, monkeypatch):
    monkeypatch.setenv("BT_IMAGE_MODEL", str(echo_model(0)))  # a stand-in subject finder: no download, same on every OS
    spec = importlib.util.spec_from_file_location("python_api_example", EXAMPLES / "python_api.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.main(tmp_path)
    assert (tmp_path / "steps" / "_sheet.jpg").is_file()
    assert (tmp_path / "pipeline" / "bullterrier" / "toxic-neon.jpg").is_file()
    assert "toxic-neon: done" in capsys.readouterr().out


def test_no_video_or_gif_is_checked_in(tracked_files):
    assert len(tracked_files) > 100  # the listing really is the repository
    assert [f for f in tracked_files if f.lower().endswith((".mp4", ".mov", ".webm", ".gif"))] == []


def test_the_readme_links_to_the_effects_page_and_every_picture_it_shows_exists():
    readme = (EXAMPLES.parent / "README.md").read_text(encoding="utf-8")
    assert re.search(r"\]\(docs/effects/README\.md\)", readme), "the README must link to the effect catalogue"
    assert (EXAMPLES.parent / "docs" / "effects" / "README.md").is_file()
    local = [p for p in re.findall(r'src="([^"]+)"', readme) if not p.startswith("http")]
    assert len(local) >= 8  # the banner, the gallery and the step sheet
    for path in local:
        assert (EXAMPLES.parent / path).is_file(), f"README shows {path}, which does not exist"
    links = re.findall(r'href="((?:examples|docs|src)/[^"]+)"', readme)
    assert links, "the README links to no local file, so the link check checks nothing"
    for path in links:
        assert (EXAMPLES.parent / path).exists(), f"README links to {path}, which does not exist"
