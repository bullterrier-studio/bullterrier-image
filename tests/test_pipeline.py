import json
import os
import subprocess
import sys
from functools import partial
from pathlib import Path

import numpy as np
import pytest
from PIL import Image

from bullterrier_image import Each, Finder, Grid, Picture, Project, Recipe, fx, imageio
from bullterrier_image.cli import main
from bullterrier_image.core.framing import Framing
from bullterrier_image.errors import BackendUnavailableError, ProjectError, SubjectNotFoundError
from bullterrier_image.finding.subject import subject_finder
from bullterrier_image.pipeline import init_project, load_project, run


def write_recipe(folder: Path, name: str, step: str, file: str | None = None) -> None:
    """recipes/<file or name>.toml: a 64 px recipe called `name` with one step, `step` (its TOML lines)."""
    (folder / "recipes" / f"{file or name}.toml").write_text(
        f'name = "{name}"\nsize = 64\n[[step]]\n{step}\n', encoding="utf-8"
    )


def write_project(folder: Path, text: str) -> None:
    (folder / "bt-image.toml").write_text(text, encoding="utf-8")


@pytest.fixture
def folder(tmp_path, photo):
    """A project with two photos and two recipes that need no subject cut-out."""
    (tmp_path / "photos").mkdir()
    imageio.save(photo, tmp_path / "photos" / "a.png")
    imageio.save(photo[::-1], tmp_path / "photos" / "b.png")
    (tmp_path / "recipes").mkdir()
    write_recipe(tmp_path, "grainy", 'effect = "grain"')
    write_recipe(tmp_path, "warm", 'effect = "grade"\nexposure = 0.3')
    write_project(tmp_path, '[export]\nformats = ["jpg", "png"]\n')
    return tmp_path


def recipes_redone(items):
    return sorted({i.recipe for i in items if i.status == "done"})


def by_pair(items) -> dict:
    """Run results by (photo, recipe)."""
    return {(i.photo, i.recipe): i for i in items}


def test_run_exports_every_photo_with_every_recipe_and_writes_the_manifest_and_report(folder):
    items = run(load_project(folder / "bt-image.toml"))
    assert len(items) == 4
    assert {i.status for i in items} == {"done"}
    for photo in ("a", "b"):
        for recipe in ("grainy", "warm"):
            assert (folder / "out" / photo / f"{recipe}.jpg").is_file()
            assert (folder / "out" / photo / f"{recipe}.png").is_file()
    manifest = json.loads((folder / "out" / "manifest.json").read_text(encoding="utf-8"))
    assert len(manifest["items"]) == 4
    assert manifest["items"][0]["status"] == "done"
    page = (folder / "out" / "index.html").read_text(encoding="utf-8")
    assert "a.png" in page
    assert "grainy" in page
    assert "4 done" in page


SENTINEL = b"left alone"


def mark_outputs(folder) -> None:
    """Overwrite every exported file with a sentinel: a file that still holds it afterwards was not made again."""
    for path in (folder / "out").glob("*/*.*"):
        path.write_bytes(SENTINEL)


def remade(folder) -> list[tuple[str, str]]:
    """(photo, file) of every exported file that no longer holds the sentinel."""
    return sorted((p.parent.name, p.name) for p in (folder / "out").glob("*/*.*") if p.read_bytes() != SENTINEL)


def test_finished_work_is_skipped_and_only_what_changed_is_redone(folder):
    project = load_project(folder / "bt-image.toml")
    run(project)
    mark_outputs(folder)
    # nothing changed: nothing made
    assert {i.status for i in run(project)} == {"skipped"}
    assert remade(folder) == []

    write_recipe(folder, "warm", 'effect = "grade"\nexposure = 0.9')
    items = run(project)
    assert sorted((i.recipe, i.status) for i in items) == [("grainy", "skipped")] * 2 + [("warm", "done")] * 2
    assert remade(folder) == [("a", "warm.jpg"), ("a", "warm.png"), ("b", "warm.jpg"), ("b", "warm.png")]

    mark_outputs(folder)
    imageio.save(imageio.load(folder / "photos" / "a.png")[:, ::-1], folder / "photos" / "a.png")
    assert sorted((i.photo, i.status) for i in run(project)) == [("a.png", "done")] * 2 + [("b.png", "skipped")] * 2
    assert remade(folder) == [("a", "grainy.jpg"), ("a", "grainy.png"), ("a", "warm.jpg"), ("a", "warm.png")]

    mark_outputs(folder)
    assert {i.status for i in run(project, force=True)} == {"done"}
    assert len(remade(folder)) == 8


RUN_STATUSES = """
import json, sys
from bullterrier_image.pipeline import load_project, run
print(json.dumps(sorted({i.status for i in run(load_project(sys.argv[1]))})))
"""


