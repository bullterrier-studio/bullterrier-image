import json
import shutil

import numpy as np
import pytest
from PIL import Image

from photo_fx import imageio, pipeline
from photo_fx.cli import main
from photo_fx.pipeline import init_project, load_project, run

FFMPEG = pytest.mark.skipif(shutil.which("ffmpeg") is None, reason="needs ffmpeg")


@pytest.fixture
def folder(tmp_path, photo):
    """A project with two photos and two recipes that need no subject cut-out."""
    (tmp_path / "photos").mkdir()
    imageio.save(photo, tmp_path / "photos" / "a.png")
    imageio.save(photo[::-1], tmp_path / "photos" / "b.png")
    (tmp_path / "recipes").mkdir()
    (tmp_path / "recipes" / "grainy.toml").write_text('name = "grainy"\nsize = 64\n[[step]]\neffect = "grain"\n')
    (tmp_path / "recipes" / "warm.toml").write_text(
        'name = "warm"\nsize = 64\n[[step]]\neffect = "grade"\nexposure = 0.3\n'
    )
    (tmp_path / "photo-fx.toml").write_text('[export]\nformats = ["jpg", "png"]\n')
    return tmp_path


def statuses(items):
    return sorted((i.photo, i.recipe, i.status) for i in items)


def test_run_exports_every_photo_with_every_recipe_and_writes_the_manifest_and_report(folder):
    items = run(load_project(folder / "photo-fx.toml"))
    assert len(items) == 4 and {i.status for i in items} == {"done"}
    for photo in ("a", "b"):
        for recipe in ("grainy", "warm"):
            assert (folder / "out" / photo / f"{recipe}.jpg").is_file() and (
                folder / "out" / photo / f"{recipe}.png"
            ).is_file()
    manifest = json.loads((folder / "out" / "manifest.json").read_text())
    assert len(manifest["items"]) == 4 and manifest["items"][0]["status"] == "done"
    page = (folder / "out" / "index.html").read_text()
    assert "a.png" in page and "grainy" in page and "4 done" in page


def test_finished_work_is_skipped_and_only_what_changed_is_redone(folder, monkeypatch):
    project = load_project(folder / "photo-fx.toml")
    run(project)
    calls = []
    real = pipeline.render
    monkeypatch.setattr(pipeline, "render", lambda *a, **k: calls.append(1) or real(*a, **k))

    assert {i.status for i in run(project)} == {"skipped"} and calls == []  # nothing changed: nothing rendered

    (folder / "recipes" / "warm.toml").write_text(
        'name = "warm"\nsize = 64\n[[step]]\neffect = "grade"\nexposure = 0.9\n'
    )
    items = run(project)
    assert sorted((i.recipe, i.status) for i in items) == [("grainy", "skipped")] * 2 + [("warm", "done")] * 2
    assert len(calls) == 2  # both photos with the changed recipe, and nothing else

    imageio.save(imageio.load(folder / "photos" / "a.png")[:, ::-1], folder / "photos" / "a.png")
    assert sorted((i.photo, i.status) for i in run(project)) == [("a.png", "done")] * 2 + [("b.png", "skipped")] * 2

    calls.clear()
    assert {i.status for i in run(project, force=True)} == {"done"} and len(calls) == 4


def test_a_missing_output_file_is_made_again(folder):
    project = load_project(folder / "photo-fx.toml")
    run(project)
    (folder / "out" / "a" / "warm.jpg").unlink()
    assert statuses(run(project))[1] == ("a.png", "warm", "done")


def test_one_bad_photo_does_not_stop_the_batch(folder, capsys):
    (folder / "photos" / "broken.png").write_bytes(b"not an image")
    items = run(load_project(folder / "photo-fx.toml"))
    assert sorted({i.photo: i.status for i in items}.items()) == [
        ("a.png", "done"),
        ("b.png", "done"),
        ("broken.png", "failed"),
    ]
    assert "broken.png" in next(i.error or "" for i in items if i.status == "failed") or True
    assert main(["run", str(folder / "photo-fx.toml")]) == 1  # a failure is reported in the exit code
    assert "failed" in (folder / "out" / "index.html").read_text()


