"""Reading a recipe file: anything wrong with the file itself (its TOML, its encoding, its format version, its top-level
keys) is a `RecipeError` that starts with the file's name."""

import re
from pathlib import Path

import numpy as np
import pytest
from PIL import Image

from bullterrier_image.core import registry
from bullterrier_image.core.framing import Framing
from bullterrier_image.core.steps import Each, Recipe, Step
from bullterrier_image.errors import ImageReadError, RecipeError
from bullterrier_image.loader import load_recipe
from bullterrier_image.recipe import Edge, Grid, Pick, Shards

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


# Without `name`, a recipe is named after its file: every run of characters a name may not hold becomes one `-`, and the
# ends are trimmed so that the name starts with a letter or digit (expected values worked out by hand).
@pytest.mark.parametrize(
    ("file", "name"),
    [
        ("my look.toml", "my-look"),
        ("  --Holiday  (2024)!!.toml", "Holiday-2024"),
        ("v1.2 final.toml", "v1.2-final"),
        ("rock - roll.toml", "rock-roll"),  # the `-` already there joins the two replaced spaces: one `-`
        ("café au lait.toml", "caf-au-lait"),
        ("_draft_.toml", "draft_"),
        ("plain.toml", "plain"),
        ("a" * 63 + " b.toml", "a" * 63),  # cut to 64 characters, then the dangling `-` trimmed
        ("b" * 80 + ".toml", "b" * 64),
    ],
)
def test_a_recipe_without_a_name_is_named_after_its_file_made_safe(tmp_path, file, name):
    assert load_recipe(_write(tmp_path, STEP, file)).name == name


@pytest.mark.parametrize("file", ["日本.toml", "---.toml", "con.toml", "LPT1 .toml"])
def test_a_file_name_that_leaves_no_usable_name_asks_for_a_name_or_a_new_file_name(tmp_path, file):
    with pytest.raises(RecipeError, match=rf"^{re.escape(file)}: .*name = .*rename"):
        load_recipe(_write(tmp_path, STEP, file))


def test_a_recipe_without_a_format_or_with_format_1_or_2_loads(tmp_path):
    assert load_recipe(_write(tmp_path, "name = 'a'\n" + STEP, "a.toml")).steps[0].runs[0].effect == "grade"
    assert load_recipe(_write(tmp_path, "format = 1\nname = 'b'\n" + STEP, "b.toml")).name == "b"
    assert load_recipe(_write(tmp_path, "format = 2\nname = 'c'\n" + STEP, "c.toml")).name == "c"


def test_a_format_1_file_means_the_same_in_format_2(tmp_path):
    body = "size = 64\n[frame]\naspect = 1\n" + STEP + "mask = 'subject'\nopacity = 0.5\n"
    assert (
        load_recipe(_write(tmp_path, "format = 1\n" + body, "one.toml")).steps
        == load_recipe(_write(tmp_path, "format = 2\n" + body, "two.toml")).steps
    )


@pytest.mark.parametrize(
    ("written", "value"), [("3", "3"), ("0", "0"), ("'1'", "'1'"), ("1.0", "1.0"), ("true", "True"), ("'2'", "'2'")]
)
def test_any_other_format_is_refused_naming_the_value_and_the_formats_this_release_reads(tmp_path, written, value):
    path = _write(tmp_path, f"format = {written}\n" + STEP, "future.toml")
    with pytest.raises(RecipeError, match=r"^future\.toml: .*`format`") as raised:
        load_recipe(path)
    message = str(raised.value)
    assert value in message
    assert re.search(r"reads\D*\b1\b.*\b2\b", message)  # the formats this release reads


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
    with pytest.raises(RecipeError, match=r"`frames`.*format, name, size, \[frame\], \[region\.NAME\], \[\[step\]\]"):
        load_recipe(_write(tmp_path, "[frames]\n" + STEP))


# --- presets in a recipe file ------------------------------------------------------------------------------------


def test_a_step_may_name_a_preset_and_its_own_keys_win_over_it(tmp_path):
    path = _write(tmp_path, "[[step]]\neffect = 'gradient_map'\npreset = 'sepia'\nshadow_clip = 0.2\nopacity = 0.5\n")
    (step,) = load_recipe(path).steps
    assert step == Step("gradient_map", {"shadow_clip": 0.2}, opacity=0.5, preset="sepia")
    assert "preset" not in step.params  # a step's key, not an effect's parameter