@pytest.mark.parametrize(
    ("change", "status"),
    [
        pytest.param(
            lambda package: edit(package / "presets" / "gradient_map.toml", "#8a5a30", "#305a8a"),
            "done",
            id="an edited preset file is shipped data that shapes the result",
        ),
        pytest.param(
            lambda package: (package / "presets" / "curves.toml").rename(package / "presets" / "curves_renamed.toml"),
            "done",
            id="a data file renamed with identical bytes is another file",
        ),
        pytest.param(lambda package: None, "skipped", id="no change: nothing is made again"),
        pytest.param(
            lambda package: write(package / "effects" / "grain" / "__pycache__" / "effect.cpython-0.pyc", "b"),
            "skipped",
            id="bytecode caches are no part of what ships",
        ),
        pytest.param(
            lambda package: write(package / ".DS_Store", "x"),
            "skipped",
            id="a dotfile is no part of what ships",
        ),
        pytest.param(
            lambda package: write(package / ".cache" / "a.txt", "x"),
            "skipped",
            id="a file in a dot folder is no part of what ships",
        ),
    ],
)
def test_work_is_redone_when_shipped_data_changes_and_not_when_only_a_cache_or_dotfile_appears(
    folder, project_copy, uv_run_env, change, status
):
    write_recipe(folder, "grainy", 'effect = "gradient_map"\npreset = "sepia"')
    source = project_copy() / "src"
    package = source / "bullterrier_image"

    def run_project() -> list[str]:  # in a child that imports the (changed) copy, as a person who edited it would
        child = subprocess.run(
            [sys.executable, "-c", RUN_STATUSES, str(folder / "bt-image.toml")],
            env={**uv_run_env, "PYTHONPATH": str(source)},
            capture_output=True,
            encoding="utf-8",
            check=False,
        )
        assert child.returncode == 0, child.stderr
        return json.loads(child.stdout)

    assert run_project() == ["done"]
    change(package)
    assert run_project() == [status]


def edit(file: Path, old: str, new: str) -> None:
    """Replace `old` by `new` in the text of `file`, which must contain it."""
    text = file.read_text(encoding="utf-8")
    assert old in text
    file.write_text(text.replace(old, new), encoding="utf-8")


def write(file: Path, text: str) -> None:
    """Write `text` to `file`, making its folders."""
    file.parent.mkdir(parents=True, exist_ok=True)
    file.write_text(text, encoding="utf-8")


def write_texture(folder, value) -> None:
    """assets/tex.png: a flat picture of grey level `value`."""
    (folder / "assets").mkdir(exist_ok=True)
    imageio.save(np.full((4, 4, 3), value, np.float32), folder / "assets" / "tex.png")


def test_changing_a_picture_file_a_recipe_names_redoes_that_recipe_and_nothing_else(folder, layer_probe):
    write_texture(folder, 0.25)
    write_recipe(folder, "layered", 'effect = "layer_probe"\nimage = "../assets/tex.png"')
    project = load_project(folder / "bt-image.toml")
    run(project)
    mark_outputs(folder)
    assert {i.status for i in run(project)} == {"skipped"}  # nothing changed: nothing made

    write_texture(folder, 0.75)  # the recipe file is the same text, the picture it names is not
    items = run(project)
    assert (
        sorted((i.recipe, i.status) for i in items)
        == [("grainy", "skipped")] * 2 + [("layered", "done")] * 2 + [("warm", "skipped")] * 2
    )
    assert {recipe for _, recipe in remade(folder)} == {"layered.jpg", "layered.png"}


def test_changing_an_svg_file_a_recipe_reads_redoes_that_recipe_and_nothing_else(folder):
    (folder / "assets").mkdir()
    shape = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect id="r" width="{}" height="5"/></svg>'
    (folder / "assets" / "cut.svg").write_text(shape.format(5), encoding="utf-8")
    (folder / "recipes" / "cut.toml").write_text(
        'format = 2\nname = "cut"\nsize = 64\n[region.s]\nkind = "svg"\nfile = "../assets/cut.svg"\n'
        '[[step]]\neach = "s"\nlooks = ["none", { effect = "grade", exposure = 0.4 }]\n',
        encoding="utf-8",
    )
    project = load_project(folder / "bt-image.toml")
    run(project)
    mark_outputs(folder)
    assert {i.status for i in run(project)} == {"skipped"}

    (folder / "assets" / "cut.svg").write_text(shape.format(9), encoding="utf-8")  # same name, other shapes
    redone = sorted((i.recipe, i.status) for i in run(project) if i.status == "done")
    assert redone == [("cut", "done")] * 2


@pytest.mark.parametrize(
    "layer",
    [
        pytest.param(lambda image: fx.layer_probe(image), id="a step"),
        pytest.param(lambda image: Each("panes", [fx.layer_probe(image), "none"]), id="a look of an each step"),
    ],
)
def test_a_recipe_built_in_code_is_redone_when_its_picture_changes_though_its_repr_does_not(folder, layer_probe, layer):
    def project_with(image: Picture) -> Project:
        recipe = Recipe("layered", [layer(image)], size=64, regions={"panes": Grid(columns=2)})
        return Project(photos=(folder / "photos" / "a.png",), recipes=(recipe,), output=folder / "out")

    dark = Picture(np.full((4, 4, 3), 0.25, np.float32), "tex")
    light = Picture(np.full((4, 4, 3), 0.75, np.float32), "tex")  # same label and size
    assert {i.status for i in run(project_with(dark))} == {"done"}
    assert {i.status for i in run(project_with(dark))} == {"skipped"}
    assert {i.status for i in run(project_with(light))} == {"done"}


def test_a_missing_output_file_is_made_again(folder):
    project = load_project(folder / "bt-image.toml")
    run(project)
    (folder / "out" / "a" / "warm.jpg").unlink()
    assert by_pair(run(project))[("a.png", "warm")].status == "done"