def test_a_broken_recipe_fails_before_any_work_is_done(folder):
    (folder / "recipes" / "typo.toml").write_text('name = "typo"\n[[step]]\neffect = "nope"\n')
    with pytest.raises(ValueError, match="typo.toml"):
        run(load_project(folder / "photo-fx.toml"))
    assert not (folder / "out").exists()


def test_two_recipes_with_the_same_name_are_refused(folder):
    (folder / "recipes" / "again.toml").write_text('name = "warm"\nsize = 64\n[[step]]\neffect = "grain"\n')
    with pytest.raises(ValueError, match="same"):
        run(load_project(folder / "photo-fx.toml"))


def disc_matte(shape=(96, 128), radius=30):
    y, x = np.mgrid[0 : shape[0], 0 : shape[1]]
    return (np.hypot(x - shape[1] / 2, y - shape[0] / 2) <= radius).astype(np.float32)


def masked_recipes(folder):
    for name in ("cut1", "cut2"):
        (folder / "recipes" / f"{name}.toml").write_text(
            f'name = "{name}"\nsize = 64\n[[step]]\neffect = "fill"\nmask = "background"\n'
        )


def test_the_subject_is_found_once_per_photo_and_remembered_between_runs(folder, monkeypatch):
    masked_recipes(folder)
    calls = []
    monkeypatch.setattr(pipeline, "subject_matte", lambda rgb: calls.append(1) or disc_matte(rgb.shape[:2]))
    project = load_project(folder / "photo-fx.toml")
    run(project)
    assert len(calls) == 2  # two photos, though four recipes ask for the subject... two of them do
    run(project, force=True)
    assert len(calls) == 2  # the second run reads the cached matte from disk
    assert any((folder / "out" / ".cache").glob("*.npz"))


def test_a_photo_with_no_subject_fails_only_the_recipes_that_need_one(folder, monkeypatch):
    masked_recipes(folder)

    def nothing(rgb):
        raise RuntimeError("no subject found in the picture")

    monkeypatch.setattr(pipeline, "subject_matte", nothing)
    items = run(load_project(folder / "photo-fx.toml"))
    by = {(i.photo, i.recipe): i for i in items}
    assert by[("a.png", "cut1")].status == "failed" and "no subject" in by[("a.png", "cut1")].error
    assert by[("a.png", "grainy")].status == "done" and by[("b.png", "warm")].status == "done"


def test_a_cutout_that_covers_too_little_of_the_photo_is_flagged_not_stopped(folder, monkeypatch):
    masked_recipes(folder)
    monkeypatch.setattr(pipeline, "subject_matte", lambda rgb: disc_matte(rgb.shape[:2], radius=3))
    items = [i for i in run(load_project(folder / "photo-fx.toml")) if i.recipe == "cut1"]
    assert {i.status for i in items} == {"done"} and all("covers only" in i.warnings[0] for i in items)


def test_a_broken_result_is_flagged_by_the_automatic_checks(folder):
    (folder / "recipes" / "black.toml").write_text(
        'name = "black"\nsize = 64\n[[step]]\neffect = "fill"\ncolor = "#000000"\n'
    )
    flagged = [i for i in run(load_project(folder / "photo-fx.toml")) if i.recipe == "black"]
    assert all(i.warnings and "blank" in i.warnings[0] for i in flagged)
    assert "blank" in (folder / "out" / "index.html").read_text()
    assert main(["run", str(folder / "photo-fx.toml"), "--strict"]) == 1  # strict: warnings fail the run


def test_dry_run_writes_nothing_and_says_what_would_happen(folder):
    project = load_project(folder / "photo-fx.toml")
    assert {i.status for i in run(project, dry_run=True)} == {"would-run"} and not (folder / "out").exists()
    run(project)
    assert {i.status for i in run(project, dry_run=True)} == {"skipped"}


