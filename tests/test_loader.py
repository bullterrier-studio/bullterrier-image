"""Reading a recipe file: anything wrong with the file itself (its TOML, its encoding, its format version, its top-level
keys) is a `RecipeError` that starts with the file's name."""

import re
from pathlib import Path

import numpy as np
import pytest
from PIL import Image

from bullterrier_image.core import looks, registry
from bullterrier_image.core.framing import Framing
from bullterrier_image.core.params import Drive
from bullterrier_image.core.steps import Each, Recipe, Step, drives, maps_needed, runs
from bullterrier_image.errors import ImageReadError, RecipeError, UnknownEffectError
from bullterrier_image.loader import load_recipe
from bullterrier_image.recipe import (
    Band,
    Bands,
    Curve,
    Cuts,
    Edge,
    Finder,
    Given,
    Grid,
    Line,
    Pick,
    Shards,
    Subject,
    Svg,
)
from bullterrier_image.recipe import Path as PathRegion

STEP = "[[step]]\neffect = 'grade'\n"


def _write(tmp_path, text, name="look.toml"):
    path = tmp_path / name
    path.write_text(text, encoding="utf-8")
    return path


def test_a_file_that_is_not_utf8_names_the_file(tmp_path):
    path = tmp_path / "latin.toml"
    path.write_bytes("name = 'café'\n".encode("latin-1") + STEP.encode())
    with pytest.raises(RecipeError, match=r"^latin\.toml: .*UTF-8"):
        load_recipe(path)


@pytest.mark.parametrize("file", ["my look.toml", "日本.toml", "con.toml", "_draft_.toml"])
def test_a_recipe_without_a_name_is_named_after_its_file_as_it_is(tmp_path, file):
    assert load_recipe(_write(tmp_path, STEP, file)).name == file.removesuffix(".toml")


def test_a_recipe_keeps_the_description_its_file_gives_and_has_none_without_one(tmp_path):
    assert load_recipe(_write(tmp_path, 'description = "Warmer."\n' + STEP)).description == "Warmer."
    assert load_recipe(_write(tmp_path, STEP)).description is None


def test_a_recipe_without_a_format_or_with_format_1_2_or_3_loads(tmp_path):
    assert runs(load_recipe(_write(tmp_path, "name = 'a'\n" + STEP, "a.toml")).steps[0])[0].effect == "grade"
    assert load_recipe(_write(tmp_path, "format = 1\nname = 'b'\n" + STEP, "b.toml")).name == "b"
    assert load_recipe(_write(tmp_path, "format = 2\nname = 'c'\n" + STEP, "c.toml")).name == "c"
    assert load_recipe(_write(tmp_path, "format = 3\nname = 'd'\n" + STEP, "d.toml")).name == "d"


@pytest.mark.parametrize(
    ("written", "size"),
    [
        pytest.param("", None, id="absent: left unset, not defaulted to some size"),
        pytest.param("size = 1024\n", 1024, id="given: kept"),
        pytest.param("size = 64\n", 64, id="given small: kept"),
    ],
)
def test_a_files_size_is_what_it_says_and_unset_when_it_says_none(tmp_path, written, size):
    assert load_recipe(_write(tmp_path, written + STEP)).size == size


@pytest.mark.parametrize(
    ("written", "shown"), [("0", "0"), ("-5", "-5"), ("'64'", "'64'"), ("64.0", "64.0"), ("true", "True")]
)
def test_a_size_that_is_not_a_positive_whole_number_is_refused_naming_it(tmp_path, written, shown):
    with pytest.raises(RecipeError, match=rf"^look\.toml: .*size.*{re.escape(shown)}"):
        load_recipe(_write(tmp_path, f"size = {written}\n" + STEP))


GIVEN = "[region.star]\nkind = 'given'\n[[step]]\neffect = 'gradient'\ncolors = ['#000000']\nmask = 'star'\n"


def test_a_given_region_loads_in_format_3_as_the_recipe_written_in_python(tmp_path):
    loaded = load_recipe(_write(tmp_path, "format = 3\nname = 'g'\nsize = 64\n" + GIVEN))
    assert loaded == Recipe(
        [Step("gradient", params={"colors": ["#000000"]}, mask="star")], name="g", size=64, regions={"star": Given()}
    )
    assert maps_needed(loaded) == frozenset({"star"})


def test_a_given_region_in_format_2_is_refused_naming_the_region_and_format_3(tmp_path):
    with pytest.raises(RecipeError, match=r"^look\.toml: region 'star': `kind` 'given' needs `format = 3`"):
        load_recipe(_write(tmp_path, "format = 2\nsize = 64\n" + GIVEN))


def test_a_format_1_file_means_the_same_in_format_2(tmp_path):
    body = "size = 64\n[frame]\naspect = 1\n" + STEP + "mask = 'subject'\nopacity = 0.5\n"
    assert (
        load_recipe(_write(tmp_path, "format = 1\n" + body, "one.toml")).steps
        == load_recipe(_write(tmp_path, "format = 2\n" + body, "two.toml")).steps
    )