def test_one_bad_photo_does_not_stop_the_batch(folder, capsys):
    (folder / "photos" / "broken.png").write_bytes(b"not an image")
    items = run(load_project(folder / "bt-image.toml"))
    assert sorted({i.photo: i.status for i in items}.items()) == [
        ("a.png", "done"),
        ("b.png", "done"),
        ("broken.png", "failed"),
    ]
    failed = [i for i in items if i.status == "failed"]
    # it says which file could not be read
    assert len(failed) == 2
    assert all("broken.png" in str(i.error) for i in failed)
    assert main(["run", str(folder / "bt-image.toml")]) == 1  # a failure is reported in the exit code
    assert "failed" in (folder / "out" / "index.html").read_text(encoding="utf-8")


def test_a_broken_recipe_fails_before_any_work_is_done(folder):
    (folder / "recipes" / "typo.toml").write_text('name = "typo"\n[[step]]\neffect = "nope"\n', encoding="utf-8")
    with pytest.raises(ValueError, match=r"typo\.toml"):
        run(load_project(folder / "bt-image.toml"))
    assert not (folder / "out").exists()


def test_two_recipes_with_the_same_name_are_refused(folder):
    write_recipe(folder, "warm", 'effect = "grain"', file="again")
    with pytest.raises(ProjectError, match="same"):
        run(load_project(folder / "bt-image.toml"))


def disc_matte(shape=(96, 128), radius=30):
    y, x = np.mgrid[0 : shape[0], 0 : shape[1]]
    return (np.hypot(x - shape[1] / 2, y - shape[0] / 2) <= radius).astype(np.float32)


def masked_recipes(folder) -> list[str]:
    """Two recipes that use the subject; returns their names."""
    for name in ("cut1", "cut2"):
        write_recipe(folder, name, 'effect = "fill"\nmask = "background"')
    return ["cut1", "cut2"]


def deep_recipes(folder) -> list[str]:
    """Two recipes that take the depth map (needs the `depth_probe` fixture); returns their names."""
    for name in ("deep1", "deep2"):
        write_recipe(folder, name, 'effect = "depth_probe"')
    return ["deep1", "deep2"]


# Each map a recipe can need: `run` given a finder of it, and what writes the project's two recipes that need it.
MAPS = {
    "the subject": (
        lambda project, finder, **options: run(project, find_subject=finder, **options),
        masked_recipes,
    ),
    "the depth": (
        lambda project, finder, **options: run(project, find_depth=finder, **options),
        deep_recipes,
    ),
}


class StandInFinder:
    """Stands in for a model that finds a map (the external boundary): records the photos it is asked about and
    returns a disc of `radius` px (a subject's alpha, or a depth map with the disc nearest), or raises `error` when
    one is given."""

    def __init__(self, radius: int = 30, error: Exception | None = None):
        self.radius, self.error = radius, error
        self.seen: list[np.ndarray] = []

    def __call__(self, rgb: np.ndarray) -> np.ndarray:
        self.seen.append(rgb.copy())
        if self.error is not None:
            raise self.error
        return disc_matte(rgb.shape[:2], self.radius)


def finds_a_disc(rgb, radius):
    return disc_matte(rgb.shape[:2], radius)


@pytest.mark.parametrize(("run_with", "recipes"), MAPS.values(), ids=MAPS.keys())
def test_a_map_is_found_once_per_photo_and_remembered_between_runs(folder, depth_probe, run_with, recipes):
    recipes(folder)
    finder = StandInFinder()
    project = load_project(folder / "bt-image.toml")
    assert {i.status for i in run_with(project, Finder("stand-in", finder))} == {"done"}
    assert len(finder.seen) == 2  # two photos; two of the four recipes need the map, and ask once per photo
    photos = sorted(imageio.load(folder / "photos" / name).tobytes() for name in ("a.png", "b.png"))
    assert sorted(seen.tobytes() for seen in finder.seen) == photos  # each photo itself, at full size
    run_with(project, Finder("stand-in", finder), force=True)
    assert len(finder.seen) == 2  # redoing everything reads the remembered map instead of looking again


@pytest.mark.parametrize(("run_with", "recipes"), MAPS.values(), ids=MAPS.keys())
def test_a_damaged_remembered_map_is_found_again_even_when_forcing(folder, depth_probe, run_with, recipes):
    recipes(folder)
    finder = StandInFinder()
    project = load_project(folder / "bt-image.toml")
    run_with(project, Finder("stand-in", finder))
    for remembered in project.cache.iterdir():  # cut short, as by a full disk or a crash
        remembered.write_bytes(remembered.read_bytes()[:40])
    assert {i.status for i in run_with(project, Finder("stand-in", finder), force=True)} == {"done"}
    assert len(finder.seen) == 4  # both photos searched again
    run_with(project, Finder("stand-in", finder), force=True)
    assert len(finder.seen) == 4  # and what was found again is remembered


def test_the_depth_a_finder_finds_is_the_map_the_recipe_takes(folder, depth_probe):
    (folder / "recipes" / "deep.toml").write_text(  # at the photo's own size: the map is not scaled
        'name = "deep"\nsize = 128\n[[step]]\neffect = "depth_probe"\n', encoding="utf-8"
    )
    write_project(folder, '[export]\nformats = ["png"]\n')
    run(load_project(folder / "bt-image.toml"), find_depth=Finder("stand-in", StandInFinder(radius=30)))
    painted = imageio.load(folder / "out" / "a" / "deep.png")[..., 0]  # depth_probe paints the depth it is given
    np.testing.assert_array_equal(painted, disc_matte())  # the 128x96 photo's disc: 1 inside, 0 outside