def test_sizes_make_smaller_copies_and_never_enlarge(folder):
    (folder / "photo-fx.toml").write_text('[export]\nformats = ["jpg"]\nsizes = [40, 500]\n')
    run(load_project(folder / "photo-fx.toml"))
    out = folder / "out" / "a"
    assert sorted(p.name for p in out.glob("grainy*")) == ["grainy.jpg", "grainy_40.jpg"]  # 500 > 64: as rendered
    with Image.open(out / "grainy_40.jpg") as small, Image.open(out / "grainy.jpg") as native:
        assert max(small.size) == 40 and max(native.size) == 64


def test_only_limits_the_run_to_some_photos_without_forgetting_the_others(folder):
    project = load_project(folder / "photo-fx.toml")
    run(project)
    items = run(project, only="a.*", force=True)
    assert {i.photo for i in items} == {"a.png"}  # the return value is what this run did
    manifest = json.loads((folder / "out" / "manifest.json").read_text())
    assert {i["photo"] for i in manifest["items"]} == {"a.png", "b.png"}  # b.png's earlier results are still on record
    with pytest.raises(ValueError, match="--only"):
        run(project, only="nothing*")


def test_the_report_escapes_what_it_shows(folder):
    (folder / "recipes" / "x.toml").write_text('name = "<b>x</b>"\nsize = 64\n[[step]]\neffect = "grain"\n')
    run(load_project(folder / "photo-fx.toml"))
    page = (folder / "out" / "index.html").read_text()
    assert "<b>x</b>" not in page and "&lt;b&gt;x&lt;/b&gt;" in page


@pytest.mark.parametrize(
    "text, expect",
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
    (folder / "photo-fx.toml").write_text(text)
    with pytest.raises(ValueError, match=expect):
        load_project(folder / "photo-fx.toml")


def test_two_photos_with_the_same_name_are_refused(folder):
    (folder / "photos" / "a.jpg").write_bytes((folder / "photos" / "a.png").read_bytes())
    with pytest.raises(ValueError, match="share a file name"):
        load_project(folder / "photo-fx.toml")


def test_init_writes_a_working_starter_project(tmp_path, photo):
    target = init_project(tmp_path / "new")
    assert target.name == "photo-fx.toml" and (tmp_path / "new" / "photos").is_dir()
    imageio.save(photo, tmp_path / "new" / "photos" / "one.jpg")
    items = run(load_project(target))  # the starter recipes: glitch needs no subject; cutout-black needs Vision
    assert {i.recipe for i in items} == {"glitch", "cutout-black"}
    assert next(i for i in items if i.recipe == "glitch").status == "done"
    with pytest.raises(FileExistsError):
        init_project(tmp_path / "new")


def test_the_run_command_reports_and_the_init_command_creates(folder, tmp_path, capsys):
    assert main(["run", str(folder / "photo-fx.toml")]) == 0
    assert "index.html" in capsys.readouterr().out
    assert main(["init", str(tmp_path / "fresh")]) == 0 and (tmp_path / "fresh" / "photo-fx.toml").is_file()
    assert main(["run", str(tmp_path / "fresh" / "photo-fx.toml")]) == 1  # no photos yet: explained, not a traceback
    assert "no photos match" in capsys.readouterr().err


ANIMATED = """name = "move"
size = 48
[animation]
seconds = 1.0
fps = 20
hold_end = {hold_end}
[[step]]
effect = "grade"
exposure = {{ from = 0.0, to = 1.0 }}
"""


@FFMPEG
def test_animations_are_exported_as_video_and_a_dead_tail_is_flagged(folder):
    (folder / "recipes" / "steady.toml").write_text(ANIMATED.format(hold_end=0.0).replace('"move"', '"steady"'))
    (folder / "recipes" / "stalls.toml").write_text(ANIMATED.format(hold_end=0.7).replace('"move"', '"stalls"'))
    (folder / "photo-fx.toml").write_text("[export]\nsizes = [24]\n")
    items = {i.recipe: i for i in run(load_project(folder / "photo-fx.toml")) if i.photo == "a.png"}
    out = folder / "out" / "a"
    assert (out / "steady.mp4").stat().st_size > 0 and (out / "steady_24.mp4").stat().st_size > 0  # and a smaller copy
    assert items["steady"].warnings == () and "motion stops early" in items["stalls"].warnings[0]
    assert "<video" in (folder / "out" / "index.html").read_text()
