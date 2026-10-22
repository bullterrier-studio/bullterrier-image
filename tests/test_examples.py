import importlib.util
import sys
from pathlib import Path

import pytest

from photo_fx import effects, load_project, load_recipe

EXAMPLES = Path(__file__).parent.parent / "examples"


@pytest.mark.parametrize("name", sorted(effects.describe()))
def test_every_effect_has_a_short_example_recipe_that_shows_it(name):
    path = EXAMPLES / "effects" / f"{name}.toml"
    assert path.is_file(), f"add {path.relative_to(EXAMPLES.parent)}: a short recipe showing {name}"
    recipe = load_recipe(path)
    assert recipe.name == name and recipe.size == 480
    assert 1 <= len(recipe.steps) <= 3
    assert name in [step.effect for step in recipe.steps]


def test_every_example_recipe_belongs_to_an_effect():
    orphans = [p.name for p in (EXAMPLES / "effects").glob("*.toml") if p.stem not in effects.describe()]
    assert not orphans, f"example recipes for effects that do not exist: {orphans}"


def test_the_demo_project_loads_and_every_recipe_in_it_is_valid():
    project = load_project(EXAMPLES / "photo-fx.toml")
    assert [p.name for p in project.photos] == ["bullterrier.jpeg"]
    recipes = [load_recipe(r) for r in project.recipes]
    assert len(recipes) <= 6  # a demo, not a catalogue
    assert len({r.name for r in recipes}) == len(recipes)
    assert any(r.animation for r in recipes) and any(r.needs_matte for r in recipes)


def test_the_text_effects_in_the_demo_recipes_use_the_same_character_size_as_the_text_they_move():
    """disperse/evaporate move whole characters, so their cell must equal the ascii step's cell."""
    for path in [*(EXAMPLES / "recipes").glob("*.toml"), *(EXAMPLES / "effects").glob("*.toml")]:
        steps = {s.effect: s.params for s in load_recipe(path).steps}
        for mover in ("disperse", "evaporate"):
            if mover in steps and "ascii" in steps:
                assert steps[mover]["cell"] == steps["ascii"]["cell"], path.name


def test_the_demo_pictures_the_readme_shows_exist():
    for name in ("neon-pulse", "glitch", "ascii-colour", "halftone-print", "ember-glitch"):
        assert (EXAMPLES / "output" / "bullterrier" / f"{name}.jpg").is_file(), name
    assert (EXAMPLES / "output" / "bullterrier" / "evaporate.frames.png").is_file()  # a picture, not a video
    assert (EXAMPLES / "output" / "steps" / "ember-glitch" / "_sheet.jpg").is_file()


@pytest.mark.skipif(sys.platform != "darwin", reason="finding the subject needs Apple Vision")
def test_the_python_example_runs(tmp_path, capsys):
    spec = importlib.util.spec_from_file_location("python_api_example", EXAMPLES / "python_api.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.main(tmp_path)
    assert (tmp_path / "steps" / "_sheet.jpg").is_file()
    assert (tmp_path / "pipeline" / "bullterrier" / "toxic-neon.jpg").is_file()
    assert "toxic-neon: done" in capsys.readouterr().out


def test_no_video_or_gif_is_checked_in():
    import subprocess

    tracked = subprocess.run(["git", "ls-files"], cwd=EXAMPLES.parent, capture_output=True, text=True).stdout.split()
    assert not [f for f in tracked if f.lower().endswith((".mp4", ".mov", ".webm", ".gif"))]


def test_the_readme_links_to_the_effects_page_and_every_picture_it_shows_exists():
    import re

    readme = (EXAMPLES.parent / "README.md").read_text()
    assert re.search(r"\]\(docs/effects\.md\)", readme), "the README must link to docs/effects.md, the effect catalogue"
    assert (EXAMPLES.parent / "docs" / "effects.md").is_file()
    local = [p for p in re.findall(r'src="([^"]+)"', readme) if not p.startswith("http")]
    assert len(local) >= 8  # the banner, the gallery, the step sheet and the filmstrip
    for path in local:
        assert (EXAMPLES.parent / path).is_file(), f"README shows {path}, which does not exist"
    links = re.findall(r'href="((?:examples|docs|src)/[^"]+)"', readme)
    for path in links:
        assert (EXAMPLES.parent / path).exists(), f"README links to {path}, which does not exist"