# A first run with the wide finder for both maps, then a second with (subject finder, depth finder): each map is known
# by its own finder's identity, so changing one finder redoes only the recipes that use its map.
WIDE = Finder("wide disc", partial(finds_a_disc, radius=30))
NARROW = Finder("narrow disc", partial(finds_a_disc, radius=3))
CHANGED_FINDERS = {
    "another depth finder redoes only the recipes that take depth": (WIDE, NARROW, ["deep1", "deep2"]),
    "another subject finder redoes only the recipes that use the subject": (NARROW, WIDE, ["cut1", "cut2"]),
    "the same finders redo nothing": (WIDE, WIDE, []),
}


@pytest.mark.parametrize(("subject", "depth", "redone"), CHANGED_FINDERS.values(), ids=CHANGED_FINDERS.keys())
def test_changing_one_finder_redoes_only_the_recipes_that_use_its_map(folder, depth_probe, subject, depth, redone):
    masked_recipes(folder)
    deep_recipes(folder)
    project = load_project(folder / "bt-image.toml")
    run(project, find_subject=WIDE, find_depth=WIDE)
    assert recipes_redone(run(project, find_subject=subject, find_depth=depth)) == redone


def test_a_subject_and_a_depth_found_by_finders_of_one_identity_are_remembered_apart(folder, depth_probe):
    masked_recipes(folder)
    deep_recipes(folder)
    project = load_project(folder / "bt-image.toml")
    subject, depth = StandInFinder(radius=30), StandInFinder(radius=10)
    find_subject, find_depth = Finder("one model", subject), Finder("one model", depth)
    assert {i.status for i in run(project, find_subject=find_subject, find_depth=find_depth)} == {"done"}
    assert (len(subject.seen), len(depth.seen)) == (2, 2)  # each asked once per photo: neither reads the other's map
    run(project, force=True, find_subject=find_subject, find_depth=find_depth)
    assert (len(subject.seen), len(depth.seen)) == (2, 2)  # and each remembered under its own name


# A second run with a finder of a wide disc's answers replaced by a narrow disc's (one function, other options): known
# by its identity alone, so another identity redoes what uses the subject and the same one reuses what is remembered.
SECOND_FINDERS = {
    "another identity finds the subjects again for the recipes that use them": ("narrow disc", ["cut1", "cut2"], True),
    "the same identity reuses the remembered subjects, whatever the function": ("wide disc", [], False),
}


@pytest.mark.parametrize(("identity", "redone", "changed"), SECOND_FINDERS.values(), ids=SECOND_FINDERS.keys())
def test_a_finder_is_known_by_its_identity(folder, identity, redone, changed):
    masked_recipes(folder)
    project = load_project(folder / "bt-image.toml")
    run(project, find_subject=Finder("wide disc", partial(finds_a_disc, radius=30)))
    picture = folder / "out" / "a" / "cut1.png"
    before = picture.read_bytes()
    items = run(project, find_subject=Finder(identity, partial(finds_a_disc, radius=3)))
    assert recipes_redone(items) == redone
    assert (picture.read_bytes() != before) is changed  # the narrow disc's cut-out, or the remembered wide one


def test_the_default_finder_is_remembered_until_the_segmenter_model_changes(folder, echo_model, monkeypatch):
    masked_recipes(folder)
    project = load_project(folder / "bt-image.toml")
    monkeypatch.setenv("BT_IMAGE_SUBJECT_MODEL", str(echo_model(0)))
    assert recipes_redone(run(project, find_subject=subject_finder("onnx"))) == ["cut1", "cut2", "grainy", "warm"]
    again = recipes_redone(run(project, find_subject=subject_finder("onnx")))
    assert again == []  # same segmenter and model: everything is up to date
    monkeypatch.setenv("BT_IMAGE_SUBJECT_MODEL", str(echo_model(1)))  # another model finds other subjects
    redone = recipes_redone(run(project, find_subject=subject_finder("onnx")))
    assert redone == ["cut1", "cut2"]  # only what uses the subject is redone


def test_the_segmenter_chosen_by_the_environment_is_part_of_what_is_remembered(folder, echo_model, monkeypatch):
    masked_recipes(folder)
    project = load_project(folder / "bt-image.toml")
    monkeypatch.setenv("BT_IMAGE_SUBJECT_MODEL", str(echo_model(0)))
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "onnx")
    run(project)
    assert recipes_redone(run(project)) == []
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "nonsense")  # not a segmenter: refused, not mistaken for the old one
    with pytest.raises(BackendUnavailableError, match="BT_IMAGE_SEGMENTER"):
        run(project)


def test_a_segmenter_setting_nobody_uses_does_not_stop_a_project_without_masked_recipes(folder, monkeypatch):
    monkeypatch.setenv("BT_IMAGE_SEGMENTER", "nonsense")
    assert {i.status for i in run(load_project(folder / "bt-image.toml"))} == {"done"}


def test_an_interrupted_run_keeps_the_record_of_what_it_finished(folder):
    masked_recipes(folder)
    project = load_project(folder / "bt-image.toml")
    finder = StandInFinder()

    def interrupted_at_the_second_photo(message):
        if "b.png" in message:  # just after b.png's first pair is finished
            raise KeyboardInterrupt  # Ctrl-C

    with pytest.raises(KeyboardInterrupt):
        run(project, find_subject=Finder("stand-in", finder), log=interrupted_at_the_second_photo)
    items = run(project, find_subject=Finder("stand-in", finder))
    assert sorted((i.photo, i.recipe, i.status) for i in items) == [
        ("a.png", "cut1", "skipped"), ("a.png", "cut2", "skipped"), ("a.png", "grainy", "skipped"),
        ("a.png", "warm", "skipped"), ("b.png", "cut1", "skipped"), ("b.png", "cut2", "done"),
        ("b.png", "grainy", "done"), ("b.png", "warm", "done"),
    ]  # fmt: skip