@pytest.mark.parametrize(
    ("written", "value"), [("4", "4"), ("0", "0"), ("'1'", "'1'"), ("1.0", "1.0"), ("true", "True"), ("'2'", "'2'")]
)
def test_any_other_format_is_refused_naming_the_value_and_the_formats_this_release_reads(tmp_path, written, value):
    path = _write(tmp_path, f"format = {written}\n" + STEP, "future.toml")
    with pytest.raises(RecipeError, match=r"^future\.toml: .*`format`") as raised:
        load_recipe(path)
    message = str(raised.value)
    assert value in message
    assert re.search(r"reads\D*\b1\b.*\b2\b.*\b3\b", message)  # the formats this release reads


# A broken file: each is refused with a RecipeError that starts with the file's name and says what is wrong.
BROKEN = {
    "TOML syntax error, with its line and column": ("name = 'ok'\nsize = 64\ncolor = '#fff\n" + STEP, "line 3, column"),
    "misspelt size": ("sise = 2\n" + STEP, "`sise`"),
    "misspelt format": ("formt = 2\n" + STEP, "`formt`"),
    "steps for step": ("steps = 2\n" + STEP, "`steps`"),
    "an [animation] table, which no longer exists": ("[animation]\nseconds = 2\n" + STEP, "`animation`"),
    "a single [step] table instead of a list": ("[step]\neffect = 'grade'\n", r"\[\[step\]\]"),
    "an animated value, which no longer exists": (STEP + "exposure = { from = 0, to = 1 }\n", "step 1: .*exposure"),
}


@pytest.mark.parametrize(("text", "complaint"), BROKEN.values(), ids=BROKEN.keys())
def test_a_broken_file_is_a_recipe_error_naming_the_file_and_the_problem(tmp_path, text, complaint):
    with pytest.raises(RecipeError, match=rf"^broken\.toml: .*{complaint}"):
        load_recipe(_write(tmp_path, text, "broken.toml"))


# --- the framing: a [frame] table ---------------------------------------------------------------------------------


def test_a_frame_table_is_the_recipes_framing_and_without_one_the_recipe_is_not_framed(tmp_path):
    text = (
        "size = 512\n[frame]\naspect = 1\nanchor = 'subject'\nmargin = 0.1\nangle = -5\nfill = '#102030'\n"
        "transparent = 'circle'\n" + STEP
    )
    framed = load_recipe(_write(tmp_path, text, "framed.toml"))
    assert framed.frame == Framing(
        aspect=1.0, anchor="subject", margin=0.1, angle=-5, fill="#102030", transparent="circle"
    )
    assert framed.size == 512
    assert load_recipe(_write(tmp_path, STEP, "plain.toml")).frame == Framing()


FRAME_MISTAKES = {
    "a value out of range": ("[frame]\naspect = 0\n", r"\[frame\]: .*`aspect`.*0"),
    "a value of the wrong kind": ("[frame]\nflip = 3\n", r"\[frame\]: .*`flip`"),
    "a choice that is not offered": ("[frame]\nanchor = 'middle'\n", r"\[frame\]: .*`anchor`.*middle"),
    "a misspelt key": ("[frame]\nrotate = 10\n", r"\[frame\]: .*`rotate`.*angle"),
    "frame given as a list of tables": ("[[frame]]\naspect = 1\n", r"\[frame\] table"),
    "frame given as a value": ("frame = 'square'\n", r"\[frame\] table"),
}


@pytest.mark.parametrize(("text", "complaint"), FRAME_MISTAKES.values(), ids=FRAME_MISTAKES.keys())
def test_a_mistake_in_the_frame_table_names_the_file_the_table_and_the_key(tmp_path, text, complaint):
    with pytest.raises(RecipeError, match=rf"^broken\.toml: .*{complaint}"):
        load_recipe(_write(tmp_path, text + STEP, "broken.toml"))


def test_a_misspelt_top_level_key_lists_the_keys_a_recipe_has(tmp_path):
    with pytest.raises(
        RecipeError, match=r"`frames`.*format, name, description, size, \[frame\], \[region\.NAME\], \[\[step\]\]"
    ):
        load_recipe(_write(tmp_path, "[frames]\n" + STEP))


# --- presets in a recipe file ------------------------------------------------------------------------------------


def test_a_step_may_name_a_preset_and_its_own_keys_win_over_it(tmp_path):
    path = _write(tmp_path, "[[step]]\neffect = 'gradient_map'\npreset = 'sepia'\nshadow_clip = 0.2\nopacity = 0.5\n")
    (step,) = load_recipe(path).steps
    assert step == Step("gradient_map", params={"shadow_clip": 0.2}, opacity=0.5, preset="sepia")
    assert "preset" not in step.params  # a step's key, not an effect's parameter


@pytest.mark.parametrize(
    ("effect", "preset", "complaint"),
    [
        pytest.param("gradient_map", "sepya", r"gradient_map.*'sepya'.*sepia", id="a preset the effect lacks"),
        pytest.param("equalize", "sepia", r"equalize.*'sepia'.*no presets", id="an effect without presets"),
    ],
)
def test_a_wrong_preset_in_a_file_names_the_file_the_step_and_the_preset(tmp_path, effect, preset, complaint):
    path = _write(tmp_path, f"[[step]]\neffect = 'grade'\n\n[[step]]\neffect = '{effect}'\npreset = '{preset}'\n")
    with pytest.raises(RecipeError, match=rf"^look\.toml: step 2: .*{complaint}"):
        load_recipe(path)