@pytest.mark.parametrize(
    ("effect", "preset", "complaint"),
    [
        pytest.param("gradient_map", "sepya", r"gradient_map.*'sepya'.*sepia", id="a preset the effect lacks"),
        pytest.param("grade", "sepia", r"grade.*'sepia'.*no presets", id="an effect without presets"),
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
    image = recipe.steps[1].runs[0].params["image"]
    assert (image.width, image.height, image.label) == (2, 1, "../assets/logo.png")
    np.testing.assert_allclose(image.pixels[0], np.array([[1, 0, 0, 1], [0, 0, 1, 128 / 255]]), atol=1e-6)


def test_an_absolute_picture_path_is_read_as_it_is(layer_probe, tmp_path):
    logo = _logo(tmp_path / "elsewhere")
    recipe = load_recipe(_layer(tmp_path / "recipes", f"'{logo}'"))
    assert recipe.steps[1].runs[0].params["image"].width == 2


@pytest.mark.parametrize(
    ("written", "pixel"),
    [("'#ff8000'", [1, 128 / 255, 0, 1]), ("[0.25, 0.5, 1.0]", [0.25, 0.5, 1, 1])],
    ids=["hex", "numbers"],
)
def test_a_picture_parameter_may_be_a_colour_and_no_file_is_read(layer_probe, tmp_path, written, pixel):
    step = load_recipe(_layer(tmp_path, written)).steps[1].runs[0]
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

[[step]]
effect = "fill"
color = "#ff0000"
mask = "head"

[[step]]
each = "glass"
looks = [
    { effect = "grade", exposure = 0.5, opacity = 0.7 },
    "none",
    { effect = "fill", color = "#00ff00", blend_mode = "multiply" },
]
assign = "random"
seed = 4
feather = 0.01
mask = "subject"
opacity = 0.9
"""

REGIONS_RECIPE = Recipe(
    "look",
    size=64,
    regions={
        "panes": Grid(columns=[0.2, 0.17, 0.63], rows=2),
        "glass": Shards(preset="glass", fit="subject", impact=(0.6, 0.2), edge=Edge(style="torn", amount=0.02)),
        "head": Pick(of="panes", pieces=[4]),
    },
    steps=[
        Step("fill", {"color": "#ff0000"}, mask="head"),
        Each(
            "glass",
            [
                Step("grade", {"exposure": 0.5}, opacity=0.7),
                "none",
                Step("fill", {"color": "#00ff00"}, blend_mode="multiply"),
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


def test_the_same_file_renders_what_the_python_recipe_renders(tmp_path, photo, matte):
    from bullterrier_image.core.runner import render

    loaded = load_recipe(_write(tmp_path, REGIONS_FILE))
    np.testing.assert_array_equal(render(photo, loaded, matte), render(photo, REGIONS_RECIPE, matte))


def test_a_look_may_hold_a_picture_given_as_a_path_relative_to_the_file(layer_probe, tmp_path):
    _logo(tmp_path / "assets")
    (tmp_path / "recipes").mkdir()
    path = _write(
        tmp_path / "recipes",
        'format = 2\n[region.g]\nkind = "grid"\n[[step]]\neach = "g"\n'
        "looks = [{ effect = 'layer_probe', image = '../assets/logo.png' }]\n",
    )
    (each,) = load_recipe(path).steps
    assert each.runs[0].params["image"].label == "../assets/logo.png"


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


REGION_MISTAKES = {
    "a kind that does not exist": ('[region.g]\nkind = "hexagons"\n', r"region 'g': .*'hexagons'.*grid, shards, pick"),
    "no kind": ("[region.g]\ncolumns = 2\n", r"region 'g': .*`kind`.*grid, shards, pick"),
    "a kind that is not text": ("[region.g]\nkind = 3\n", r"region 'g': .*`kind`.*grid, shards, pick"),
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
}


@pytest.mark.parametrize(("text", "complaint"), REGION_MISTAKES.values(), ids=REGION_MISTAKES.keys())
def test_a_mistake_in_a_region_names_the_file_the_region_and_the_problem(tmp_path, text, complaint):
    with pytest.raises(RecipeError, match=rf"^bad\.toml: .*{complaint}"):
        load_recipe(_write(tmp_path, "format = 2\n" + text + STEP, "bad.toml"))


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