@pytest.mark.parametrize(("run_with", "recipes"), MAPS.values(), ids=MAPS.keys())
def test_a_photo_whose_map_is_not_found_fails_only_the_recipes_that_need_it_and_is_searched_once(
    folder, depth_probe, run_with, recipes
):
    needing = recipes(folder)
    finder = StandInFinder(error=RuntimeError("nothing found in the picture"))
    by = by_pair(run_with(load_project(folder / "bt-image.toml"), Finder("stand-in", finder)))
    for recipe in needing:
        assert by[("a.png", recipe)].status == "failed"
        assert "nothing found" in str(by[("a.png", recipe)].error)
    assert by[("a.png", "grainy")].status == "done"
    assert by[("b.png", "warm")].status == "done"
    assert len(finder.seen) == 2  # a failed search is not repeated for the photo's second recipe that needs it


def test_a_project_that_needs_depth_is_refused_at_once_when_the_depth_model_cannot_run(folder, depth_probe):
    deep_recipes(folder)  # the suite's BT_IMAGE_DEPTH_MODEL names no file
    with pytest.raises(BackendUnavailableError, match="BT_IMAGE_DEPTH_MODEL"):
        run(load_project(folder / "bt-image.toml"))
    assert not (folder / "out").exists()


def test_the_default_depth_finder_is_remembered_until_the_depth_model_changes(folder, depth_probe, depth_echo_model):
    deep_recipes(folder)
    project = load_project(folder / "bt-image.toml")
    depth_echo_model(0)  # near where the photo is red
    assert recipes_redone(run(project)) == ["deep1", "deep2", "grainy", "warm"]
    assert recipes_redone(run(project)) == []  # same model: everything is up to date
    depth_echo_model(1)  # another model: near where the photo is green
    assert recipes_redone(run(project)) == ["deep1", "deep2"]  # only what uses the depth is redone


def test_a_depth_map_of_another_size_fails_the_recipes_that_take_it_naming_both_sizes_and_is_asked_once(
    folder, depth_probe
):
    deep_recipes(folder)
    asked: list[tuple[int, ...]] = []

    def finds_half_the_photo(rgb):
        asked.append(rgb.shape)
        return np.zeros((48, 64), np.float32)

    project = load_project(folder / "bt-image.toml")
    by = by_pair(run(project, find_depth=Finder("half size", finds_half_the_photo)))
    for recipe in ("deep1", "deep2"):
        assert by[("a.png", recipe)].status == "failed"
        assert "(48, 64)" in str(by[("a.png", recipe)].error)
        assert "(96, 128)" in str(by[("a.png", recipe)].error)
    assert by[("a.png", "warm")].status == "done"
    assert len(asked) == 2  # once per photo, not once per recipe
    assert list(project.cache.glob("*")) == []  # a map that does not fit is never remembered


@pytest.mark.skipif(
    sys.platform == "win32" or os.geteuid() == 0, reason="makes a folder read-only with chmod: POSIX, not as root"
)
def test_a_map_the_cache_cannot_keep_fails_only_its_pair_and_is_found_again_for_the_next(folder):
    masked_recipes(folder)
    project = load_project(folder / "bt-image.toml")
    project.cache.mkdir(parents=True)
    project.cache.chmod(0o500)  # read-only, as a locked or full disk refuses the write
    finder = StandInFinder()

    def repaired_after_the_first_pair(message):
        project.cache.chmod(0o700)

    found = Finder("stand-in", finder)
    by = by_pair(run(project, only="a.png", find_subject=found, log=repaired_after_the_first_pair))
    assert by[("a.png", "cut1")].status == "failed"
    assert "PermissionError" in str(by[("a.png", "cut1")].error)
    assert by[("a.png", "cut2")].status == "done"  # a disk failure is not the map's: the next pair finds it again
    assert len(finder.seen) == 2


def test_a_finder_that_returns_nothing_fails_the_recipes_that_need_the_subject_and_says_so(folder):
    masked_recipes(folder)

    def finds_nothing(rgb):
        return None  # a broken finder, typed loosely on purpose: the point of the test

    by = by_pair(run(load_project(folder / "bt-image.toml"), find_subject=Finder("stand-in", finds_nothing)))
    assert by[("a.png", "cut1")].status == "failed"
    assert "`find_subject` returned None" in str(by[("a.png", "cut1")].error)
    assert by[("a.png", "grainy")].status == "done"


def test_a_cutout_that_covers_too_little_of_the_photo_is_flagged_not_stopped(folder):
    masked_recipes(folder)
    items = run(load_project(folder / "bt-image.toml"), find_subject=Finder("stand-in", StandInFinder(radius=3)))
    cut = [i for i in items if i.recipe == "cut1"]
    assert {i.status for i in cut} == {"done"}
    assert all("covers only" in i.warnings[0] for i in cut)
    assert all(i.warnings == () for i in items if i.recipe == "grainy")  # recipes without a subject are not judged