# --- a picture parameter is a path, relative to the recipe file -------------------------------------------------------


def _logo(folder, name="logo.png"):
    """A 2 x 1 picture: an opaque red pixel and a half-transparent blue one; returns its path."""
    folder.mkdir(exist_ok=True)
    Image.fromarray(np.array([[(255, 0, 0, 255), (0, 0, 255, 128)]], np.uint8), "RGBA").save(folder / name)
    return folder / name


def _layer(folder, image, name="look.toml"):
    """A recipe file in `folder` whose second step gives the picture parameter `image` (TOML text)."""
    folder.mkdir(exist_ok=True)
    return _write(folder, f"{STEP}\n[[step]]\neffect = 'layer_probe'\nimage = {image}\n", name)


def test_a_picture_path_is_read_relative_to_the_recipe_file_keeping_its_alpha(layer_probe, tmp_path, monkeypatch):
    _logo(tmp_path / "assets")
    recipe = load_recipe(_layer(tmp_path / "recipes", "'../assets/logo.png'"))
    monkeypatch.chdir(tmp_path)  # the folder the process runs in does not matter
    image = runs(recipe.steps[1])[0].params["image"]
    assert (image.width, image.height, image.label) == (2, 1, "../assets/logo.png")
    np.testing.assert_allclose(image.pixels[0], np.array([[1, 0, 0, 1], [0, 0, 1, 128 / 255]]), atol=1e-6)


def test_an_absolute_picture_path_is_read_as_it_is(layer_probe, tmp_path):
    logo = _logo(tmp_path / "elsewhere")
    recipe = load_recipe(_layer(tmp_path / "recipes", f"'{logo}'"))
    assert runs(recipe.steps[1])[0].params["image"].width == 2


@pytest.mark.parametrize(
    ("written", "pixel"),
    [("'#ff8000'", [1, 128 / 255, 0, 1]), ("[0.25, 0.5, 1.0]", [0.25, 0.5, 1, 1])],
    ids=["hex", "numbers"],
)
def test_a_picture_parameter_may_be_a_colour_and_no_file_is_read(layer_probe, tmp_path, written, pixel):
    step = runs(load_recipe(_layer(tmp_path, written)).steps[1])[0]
    out = registry.apply("layer_probe", np.zeros((2, 3, 3), np.float32), **step.params)
    np.testing.assert_allclose(out, np.broadcast_to(pixel[:3], (2, 3, 3)), atol=1e-6)


def _not_an_image(folder):
    (folder / "words.png").write_text("not a picture", encoding="utf-8")


@pytest.mark.parametrize(
    ("written", "make", "kind"),
    [
        pytest.param("'assets/missing.png'", lambda folder: None, ImageReadError, id="missing file"),
        pytest.param("'words.png'", _not_an_image, ImageReadError, id="not an image"),
        pytest.param("'#zzzzzz'", lambda folder: None, RecipeError, id="a bad colour is not read as a path"),
    ],
)
def test_a_picture_that_cannot_be_used_fails_at_load_naming_file_step_parameter_and_value(
    layer_probe, tmp_path, written, make, kind
):
    make(tmp_path)
    with pytest.raises(kind, match=r"^look\.toml: step 2: .*`image`.*" + re.escape(str(Path(written.strip("'"))))):
        load_recipe(_layer(tmp_path, written))


# --- format 2: regions, and steps that give each piece of one a look -----------------------------------------------

REGIONS_FILE = """
format = 2
size = 64

[region.panes]
kind = "grid"
columns = [0.2, 0.17, 0.63]
rows = 2

[region.glass]
kind = "shards"
preset = "glass"
fit = "subject"
impact = [0.6, 0.2]
edge = { style = "torn", amount = 0.02 }

[region.head]
kind = "pick"
of = "panes"
pieces = [4]

[region.found]
kind = "subject"
per = "panes"
context = 0.3
threshold = 0.8

[[step]]
effect = "gradient"
colors = ["#ff0000"]
mask = "head"

[[step]]
effect = "gradient"
colors = ["#0000ff"]
mask = "found"

[[step]]
each = "glass"
looks = [
    { effect = "grade", exposure = 0.5, opacity = 0.7 },
    "none",
    { effect = "gradient", colors = ["#00ff00"], blend_mode = "multiply" },
]
assign = "random"
seed = 4
feather = 0.01
mask = "subject"
opacity = 0.9
"""

REGIONS_RECIPE = Recipe(
    name="look",
    size=64,
    regions={
        "panes": Grid(columns=[0.2, 0.17, 0.63], rows=2),
        "glass": Shards(preset="glass", fit="subject", impact=(0.6, 0.2), edge=Edge(style="torn", amount=0.02)),
        "head": Pick(of="panes", pieces=[4]),
        "found": Subject(per="panes", context=0.3, threshold=0.8),
    },
    steps=[
        Step("gradient", params={"colors": ["#ff0000"]}, mask="head"),
        Step("gradient", params={"colors": ["#0000ff"]}, mask="found"),
        Each(
            "glass",
            looks=[
                Step("grade", params={"exposure": 0.5}, opacity=0.7),
                "none",
                Step("gradient", params={"colors": ["#00ff00"]}, blend_mode="multiply"),
            ],
            mask="subject",
            opacity=0.9,
            assign="random",
            seed=4,
            feather=0.01,
        ),
    ],
)


