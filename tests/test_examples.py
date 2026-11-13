import importlib.util
import re
from pathlib import Path

import numpy as np
import pytest

from bullterrier_image import Finder, Recipe, Step, gallery, imageio, load_project, load_recipe, render
from bullterrier_image.core import registry
from bullterrier_image.core.bridge import maps_needed
from bullterrier_image.core.steps import runs

EXAMPLES = Path(__file__).parent.parent / "examples"


@pytest.mark.parametrize("name", sorted(registry.describe()))
def test_every_effect_has_a_short_example_recipe_that_shows_it(name):
    path = gallery.effect_files(name).example
    assert path.is_file(), f"add {path.relative_to(EXAMPLES.parent)}: a short recipe showing {name}"
    recipe = load_recipe(path)
    assert recipe.name == name
    assert recipe.size == 480
    assert 1 <= len(recipe.steps) <= 3
    assert name in [run.effect for step in recipe.steps for run in runs(step)]


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
    assert any("subject" in maps_needed(r) for r in recipes)


def _is_a_combination(recipe: Recipe) -> bool:
    """A demo recipe combines effects: three different ones at least."""
    return len({run.effect for step in recipe.steps for run in runs(step)}) >= 3


def a_floor_rising_away(photo: np.ndarray) -> np.ndarray:
    """A stand-in depth map (the model is an external boundary): the bottom row nearest (1), the top farthest (0)."""
    height, width = photo.shape[:2]
    return np.repeat(np.linspace(0, 1, height, dtype=np.float32)[:, None], width, axis=1)


@pytest.mark.parametrize("path", sorted((EXAMPLES / "recipes").glob("*.toml")), ids=lambda path: path.stem)
def test_every_demo_recipe_combines_effects_and_renders(path, photo, subject):
    recipe = load_recipe(path)
    assert _is_a_combination(recipe), f"{path.name} uses fewer than three different effects"
    out = render(photo, recipe, maps={"subject": subject, "depth": a_floor_rising_away(photo)})
    assert max(out.shape[:2]) == recipe.size  # rendered at the recipe's size


@pytest.mark.parametrize("path", sorted((EXAMPLES / "fields").glob("*.toml")), ids=lambda path: path.stem)
def test_every_fields_example_needs_no_map_and_renders_to_something_other_than_the_photo(path, photo):
    recipe = load_recipe(path)
    assert recipe.name == path.stem
    assert maps_needed(recipe) == frozenset()  # made from a gradient or the picture's own light: no model, no cut-out
    out = render(photo, recipe)
    assert max(out.shape[:2]) == recipe.size
    assert not np.array_equal(out, render(photo, Recipe([Step("grade")], name="plain", size=recipe.size)))


def a_disc_in_every_view(view: np.ndarray) -> np.ndarray:
    """A stand-in subject finder (the model is an external boundary): the disc in the middle of whatever it is shown."""
    height, width = view.shape[:2]
    y, x = np.mgrid[0:height, 0:width]
    return (np.hypot(x - width / 2, y - height / 2) <= 0.3 * max(height, width)).astype(np.float32)


@pytest.mark.parametrize("path", sorted((EXAMPLES / "regions").glob("*.toml")), ids=lambda path: path.stem)
def test_every_regions_example_declares_regions_and_renders_to_something_other_than_the_photo(path, photo, subject):
    recipe = load_recipe(path)
    assert recipe.name == path.stem
    assert recipe.regions
    out = render(
        photo, recipe, maps={"subject": subject}, finders={"subject": Finder("stand-in", a_disc_in_every_view)}
    )
    assert max(out.shape[:2]) == recipe.size
    assert not np.array_equal(out, render(photo, Recipe([Step("grade")], name="plain", size=recipe.size)))


def test_a_recipe_of_two_effects_is_not_a_combination(tmp_path):
    path = tmp_path / "two.toml"
    path.write_text(
        'name = "two"\n[[step]]\neffect = "grain"\n[[step]]\neffect = "grain"\n[[step]]\neffect = "bloom"\n',
        encoding="utf-8",
    )
    assert not _is_a_combination(load_recipe(path))


def test_the_text_effects_in_the_demo_recipes_use_the_same_character_size_as_the_text_they_move():
    """disperse moves whole characters, so their cell must equal the ascii step's cell."""
    checked = []
    examples = [gallery.effect_files(name).example for name in registry.describe()]
    for path in [*(EXAMPLES / "recipes").glob("*.toml"), *examples]:
        steps = {run.effect: run.params for step in load_recipe(path).steps for run in runs(step)}
        if "disperse" in steps and "ascii" in steps:
            assert steps["disperse"]["cell"] == steps["ascii"]["cell"], path.name
            checked.append(path.name)
    assert checked, "no demo recipe moves ascii text, so nothing was checked"


def test_the_python_example_runs(tmp_path, capsys, echo_model, monkeypatch):
    monkeypatch.setenv(
        "BT_IMAGE_SUBJECT_MODEL", str(echo_model(0))
    )  # a stand-in subject finder: no download, same on every OS
    spec = importlib.util.spec_from_file_location("python_api_example", EXAMPLES / "python_api.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.main(tmp_path)
    assert (tmp_path / "steps" / "00.jpg").is_file()
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


def test_the_panes_example_gives_each_pane_of_its_facade_a_look_and_leaves_the_brick_alone():
    facade = imageio.load(EXAMPLES / "assets" / "facade.png")
    out = render(facade, load_recipe(EXAMPLES / "regions" / "panes.toml"))
    assert out.shape == facade.shape
    # the facade is 480 x 360: eight panes of 82 x 120 px, the first at (40, 30), then every 112 columns and 160 rows
    for number in range(8):
        top, left = 40 + 160 * (number // 4), 30 + 112 * (number % 4)
        centre = (top + 60, left + 41)
        assert np.abs(out[centre] - facade[centre]).max() > 0.05, f"pane {number} was not restyled"
    wall = np.ones(facade.shape[:2], bool)
    for number in range(8):
        top, left = 40 + 160 * (number // 4), 30 + 112 * (number % 4)
        wall[top - 4 : top + 124, left - 4 : left + 86] = False  # the panes and a margin for the seams drawn on them
    np.testing.assert_array_equal(out[wall], facade[wall])