def test_a_broken_result_is_flagged_by_the_automatic_checks(folder):
    write_recipe(folder, "black", 'effect = "fill"\ncolor = "#000000"')
    flagged = [i for i in run(load_project(folder / "bt-image.toml")) if i.recipe == "black"]
    assert all(i.warnings and "blank" in i.warnings[0] for i in flagged)
    assert "blank" in (folder / "out" / "index.html").read_text(encoding="utf-8")
    assert main(["run", str(folder / "bt-image.toml"), "--strict"]) == 1  # strict: warnings fail the run


def test_dry_run_writes_nothing_and_says_what_would_happen(folder):
    project = load_project(folder / "bt-image.toml")
    assert {i.status for i in run(project, dry_run=True)} == {"would-run"}
    assert not (folder / "out").exists()
    run(project)
    assert {i.status for i in run(project, dry_run=True)} == {"skipped"}


def test_sizes_make_smaller_copies_and_never_enlarge(folder):
    write_project(folder, '[export]\nformats = ["jpg"]\nsizes = [40, 500]\n')
    run(load_project(folder / "bt-image.toml"))
    out = folder / "out" / "a"
    assert sorted(p.name for p in out.glob("grainy*")) == ["grainy.jpg", "grainy_40.jpg"]  # 500 > 64: as rendered
    with Image.open(out / "grainy_40.jpg") as small, Image.open(out / "grainy.jpg") as native:
        assert max(small.size) == 40
        assert max(native.size) == 64


def test_only_limits_the_run_to_some_photos_without_forgetting_the_others(folder):
    project = load_project(folder / "bt-image.toml")
    run(project)
    items = run(project, only="a.*", force=True)
    assert {i.photo for i in items} == {"a.png"}  # the return value is what this run did
    manifest = json.loads((folder / "out" / "manifest.json").read_text(encoding="utf-8"))
    assert {i["photo"] for i in manifest["items"]} == {"a.png", "b.png"}  # b.png's earlier results are still on record
    with pytest.raises(ProjectError, match="--only"):
        run(project, only="nothing*")


def test_the_report_escapes_what_it_shows(folder, photo):
    # a photo's file name may hold characters HTML gives a meaning (a recipe name may not: it must be a safe file
    # name); `&` and `'` are legal file-name characters on every system, `<` and `>` are not on Windows
    imageio.save(photo, folder / "photos" / "Tom's x&y.png")
    run(load_project(folder / "bt-image.toml"))
    page = (folder / "out" / "index.html").read_text(encoding="utf-8")
    assert "x&y" not in page
    assert "Tom&#x27;s x&amp;y" in page


RUN_IN_CHILD = """
import json, locale, sys
from bullterrier_image.pipeline import load_project, run
print(locale.getencoding())
print(json.dumps([[i.photo, i.status, i.error] for i in run(load_project(sys.argv[1]))]))
"""


def test_text_files_are_utf8_whatever_the_locale(folder, photo):
    # Windows' default is cp1252, not UTF-8 (there `LC_ALL` is ignored and the code page applies). Elsewhere the C
    # locale with UTF-8 mode off, which every POSIX system has, stands in for it: ASCII, so no text survives a write or
    # read in the locale's encoding. On Linux the photo's name is then listed with its undecodable bytes as surrogates.
    imageio.save(photo, folder / "photos" / "Jānis-č.png")
    write_recipe(folder, "grainy", '# Jānis’ grain, fine as sand\neffect = "grain"')
    child = subprocess.run(
        [sys.executable, "-c", RUN_IN_CHILD, str(folder / "bt-image.toml")],
        env={**os.environ, "LC_ALL": "C", "PYTHONUTF8": "0"},
        capture_output=True,
        check=False,
    )
    out, err = child.stdout.decode("ascii", "replace"), child.stderr.decode("latin-1")
    encoding = out.splitlines()[0] if out else "?"
    assert encoding.lower().replace("-", "") not in ("utf8", "?"), f"the child's locale is still {encoding}: {err}"
    assert child.returncode == 0, err
    assert {status for _, status, _ in json.loads(out.splitlines()[1])} == {"done"}
    manifest = json.loads((folder / "out" / "manifest.json").read_text(encoding="utf-8"))
    assert "Jānis-č.png" in {item["photo"] for item in manifest["items"]}
    assert "Jānis-č" in (folder / "out" / "index.html").read_text(encoding="utf-8")


@pytest.mark.parametrize(
    ("text", "expect"),
    [
        ('[export]\nformats = ["tiff"]\n', "formats"),
        ("[export]\nsizes = [4]\n", "sizes"),
        ("[export]\nquality = 500\n", "quality"),
        ("[export]\nspeed = 3\n", "unknown keys"),
        ('colour = "red"\n', "unknown keys"),
        ("[checks]\nsubject_share = [0.9, 0.1]\n", "subject_share"),
        ('photos = ["nothing/*.jpg"]\n', "no photos match"),
        ('recipes = ["missing"]\n', "no recipe files"),
    ],
)
def test_a_bad_project_file_is_explained(folder, text, expect):
    write_project(folder, text)
    with pytest.raises(ProjectError, match=expect):
        load_project(folder / "bt-image.toml")


def test_a_project_file_that_is_not_toml_is_named_in_the_error(folder):
    write_project(folder, 'output = "out"\nrecipes = = ["recipes"]\n')
    with pytest.raises(ProjectError, match=r"bt-image\.toml.*line 2"):
        load_project(folder / "bt-image.toml")


def test_two_photos_with_the_same_name_are_refused(folder):
    (folder / "photos" / "a.jpg").write_bytes((folder / "photos" / "a.png").read_bytes())
    with pytest.raises(ProjectError, match="share a file name"):
        load_project(folder / "bt-image.toml")