def test_a_format_2_file_with_regions_and_an_each_step_loads_as_the_recipe_written_in_python(tmp_path):
    assert load_recipe(_write(tmp_path, REGIONS_FILE)) == REGIONS_RECIPE


def test_the_same_file_renders_what_the_python_recipe_renders(tmp_path, photo, subject):
    from bullterrier_image.core.runner import render

    loaded = load_recipe(_write(tmp_path, REGIONS_FILE))
    finder = lambda crop: np.where(crop[..., 0] > 0.5, 1.0, 0.0).astype(np.float32)  # noqa: E731  (a stand-in model)
    np.testing.assert_array_equal(
        render(photo, loaded, maps={"subject": subject}, finders={"subject": Finder("stand-in", finder)}),
        render(photo, REGIONS_RECIPE, maps={"subject": subject}, finders={"subject": Finder("stand-in", finder)}),
    )


FIELDS_FILE = """
format = 3

[region.near]
kind = "band"
of = "depth"
low = 0.6
high = 0.9
feather = 0.05

[region.bands]
kind = "bands"
of = "depth"
count = 4

[[step]]
each = "bands"
looks = [{ effect = "grade", exposure = 0.5 }, "none"]

[[step]]
effect = "gradient"
colors = ["#ff0000"]
mask = "near"
"""


def test_field_regions_load_as_the_recipe_written_in_python(tmp_path):
    written = Recipe(
        name="look",
        size=None,
        regions={"near": Band(of="depth", low=0.6, high=0.9, feather=0.05), "bands": Bands(of="depth", count=4)},
        steps=[
            Each("bands", looks=[Step("grade", params={"exposure": 0.5}), "none"]),
            Step("gradient", params={"colors": ["#ff0000"]}, mask="near"),
        ],
    )
    assert load_recipe(_write(tmp_path, FIELDS_FILE)) == written
    assert "depth" in maps_needed(written)


SHAPES_FILE = """
format = 2
size = 64

[region.halves]
kind = "cuts"
lines = [{ x = 0.5, y = 0.5, angle = 298 }, { path = "M 0.38 0 C 0.15 0.45, 0.85 0.55, 0.62 1" }]
edge = { style = "torn", amount = 0.01 }

[region.blob]
kind = "path"
d = "M 0.1 0.1 H 0.6 V 0.7 Z"
samples = 8

[region.shapes]
kind = "svg"
file = "../assets/shapes.svg"
max_shapes = 50

[region.ring]
kind = "pick"
of = "shapes"
pieces = ["ring", 0]

[[step]]
effect = "gradient"
colors = ["#ff0000"]
mask = "ring"

[[step]]
each = "halves"
looks = ["none", { effect = "gradient", colors = ["#00ff00"] }]
mask = "blob"
"""

SHAPES_SVG = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
  <circle id="ring" cx="5" cy="5" r="4"/><rect id="corner" width="3" height="3"/></svg>"""


def test_cuts_path_and_svg_regions_load_as_the_recipe_written_in_python_and_the_svg_is_found_beside_the_file(
    tmp_path, photo, subject
):
    from bullterrier_image.core.runner import render

    (tmp_path / "assets").mkdir()
    (tmp_path / "assets" / "shapes.svg").write_text(SHAPES_SVG, encoding="utf-8")
    (tmp_path / "recipes").mkdir()
    loaded = load_recipe(_write(tmp_path / "recipes", SHAPES_FILE))
    written = Recipe(
        name="look",
        size=64,
        regions={
            "halves": Cuts(
                lines=[Line(x=0.5, y=0.5, angle=298), Curve(path="M 0.38 0 C 0.15 0.45, 0.85 0.55, 0.62 1")],
                edge=Edge(style="torn", amount=0.01),
            ),
            "blob": PathRegion(d="M 0.1 0.1 H 0.6 V 0.7 Z", samples=8),
            "shapes": Svg(file=str(tmp_path / "recipes" / "../assets/shapes.svg"), max_shapes=50),
            "ring": Pick(of="shapes", pieces=["ring", 0]),
        },
        steps=[
            Step("gradient", params={"colors": ["#ff0000"]}, mask="ring"),
            Each("halves", looks=["none", Step("gradient", params={"colors": ["#00ff00"]})], mask="blob"),
        ],
    )
    assert loaded == written
    np.testing.assert_array_equal(
        render(photo, loaded, maps={"subject": subject}), render(photo, written, maps={"subject": subject})
    )


def test_a_look_may_hold_a_picture_given_as_a_path_relative_to_the_file(layer_probe, tmp_path):
    _logo(tmp_path / "assets")
    (tmp_path / "recipes").mkdir()
    path = _write(
        tmp_path / "recipes",
        'format = 2\n[region.g]\nkind = "grid"\n[[step]]\neach = "g"\n'
        "looks = [{ effect = 'layer_probe', image = '../assets/logo.png' }]\n",
    )
    (each,) = load_recipe(path).steps
    assert runs(each)[0].params["image"].label == "../assets/logo.png"


DRIVEN_FILE = """
format = 3

[region.near]
kind = "band"
of = "depth"
low = 0.5

[[step]]
effect = "bloom"
radius = { by = "near", start = 0, end = 30 }
amount = { by = "near", start = 0.1, end = 0.8 }
threshold = 0.6
"""


def test_a_parameter_written_as_a_table_with_by_is_a_drive_the_same_as_written_in_python(tmp_path):
    written = Recipe(
        name="look",
        size=None,
        regions={"near": Band(of="depth", low=0.5)},
        steps=[
            Step(
                "bloom",
                params={
                    "radius": Drive(by="near", start=0, end=30),
                    "amount": Drive(by="near", start=0.1, end=0.8),
                    "threshold": 0.6,
                },
            )
        ],
    )
    assert load_recipe(_write(tmp_path, DRIVEN_FILE)) == written
    assert maps_needed(written) == frozenset({"depth"})


def test_a_look_of_an_each_can_be_driven_in_a_recipe_file(tmp_path):
    text = (
        'format = 3\n[region.g]\nkind = "grid"\n[[step]]\neach = "g"\n'
        'looks = [{ effect = "bloom", radius = { by = "depth", start = 0, end = 9, levels = 3 } }, "none"]\n'
    )
    recipe = load_recipe(_write(tmp_path, text))
    (look,) = runs(recipe.steps[0])
    assert drives(look) == {"radius": Drive(by="depth", start=0, end=9, levels=3)}


DRIVE_MISTAKES = {
    "a parameter the effect does not declare drivable": (
        'effect = "bloom"\nthreshold = { by = "depth", start = 0, end = 0.5 }',
        r"step 1: effect 'bloom': `threshold` cannot be driven.*drivable parameters: amount, radius",
    ),
    "a whole number": (
        'effect = "halftone"\ncell = { by = "depth", start = 4, end = 12 }',
        r"step 1: effect 'halftone': `cell` cannot be driven.*whole number",
    ),
    "a drive out of the parameter's range": (
        'effect = "bloom"\nradius = { by = "depth", start = 0, end = 900 }',
        r"step 1: effect 'bloom': `radius`.*<= 500.*900",
    ),
    "a misspelt key": (
        'effect = "bloom"\nradius = { by = "depth", start = 0, end = 9, step = 2 }',
        r"step 1: effect 'bloom': `radius`: .*step",
    ),
    "a table with no end": (
        'effect = "bloom"\nradius = { by = "depth", start = 0 }',
        r"step 1: effect 'bloom': `radius`: .*end",
    ),
    "a field that is no region": (
        'effect = "bloom"\nradius = { by = "gone", start = 0, end = 9 }',
        r"step 1: `by` names 'gone', which is no region; valid selections: subject, background, depth, light",
    ),
    "a field that is a partition": (
        'effect = "bloom"\nradius = { by = "g", start = 0, end = 9 }',
        r"step 1: `by` names 'g', a partition, but it needs a selection",
    ),
    "two fields in one step": (
        'effect = "bloom"\nradius = { by = "depth", start = 0, end = 9 }\n'
        'amount = { by = "light", start = 0, end = 1 }',
        r"step 1: effect 'bloom': the drives of one step must vary by one field, got 'depth' and 'light'",
    ),
}


@pytest.mark.parametrize(("step", "complaint"), DRIVE_MISTAKES.values(), ids=DRIVE_MISTAKES.keys())
def test_a_mistake_in_a_drive_names_the_file_the_step_the_parameter_and_the_problem(tmp_path, step, complaint):
    text = 'format = 3\n[region.g]\nkind = "grid"\n[[step]]\n' + step + "\n"
    with pytest.raises(RecipeError, match=rf"^bad\.toml: .*{complaint}"):
        load_recipe(_write(tmp_path, text, "bad.toml"))


@pytest.mark.parametrize("header", ["", "format = 1\n", "format = 2\n"], ids=["no format", "format 1", "format 2"])
def test_a_drive_needs_format_3_and_a_file_of_an_earlier_one_says_so_naming_the_parameter(tmp_path, header):
    text = header + '[[step]]\neffect = "bloom"\nradius = { by = "depth", start = 0, end = 9 }\n'
    with pytest.raises(
        RecipeError, match=r"^old\.toml: step 1: `radius` is a table with `by`, a drive, which needs `format = 3`"
    ):
        load_recipe(_write(tmp_path, text, "old.toml"))


def test_a_drive_in_a_look_of_an_earlier_format_is_refused_naming_the_look(tmp_path):
    text = (
        'format = 2\n[region.g]\nkind = "grid"\n[[step]]\neach = "g"\n'
        'looks = ["none", { effect = "bloom", radius = { by = "depth", start = 0, end = 9 } }]\n'
    )
    with pytest.raises(RecipeError, match=r"^old\.toml: step 1, look 2: `radius` is a table with `by`"):
        load_recipe(_write(tmp_path, text, "old.toml"))


NAMED_FILE = """
format = 3

[region.g]
kind = "grid"

[[step]]
effect = "bloom"
name = "pulse"
radius = 30