@pytest.mark.parametrize(
    ("option", "expect"),
    [
        ({"quality": 500}, "quality.*500"),
        ({"formats": ("bmp",)}, "formats.*bmp"),
        ({"formats": ()}, "formats"),
        ({"sizes": (4,)}, "sizes.*4"),
        ({"subject_share": (0.9, 0.1)}, "subject_share"),
        ({"steps": "yes"}, "steps.*yes"),
        ({"colour": "red"}, "colour"),
    ],
)
def test_a_project_built_in_code_is_checked_like_a_project_file(folder, option, expect):
    with pytest.raises(ProjectError, match=expect):
        Project(photos=(folder / "photos" / "a.png",), recipes=(folder / "recipes" / "warm.toml",), output=folder / "o",
                **option)  # fmt: skip


@pytest.mark.skipif(Path("...png").stem != "..", reason='this pathlib gives "...png" no ".." stem (Python 3.14+)')
def test_a_photo_name_cannot_carry_files_out_of_the_output_folder(folder):
    # "...png" is a legal file name whose stem, "..", would make the photo's output folder the output's parent
    (folder / "photos" / "...png").write_bytes((folder / "photos" / "a.png").read_bytes())
    project = Project(
        photos=(folder / "photos" / "...png", folder / "photos" / "a.png"),
        recipes=(folder / "recipes" / "grainy.toml",),
        output=folder / "out",
    )
    by_photo = {i.photo: i for i in run(project)}
    assert by_photo["...png"].status == "failed"
    assert "outside the output folder" in str(by_photo["...png"].error)
    assert by_photo["a.png"].status == "done"  # the other photos still run
    assert sorted(p.name for p in folder.iterdir()) == ["bt-image.toml", "out", "photos", "recipes"]


def test_a_link_in_the_output_folder_cannot_carry_a_photos_files_elsewhere(folder):
    elsewhere = folder / "elsewhere"
    elsewhere.mkdir()
    (folder / "out").mkdir()
    try:
        (folder / "out" / "a").symlink_to(elsewhere, target_is_directory=True)  # where a.png's results would go
    except OSError:  # Windows without the symlink privilege
        pytest.skip("this account cannot create symbolic links")
    project = Project(
        photos=(folder / "photos" / "a.png", folder / "photos" / "b.png"),
        recipes=(folder / "recipes" / "grainy.toml",),
        output=folder / "out",
    )
    by_photo = {i.photo: i for i in run(project)}
    assert by_photo["a.png"].status == "failed"
    assert "outside the output folder" in str(by_photo["a.png"].error)
    assert list(elsewhere.iterdir()) == []
    assert by_photo["b.png"].status == "done"  # the other photos still run


def test_a_project_needs_a_photo_and_a_recipe(folder):
    with pytest.raises(ProjectError, match="photos"):
        Project(photos=(), recipes=(folder / "recipes" / "warm.toml",), output=folder / "o")
    with pytest.raises(ProjectError, match="recipes"):
        Project(photos=(folder / "photos" / "a.png",), recipes=(), output=folder / "o")


def test_init_writes_a_working_starter_project(tmp_path, photo):
    target = init_project(tmp_path / "new")
    assert target.name == "bt-image.toml"
    assert (tmp_path / "new" / "photos").is_dir()
    imageio.save(photo, tmp_path / "new" / "photos" / "one.jpg")
    finder = StandInFinder()  # cutout-black needs the subject; the stand-in keeps the model out of the test
    items = run(load_project(target), find_subject=Finder("stand-in", finder))
    assert sorted((i.recipe, i.status) for i in items) == [("cutout-black", "done"), ("glitch", "done")]
    assert len(finder.seen) == 1
    with pytest.raises(FileExistsError):
        init_project(tmp_path / "new")


def test_the_run_command_reports_and_the_init_command_creates(folder, tmp_path, capsys):
    assert main(["run", str(folder / "bt-image.toml")]) == 0
    assert "index.html" in capsys.readouterr().out
    assert main(["init", str(tmp_path / "fresh")]) == 0
    assert (tmp_path / "fresh" / "bt-image.toml").is_file()
    assert main(["run", str(tmp_path / "fresh" / "bt-image.toml")]) == 1  # no photos yet: explained, not a traceback
    assert "no photos match" in capsys.readouterr().err


def circle_project(folder, **settings) -> Project:
    recipe = Recipe("round", [fx.grade(exposure=0.3)], size=64, frame=Framing(transparent="circle"))
    return Project(photos=(folder / "photos" / "a.png",), recipes=(recipe,), output=folder / "out", **settings)


def test_a_recipe_with_transparency_is_refused_for_jpeg_before_anything_is_rendered(folder):
    with pytest.raises(ProjectError, match=r"'round'.*jpg|jpg.*'round'"):
        run(circle_project(folder, formats=("png", "jpg")))
    assert not (folder / "out").exists()
    run(circle_project(folder, formats=("png",)))  # known good: the same recipe to PNG
    assert (folder / "out" / "a" / "round.png").exists()


@pytest.mark.parametrize("fmt", ["png", "webp"])
@pytest.mark.parametrize(
    ("sizes", "name", "side"), [((), "round", 64), ((32,), "round_32", 32)], ids=["as rendered", "smaller"]
)
def test_a_recipe_with_transparency_is_exported_with_its_alpha_at_every_size(folder, fmt, sizes, name, side):
    run(circle_project(folder, formats=(fmt,), sizes=sizes))
    with Image.open(folder / "out" / "a" / f"{name}.{fmt}") as img:
        assert img.mode == "RGBA"
        alpha = np.asarray(img)[..., 3]
    assert alpha.shape == (side * 3 // 4, side)  # the 128x96 photo at that longest side
    assert alpha[0, 0] == 0
    assert alpha[side * 3 // 8, side // 2] == 255


def test_the_steps_of_a_recipe_with_transparency_are_saved_and_the_cut_is_a_png(folder):
    run(circle_project(folder, formats=("png",), steps=True))
    steps = folder / "out" / "a" / "steps" / "round"
    assert sorted(path.name for path in steps.iterdir()) == [
        "00-input.jpg",
        "01-grade.jpg",
        "02-transparent.png",
        "_sheet.jpg",
    ]
    with Image.open(steps / "02-transparent.png") as img:
        assert img.mode == "RGBA"


GRID_FIRST_RECIPE = """format = 2
name = "grid-first"
size = 64

[region.cells]
kind = "grid"
columns = 2
rows = 2

[region.found]
kind = "subject"
per = "cells"
context = 0

[[step]]
effect = "fill"
mask = "found"
"""


class PieceFinder:
    """Stands in for the subject-finding model: records the crops it is asked about, finds a soft square in each (a
    value a 16-bit store rounds), and finds nothing in the crops it is told to leave empty."""

    def __init__(self, empty: tuple[int, ...] = ()):
        self.empty = empty
        self.crops: list[np.ndarray] = []

    def __call__(self, rgb: np.ndarray) -> np.ndarray:
        self.crops.append(rgb.copy())
        if len(self.crops) in self.empty:
            raise SubjectNotFoundError("nothing stands out")
        matte = np.zeros(rgb.shape[:2], np.float32)
        matte[2:8, 2:8] = 0.9001  # not a value 16 bits hold exactly
        return matte


def grid_first_project(folder) -> Project:
    (folder / "recipes" / "grid-first.toml").write_text(GRID_FIRST_RECIPE, encoding="utf-8")
    write_project(folder, '[export]\nformats = ["png"]\n')
    return load_project(folder / "bt-image.toml")


def test_the_subject_of_each_piece_is_remembered_between_runs_so_the_model_is_not_asked_again(folder):
    project = grid_first_project(folder)
    finder = PieceFinder()
    run(project, only="a.png", find_subject=Finder("stand-in", finder))
    assert len(finder.crops) == 4  # one for each piece of the picture, at the recipe's size
    assert {crop.shape for crop in finder.crops} == {(24, 32, 3)}
    first = (folder / "out" / "a" / "grid-first.png").read_bytes()
    run(project, only="a.png", force=True, find_subject=Finder("stand-in", finder))
    assert len(finder.crops) == 4  # the repeat render reads what was found
    assert (folder / "out" / "a" / "grid-first.png").read_bytes() == first  # and comes out the same picture


def test_a_piece_the_finder_found_nothing_in_is_remembered_too(folder):
    project = grid_first_project(folder)
    finder = PieceFinder(empty=(1, 3))
    run(project, only="a.png", find_subject=Finder("stand-in", finder))
    first = (folder / "out" / "a" / "grid-first.png").read_bytes()
    run(project, only="a.png", force=True, find_subject=Finder("stand-in", finder))
    assert len(finder.crops) == 4
    assert (folder / "out" / "a" / "grid-first.png").read_bytes() == first


def test_a_piece_finder_that_answers_values_that_are_not_finite_fails_the_recipe_and_says_so(folder):
    project = grid_first_project(folder)

    def answers_nan(rgb):
        return np.full(rgb.shape[:2], np.nan, np.float32)

    by = by_pair(run(project, only="a.png", find_subject=Finder("stand-in", answers_nan)))
    assert by[("a.png", "grid-first")].status == "failed"
    assert "not finite" in str(by[("a.png", "grid-first")].error)


def test_another_photo_and_a_recipe_that_cuts_other_pieces_ask_about_their_own_crops(folder):
    project = grid_first_project(folder)
    finder = PieceFinder()
    run(project, find_subject=Finder("stand-in", finder))
    assert len(finder.crops) == 8  # a.png and b.png (the photo flipped) have other pixels in their pieces
    (folder / "recipes" / "grid-first.toml").write_text(
        GRID_FIRST_RECIPE.replace("columns = 2", "columns = 3"), encoding="utf-8"
    )
    run(project, only="a.png", find_subject=Finder("stand-in", finder))
    assert len(finder.crops) == 8 + 6  # other pieces of the same photo: other crops, asked about once each


def finds_a_square(rgb, start, stop):
    matte = np.zeros(rgb.shape[:2], np.float32)
    matte[start:stop, start:stop] = 1
    return matte


def test_a_finder_of_another_identity_finds_the_pieces_again(folder):
    project = grid_first_project(folder)
    run(project, only="a.png", find_subject=Finder("square 2..8", partial(finds_a_square, start=2, stop=8)))
    picture = folder / "out" / "a" / "grid-first.png"
    before = picture.read_bytes()
    another = Finder("square 4..9", partial(finds_a_square, start=4, stop=9))
    by_recipe = {i.recipe: i.status for i in run(project, only="a.png", find_subject=another)}
    assert by_recipe["grid-first"] == "done"  # a recipe that uses what the finder finds is redone
    assert by_recipe["grainy"] == "skipped"
    assert picture.read_bytes() != before  # the other finder's squares, not the remembered ones