[[step]]
each = "g"
name = "pieces"
looks = ["none", { effect = "grade", exposure = 0.5 }]
"""


def test_a_step_and_an_each_take_a_name_in_a_recipe_file_and_it_addresses_their_numbers(tmp_path):
    recipe = load_recipe(_write(tmp_path, NAMED_FILE))
    assert [step.name for step in recipe.steps] == ["pulse", "pieces"]
    numbers = recipe.parameters()
    assert numbers["pulse.radius"].value == 30
    assert numbers["pieces.feather"].value == 0.5
    assert numbers["pieces.looks[1].exposure"].value == 0.5
    assert recipe.with_values({"pulse.radius": 12}).parameters()["pulse.radius"].value == 12


NAME_MISTAKES = {
    "a name that is not a word": ("name = 'my-pulse'", r"step 1: .*`name`.*letters, digits and underscores"),
    "a reserved name": ("name = 'region'", r"step 1: .*`name`.*reserved"),
    "a name that is no text": ("name = 3", r"step 1: .*`name`"),
}


@pytest.mark.parametrize(("line", "complaint"), NAME_MISTAKES.values(), ids=NAME_MISTAKES.keys())
def test_a_bad_name_is_refused_naming_the_file_the_step_and_the_key(tmp_path, line, complaint):
    with pytest.raises(RecipeError, match=rf"^bad\.toml: .*{complaint}"):
        load_recipe(_write(tmp_path, f"format = 3\n[[step]]\neffect = 'grade'\n{line}\n", "bad.toml"))


def test_two_steps_of_a_file_with_one_name_are_refused_naming_the_file_and_the_steps(tmp_path):
    text = "format = 3\n[[step]]\neffect = 'grade'\nname = 'a'\n[[step]]\neffect = 'grade'\nname = 'a'\n"
    with pytest.raises(RecipeError, match=r"^bad\.toml: .*`name` 'a' is the name of steps 1 and 2"):
        load_recipe(_write(tmp_path, text, "bad.toml"))


@pytest.mark.parametrize("header", ["", "format = 1\n", "format = 2\n"], ids=["no format", "format 1", "format 2"])
def test_a_step_name_needs_format_3_and_a_file_of_an_earlier_one_says_so(tmp_path, header):
    text = header + "[[step]]\neffect = 'grade'\nname = 'warm'\n"
    with pytest.raises(RecipeError, match=r"^old\.toml: step 1: `name` needs `format = 3`"):
        load_recipe(_write(tmp_path, text, "old.toml"))


def test_a_look_in_a_recipe_file_takes_no_name_naming_the_look(tmp_path):
    text = (
        'format = 3\n[region.g]\nkind = "grid"\n[[step]]\neach = "g"\n'
        'looks = ["none", { effect = "grade", name = "warm" }]\n'
    )
    with pytest.raises(RecipeError, match=r"^bad\.toml: .*a look takes no `name`.*each\.looks\[1\]"):
        load_recipe(_write(tmp_path, text, "bad.toml"))


NEW_IN_FORMAT_2 = {
    "a region table": '[region.g]\nkind = "grid"\n' + STEP,
    "an each step": '[[step]]\neach = "subject"\nlooks = ["none"]\n',
    "a partition key": "[[step]]\neffect = 'grade'\npartition = 'subject'\n",
}


@pytest.mark.parametrize("text", NEW_IN_FORMAT_2.values(), ids=NEW_IN_FORMAT_2.keys())
@pytest.mark.parametrize("header", ["", "format = 1\n"], ids=["no format", "format 1"])
def test_regions_and_each_steps_need_format_2_and_a_format_1_file_says_so(tmp_path, header, text):
    with pytest.raises(RecipeError, match=r"^old\.toml: .*format = 2"):
        load_recipe(_write(tmp_path, header + text, "old.toml"))


# the kinds, in the order the error for a bad one lists them
ALL_KINDS = (
    "grid, shards, cuts, path, svg, pick, subject, band, bands, combine, linear_gradient, radial_gradient, given"
)
REGION_MISTAKES = {
    "a kind that does not exist": (
        '[region.g]\nkind = "hexagons"\n',
        rf"region 'g': .*'hexagons'.*{ALL_KINDS}",
    ),
    "no kind": (
        "[region.g]\ncolumns = 2\n",
        rf"region 'g': .*`kind`.*{ALL_KINDS}",
    ),
    "a kind that is not text": (
        "[region.g]\nkind = 3\n",
        rf"region 'g': .*`kind`.*{ALL_KINDS}",
    ),
    "a misspelt field": ('[region.g]\nkind = "grid"\ncolums = 2\n', r"region 'g'.*`colums`.*columns.*rows"),
    "a value out of range": ('[region.g]\nkind = "shards"\ncount = 0\n', r"region 'g'.*`count`.*1"),
    "an edge that is not a table": ('[region.g]\nkind = "grid"\nedge = 3\n', r"region 'g'.*`edge`"),
    "a region that is not a table": ("region = 3\n", r"\[region\.NAME\]"),
    "a fit that no region has": (
        '[region.g]\nkind = "grid"\nfit = "nope"\n',
        r"region 'g': `fit` names 'nope'.*valid selections: subject, background",
    ),
    "a fit that is a partition": (
        '[region.a]\nkind = "grid"\n[region.b]\nkind = "grid"\nfit = "a"\n',
        r"region 'b': `fit` names 'a', a partition",
    ),
    "a pick of nothing": ('[region.p]\nkind = "pick"\nof = "gone"\n', r"region 'p': `of` names 'gone'"),
    "a cycle": ('[region.a]\nkind = "shards"\nfit = "b"\n[region.b]\nkind = "pick"\nof = "a"\n', r"cycle"),
    "a name taken by a built-in": ('[region.subject]\nkind = "grid"\n', r"region 'subject' is a built-in"),
    "a band with low above high": (
        '[region.n]\nkind = "band"\nof = "depth"\nlow = 0.7\nhigh = 0.4\n',
        r"region 'n'.*`low` 0\.7.*`high` 0\.4",
    ),
    "a band feather out of range": (
        '[region.n]\nkind = "band"\nof = "depth"\nfeather = 2\n',
        r"region 'n'.*`feather`.*2",
    ),
    "no bands": ('[region.b]\nkind = "bands"\nof = "depth"\ncount = 0\n', r"region 'b'.*`count`.*0"),
    "a band of nothing": ('[region.n]\nkind = "band"\n', r"region 'n': `of` names ''.*no region"),
    "a band of a region that is not there": (
        '[region.n]\nkind = "band"\nof = "gone"\n',
        r"region 'n': `of` names 'gone'",
    ),
    "a combination of one": ('[region.c]\nkind = "combine"\nof = ["depth"]\n', r"region 'c'.*`of`"),
    "a combination with an operation that is not offered": (
        '[region.c]\nkind = "combine"\nop = "xor"\nof = ["depth", "subject"]\n',
        r"region 'c'.*`op`.*xor",
    ),
    "a gradient with an angle that is not a number": (
        '[region.g]\nkind = "linear_gradient"\nangle = "up"\n',
        r"region 'g'.*`angle`.*up",
    ),
    "a radial gradient with a radius of nothing": (
        '[region.g]\nkind = "radial_gradient"\nradius = 0\n',
        r"region 'g'.*`radius`.*0",
    ),
    "a name taken by the built-in light": ('[region.light]\nkind = "grid"\n', r"region 'light' is a built-in"),
}


@pytest.mark.parametrize(("text", "complaint"), REGION_MISTAKES.values(), ids=REGION_MISTAKES.keys())
def test_a_mistake_in_a_region_names_the_file_the_region_and_the_problem(tmp_path, text, complaint):
    with pytest.raises(RecipeError, match=rf"^bad\.toml: .*{complaint}"):
        load_recipe(_write(tmp_path, "format = 3\n" + text + STEP, "bad.toml"))


REPLACED_KINDS = {
    "depth": (
        '[region.n]\nkind = "depth"\nnear = 0.9\nfar = 0.6\n',
        r'`kind` \'depth\' is gone.*kind = "band".*of = "depth".*`low`.*`far`.*`high`.*`near`',
    ),
    "depth_bands": (
        '[region.b]\nkind = "depth_bands"\ncount = 4\n',
        r'`kind` \'depth_bands\' is gone.*kind = "bands".*of = "depth".*`count`',
    ),
}


@pytest.mark.parametrize(("text", "complaint"), REPLACED_KINDS.values(), ids=REPLACED_KINDS.keys())
@pytest.mark.parametrize("header", ["format = 2\n", "format = 3\n"])
def test_a_region_kind_format_3_replaced_is_refused_saying_what_to_write_instead(tmp_path, header, text, complaint):
    with pytest.raises(RecipeError, match=rf"^old\.toml: region '[nb]': {complaint}"):
        load_recipe(_write(tmp_path, header + text + STEP, "old.toml"))


NEW_KINDS = {
    "a band": '[region.n]\nkind = "band"\nof = "depth"\n',
    "bands": '[region.b]\nkind = "bands"\nof = "depth"\n',
    "a combination": '[region.c]\nkind = "combine"\nof = ["depth", "subject"]\n',
    "a linear gradient": '[region.g]\nkind = "linear_gradient"\n',
    "a radial gradient": '[region.g]\nkind = "radial_gradient"\n',
}


@pytest.mark.parametrize("text", NEW_KINDS.values(), ids=NEW_KINDS.keys())
def test_the_region_kinds_format_3_added_are_refused_in_format_2_naming_format_3(tmp_path, text):
    with pytest.raises(RecipeError, match=r"^old\.toml: region '\w': `kind` '\w+' needs `format = 3`"):
        load_recipe(_write(tmp_path, "format = 2\n" + text + STEP, "old.toml"))
    assert load_recipe(_write(tmp_path, "format = 3\n" + text + STEP, "new.toml")).regions  # and fine in format 3


EACH = '[region.g]\nkind = "grid"\n'
EACH_MISTAKES = {
    "looks that are not a list": ('[[step]]\neach = "g"\nlooks = "none"\n', r"step 1: .*`looks`.*list"),
    "no looks": ('[[step]]\neach = "g"\n', r"step 1: .*`looks`"),
    "looks without each": ('[[step]]\nlooks = ["none"]\n', r"step 1: .*`each`"),
    "an effect beside each": ('[[step]]\neach = "g"\nlooks = ["none"]\neffect = "grade"\n', r"step 1: .*`effect`"),
    "an effect parameter on the each step": (
        '[[step]]\neach = "g"\nlooks = ["none"]\nexposure = 1\n',
        r"step 1: .*`exposure`",
    ),
    "a look that is a word other than none": (
        '[[step]]\neach = "g"\nlooks = ["grade"]\n',
        r"step 1: look 1 is 'grade'.*none",
    ),
    "a look that is a number": ('[[step]]\neach = "g"\nlooks = [3]\n', r"step 1: .*look 1.*table"),
    "a look without an effect": (
        '[[step]]\neach = "g"\nlooks = ["none", { exposure = 1 }]\n',
        r"step 1: look 2: .*`effect`",
    ),
    "a look with a misspelt parameter": (
        '[[step]]\neach = "g"\nlooks = [{ effect = "grade", exposur = 1 }]\n',
        r"step 1: look 1: .*`exposur`",
    ),
    "a look with an opacity out of range": (
        '[[step]]\neach = "g"\nlooks = [{ effect = "grade", opacity = 2 }]\n',
        r"step 1: look 1: .*opacity.*2",
    ),
    "an assignment that does not exist": (
        '[[step]]\neach = "g"\nlooks = ["none"]\nassign = "shuffle"\n',
        r"step 1: .*`assign`.*cycle.*random",
    ),
    "an each that is no region": (
        '[[step]]\neach = "h"\nlooks = ["none"]\n',
        r"step 1: `each` names 'h'.*valid partitions: g",
    ),
    "a mask that is no region": (
        '[[step]]\neffect = "grade"\nmask = "h"\n',
        r"step 1: `mask` names 'h'.*valid selections: subject, background",
    ),
}


@pytest.mark.parametrize(("text", "complaint"), EACH_MISTAKES.values(), ids=EACH_MISTAKES.keys())
def test_a_mistake_in_an_each_step_names_the_file_the_step_and_the_problem(tmp_path, text, complaint):
    with pytest.raises(RecipeError, match=rf"^bad\.toml: .*{complaint}"):
        load_recipe(_write(tmp_path, "format = 2\n" + EACH + text, "bad.toml"))


@pytest.mark.parametrize(
    ("effect", "says"),
    [
        pytest.param("glitch", 'fx.slice_shift(preset="glitch")', id="known-good-a-preset-name"),
        pytest.param("vibrance", "'vibrance' is a parameter of grade", id="known-good-a-parameter-name"),
        pytest.param("depth_of_field", "named in the summary of focus_blur", id="known-good-a-word-of-a-summary"),
        pytest.param("crt", 'load_recipe("crt")', id="known-good-a-look-name"),
        pytest.param("zzqxv", "unknown effect 'zzqxv'", id="known-bad-nonsense"),
    ],
)
def test_a_step_naming_something_that_is_no_effect_says_where_the_name_lives(tmp_path, effect, says):
    path = _write(tmp_path, f"[[step]]\neffect = '{effect}'\n")
    with pytest.raises(UnknownEffectError, match=re.escape(says)) as raised:
        load_recipe(path)
    assert str(raised.value).startswith("look.toml: step 1")


# --- built-in looks, loaded by name ------------------------------------------------------------------------------


def test_a_look_loads_by_name_as_its_file_does():
    shipped = looks.read()["crt"]
    assert load_recipe("crt") == load_recipe(shipped.file)
    assert load_recipe("crt").name == "crt"


def test_a_str_that_is_a_look_name_is_the_look_even_beside_a_file_of_that_name(tmp_path, monkeypatch):
    monkeypatch.chdir(tmp_path)
    (tmp_path / "crt").write_text(STEP, encoding="utf-8")
    assert load_recipe("crt") == load_recipe(looks.read()["crt"].file)
    assert len(load_recipe("./crt").steps) == 1  # the file, spelt as a path


def test_a_path_is_always_a_file_never_a_look(tmp_path, monkeypatch):
    monkeypatch.chdir(tmp_path)
    with pytest.raises(FileNotFoundError):
        load_recipe(Path("crt"))


@pytest.mark.parametrize(
    ("name", "says"),
    [
        pytest.param("glich", r"no look or file 'glich'.*did you mean load_recipe\(\"glitch\"\)", id="near a look"),
        pytest.param("zzz", r"no look or file 'zzz'.*the looks: .*crt", id="near nothing: the looks listed"),
    ],
)
def test_a_name_that_is_no_look_and_no_file_suggests_the_nearest_looks(tmp_path, monkeypatch, name, says):
    monkeypatch.chdir(tmp_path)
    with pytest.raises(RecipeError, match=says):
        load_recipe(name)


@pytest.mark.parametrize(
    ("description", "ok"),
    [
        pytest.param("'A sentence.'", True, id="a-sentence"),
        pytest.param("3", False, id="a-number"),
        pytest.param("'  '", False, id="blank"),
    ],
)
def test_a_recipe_may_describe_itself_in_a_sentence(tmp_path, description, ok):
    path = _write(tmp_path, f"description = {description}\n{STEP}")
    if ok:
        assert load_recipe(path).name == "look"
    else:
        with pytest.raises(RecipeError, match=r"look\.toml.*description"):
            load_recipe(path)
