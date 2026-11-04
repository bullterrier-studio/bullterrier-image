"""The Rust core's checks of parameters and recipe files against Python's models and loader, which are the oracle.

The extension is the public contract here (`_core.check`, `_core.read_recipe`, `_core.vocabulary`); Python's own models
and loader say what is right. Parameters: for each row both accept it and give the same complete values, or both
refuse it. Files: `read_recipe` only reads (shape and format versions; the rules of a look are `Recipe.checked` in
Rust, tested there), so a Python-valid file must read into the steps Python loads, and a file Python refuses for its
shape or its format version must be refused by the core too.
"""

import json
from pathlib import Path
from typing import Any, get_args

import pytest

from bullterrier_image._native import _core  # noqa: PLC2701  architecture: the Rust core is compared with Python's models
from bullterrier_image.core import regions, registry
from bullterrier_image.core.framing import Framing
from bullterrier_image.core.presets import SHIPPED as SHIPPED_PRESETS
from bullterrier_image.core.registry import CATEGORIES, INPUTS, STEP_KEYS
from bullterrier_image.core.steps import Each, Recipe, Step
from bullterrier_image.core.values import Lut
from bullterrier_image.loader import load_recipe
from bullterrier_image.ops.composite import BlendMode

ROOT = Path(__file__).parents[2]

# --- parameters: Table::check against the Pydantic models ------------------------------------------------------------

# (effect, the values given, whether the parameters are valid): a known-good and a known-bad row for every rule
ROWS = [
    # numbers: bounds inclusive on both sides, whole numbers read as decimals, nothing else
    pytest.param("grade", {}, True, id="grade: defaults"),
    pytest.param(
        "grade",
        {
            "temperature": 0.5,
            "tint": -0.25,
            "exposure": 1.5,
            "contrast": 2,
            "pivot": 0.25,
            "saturation": 0.5,
            "vibrance": 0.75,
        },
        True,
        id="grade: distinct values, a whole number for a decimal",
    ),
    pytest.param("grade", {"exposure": -10}, True, id="grade: lowest exposure"),
    pytest.param("grade", {"exposure": 10.0}, True, id="grade: highest exposure"),
    pytest.param("grade", {"exposure": 10.01}, False, id="grade: above the highest"),
    pytest.param("grade", {"exposure": -10.5}, False, id="grade: below the lowest"),
    pytest.param("grade", {"contrast": "2"}, False, id="grade: text for a decimal"),
    pytest.param("grade", {"contrast": True}, False, id="grade: a bool for a decimal"),
    pytest.param("grade", {"contrast": float("nan")}, False, id="grade: NaN"),
    pytest.param("grade", {"contrast": float("inf")}, False, id="grade: infinity"),
    pytest.param("grade", {"pivot": None}, False, id="grade: None for a decimal"),
    pytest.param("grade", {"kelvin": 5000}, False, id="grade: a parameter the effect lacks"),
    # a cyclic angle takes any real and wraps it into its turn, but 360 and 0.1 stay as they are
    pytest.param("pattern", {"angle": 360}, True, id="pattern: 360 stays 360"),
    pytest.param("pattern", {"angle": 0.1}, True, id="pattern: 0.1 stays 0.1"),
    pytest.param("pattern", {"angle": 725}, True, id="pattern: past the turn wraps"),
    pytest.param("pattern", {"angle": -90}, True, id="pattern: below the turn wraps"),
    pytest.param("pattern", {"angle": -1e-20}, True, id="pattern: a hair below 0 wraps to 360"),
    pytest.param("pattern", {"angle": 1e9 + 0.5}, True, id="pattern: far past the turn wraps"),
    pytest.param("pattern", {"angle": "up"}, False, id="pattern: text for an angle"),
    pytest.param("pattern", {"angle": True}, False, id="pattern: a bool for an angle"),
    pytest.param("pattern", {"angle": float("inf")}, False, id="pattern: infinity for an angle"),
    # choices
    pytest.param("pattern", {"kind": "dots"}, True, id="pattern: a listed choice"),
    pytest.param("pattern", {"kind": "hexagons"}, False, id="pattern: a choice not listed"),
    pytest.param("pattern", {"kind": 2}, False, id="pattern: a number for a choice"),
    # decimals with bounds
    pytest.param("pattern", {"cell": 2}, True, id="pattern: lowest cell"),
    pytest.param("pattern", {"cell": 512}, True, id="pattern: highest cell"),
    pytest.param("pattern", {"cell": 1.99}, False, id="pattern: cell below the lowest"),
    pytest.param("pattern", {"cell": 513}, False, id="pattern: cell above the highest"),
    pytest.param("pattern", {"radius": 0.75}, True, id="pattern: radius at its highest"),
    pytest.param("pattern", {"radius": 0.76}, False, id="pattern: radius above it"),
    # colours
    pytest.param("pattern", {"foreground": "#abc"}, True, id="pattern: short hex"),
    pytest.param("pattern", {"foreground": " #ABCDEF "}, True, id="pattern: capital hex with spaces around"),
    pytest.param("pattern", {"foreground": [0, 0.5, 1]}, True, id="pattern: three numbers"),
    pytest.param("pattern", {"foreground": (0.25, 0.5, 0.75)}, True, id="pattern: a tuple of three numbers"),
    pytest.param("pattern", {"foreground": "#abcd"}, False, id="pattern: four hex digits"),
    pytest.param("pattern", {"foreground": "abcdef"}, False, id="pattern: no hash"),
    pytest.param("pattern", {"foreground": "#ggg"}, False, id="pattern: not hex"),
    pytest.param("pattern", {"foreground": [0, 0, 2]}, False, id="pattern: a channel above 1"),
    pytest.param("pattern", {"foreground": [0, 0]}, False, id="pattern: two channels"),
    pytest.param("pattern", {"foreground": [True, 0, 0]}, False, id="pattern: a bool channel"),
    pytest.param("pattern", {"background": 7}, False, id="pattern: a number for a colour"),
    # whole numbers and switches
    pytest.param("pixel_grid", {"cell": 3}, True, id="pixel_grid: lowest cell"),
    pytest.param(
        "pixel_grid", {"cell": 32, "keep_brightness": True, "gap": 0, "amount": 1}, True, id="pixel_grid: highest"
    ),
    pytest.param("pixel_grid", {"cell": 2}, False, id="pixel_grid: cell below the lowest"),
    pytest.param("pixel_grid", {"cell": 33}, False, id="pixel_grid: cell above the highest"),
    pytest.param("pixel_grid", {"cell": 4.0}, False, id="pixel_grid: a whole decimal for a whole number"),
    pytest.param("pixel_grid", {"cell": 4.5}, False, id="pixel_grid: a fraction for a whole number"),
    pytest.param("pixel_grid", {"cell": True}, False, id="pixel_grid: a bool for a whole number"),
    pytest.param("pixel_grid", {"keep_brightness": 1}, False, id="pixel_grid: a number for a switch"),
    pytest.param("pixel_grid", {"keep_brightness": "yes"}, False, id="pixel_grid: text for a switch"),
    # lists of colours: a floor on the length (gradient_map two, gradient one), each element read as a colour
    pytest.param("gradient_map", {"stops": ["#000000", "#ffffff"]}, True, id="gradient_map: the shortest list"),
    pytest.param(
        "gradient_map",
        {"stops": ["#abc", [0, 0.5, 1], (0.25, 0.5, 0.75), " #ABCDEF "]},
        True,
        id="gradient_map: every way of writing a colour in one list",
    ),
    pytest.param("gradient_map", {"stops": ["#000000"] * 40}, True, id="gradient_map: a long list"),
    pytest.param("gradient_map", {"stops": ["#000000"]}, False, id="gradient_map: one colour is too few"),
    pytest.param("gradient_map", {"stops": []}, False, id="gradient_map: an empty list"),
    pytest.param(
        "gradient_map", {"stops": ["#000000", "nope"]}, False, id="gradient_map: the second element is not a colour"
    ),
    pytest.param(
        "gradient_map", {"stops": ["#000000", [0, 0, 2]]}, False, id="gradient_map: a channel above 1 in element 1"
    ),
    pytest.param(
        "gradient_map", {"stops": ["#000", "#fff", [0, 0]]}, False, id="gradient_map: two channels in element 2"
    ),
    pytest.param("gradient_map", {"stops": "#000000"}, False, id="gradient_map: a colour for a list"),
    pytest.param("gradient_map", {"stops": None}, False, id="gradient_map: None for a list"),
    pytest.param("gradient", {"colors": ["#102030"]}, True, id="gradient: one colour is a flat fill"),
    pytest.param("gradient", {"colors": []}, False, id="gradient: an empty list"),
    pytest.param("gradient", {"colors": ["#102030", 7]}, False, id="gradient: a number for element 1"),
    # a choice of thirteen, and the whole number and switch of a generated picture
    pytest.param("gradient_map", {"quantile_method": "closest_observation"}, True, id="gradient_map: a listed method"),
    pytest.param("gradient_map", {"quantile_method": "quartile"}, False, id="gradient_map: a method not listed"),
    pytest.param("gradient", {"repeat": 64, "reflect": True}, True, id="gradient: highest repeat, reflected"),
    pytest.param("gradient", {"repeat": 65}, False, id="gradient: repeat above the highest"),
    pytest.param("gradient", {"repeat": 2.0}, False, id="gradient: a whole decimal for a repeat"),
    pytest.param("fractal", {"colors": ["#102030"]}, False, id="fractal: one colour is too few"),
    pytest.param("fractal", {"colors": ["#102030", "#405060"]}, True, id="fractal: two colours"),
    pytest.param("fractal", {"iterations": 2000, "zoom": 10000}, True, id="fractal: highest iterations and zoom"),
    pytest.param("fractal", {"iterations": 0}, False, id="fractal: iterations below the lowest"),
    pytest.param("fractal", {"c_real": -2.5}, False, id="fractal: constant below the lowest"),
    pytest.param("lens_flare", {"ghosts": 12, "spacing": 3.0, "amount": 10.0}, True, id="lens_flare: highest ghosts"),
    pytest.param("lens_flare", {"ghosts": 13}, False, id="lens_flare: ghosts above the highest"),
    pytest.param("lens_flare", {"ring_width": 0.5}, False, id="lens_flare: ring width below the lowest"),
    pytest.param("light_leak", {"blobs": 16, "seed": 2147483647}, True, id="light_leak: highest blobs and seed"),
    pytest.param("light_leak", {"blobs": 0}, False, id="light_leak: blobs below the lowest"),
    pytest.param("light_leak", {"seed": 2147483648}, False, id="light_leak: seed above the highest"),
    pytest.param("dehaze", {"patch": 201, "edge": "wrap"}, True, id="dehaze: widest patch, wrap"),
    pytest.param("dehaze", {"patch": 202}, False, id="dehaze: patch above the widest"),
    pytest.param("dehaze", {"patch": 0}, False, id="dehaze: patch below the narrowest"),
    pytest.param("dehaze", {"edge": "constant"}, False, id="dehaze: constant is not one of its four edge rules"),
    pytest.param(
        "tone_map", {"edge_threshold": 0.01, "radius": 500, "pivot": 8}, True, id="tone_map: lowest threshold"
    ),
    pytest.param("tone_map", {"edge_threshold": 0.009}, False, id="tone_map: threshold below the lowest"),
    pytest.param(
        "motion_blur", {"angle": 360, "length": 500, "edge": "wrap"}, True, id="motion_blur: highest angle and length"
    ),
    pytest.param("motion_blur", {"edge": "constant"}, False, id="motion_blur: constant is not one of its four edges"),
    pytest.param("motion_blur", {"length": 501}, False, id="motion_blur: length above the highest"),
    pytest.param(
        "lens_blur", {"radius": 100, "highlights": 50, "edge": "mirror"}, True, id="lens_blur: highest settings"
    ),
    pytest.param("lens_blur", {"radius": 100.5}, False, id="lens_blur: radius above the highest"),
    pytest.param("lens_blur", {"edge": "constant"}, False, id="lens_blur: constant is not one of its four edges"),
    # a lookup table is a name to the core and a Lut to Python (the other rules of `file` are tested where each lives)
    pytest.param("lut", {}, True, id="lut: defaults"),
    pytest.param("lut", {"amount": 0}, True, id="lut: lowest amount"),
    pytest.param("lut", {"amount": 1}, True, id="lut: highest amount, a whole number for a decimal"),
    pytest.param("lut", {"amount": 1.01}, False, id="lut: amount above the highest"),
    pytest.param("lut", {"amount": -0.01}, False, id="lut: amount below the lowest"),
    pytest.param("lut", {"strength": 1}, False, id="lut: a parameter the effect lacks"),
]


def python_answer(effect: str, given: dict) -> dict | None:
    """The complete parameters Python's model makes of `given`, or None when it refuses them."""
    try:
        values = registry.get(effect).validate(given).model_dump()
    except ValueError:  # RecipeError, which is one
        return None
    # the identity table Python's model defaults to is the `null` the core writes
    return {key: None if value == Lut.identity() else value for key, value in values.items()}


def core_answer(effect: str, given: dict) -> dict | None:
    try:
        return _core.check(effect, given)
    except ValueError:
        return None


def written(values: dict) -> str:
    """The values as JSON text, which tells `3` from `3.0` and a list from a tuple of the same numbers."""
    return json.dumps(values, sort_keys=True)


@pytest.mark.parametrize(("effect", "given", "valid"), ROWS)
def test_the_core_and_python_accept_and_refuse_the_same_parameters_and_make_the_same_of_them(effect, given, valid):
    python, core = python_answer(effect, given), core_answer(effect, given)
    assert (python is not None) == valid, f"Python's model, {python}"
    assert (core is not None) == valid, f"the core, {core}"
    if python is not None and core is not None:
        assert written(core) == written(python)


# --- files: read_recipe against load_recipe -----------------------------------------------------------------------

SHIPPED = sorted((ROOT / "src" / "bullterrier_image" / "looks").glob("*.toml"))
EXAMPLES = sorted(path for path in (ROOT / "examples").rglob("*.toml") if path.name != "bt-image.toml")
FILES = [pytest.param(path, id=str(path.relative_to(ROOT))) for path in [*SHIPPED, *EXAMPLES]]


def python_step(step: Step | Each) -> dict:
    """What `read_recipe` says of a step, from the step Python loaded: its opacity resolved as Python resolves it."""
    if isinstance(step, Each):
        return {
            "each": step.each,
            "looks": [None if look is None else python_step(look) for look in step.looks],
            "assign": step.assign,
            "seed": step.seed,
            "feather": step.feather,
            "mask": step.mask,
            "opacity": step.opacity,
            "blend_mode": step.blend_mode,
            "name": step.name,
        }
    return {
        "effect": step.effect,
        "mask": step.mask,
        "opacity": step.opacity,
        "blend_mode": step.blend_mode,
        "partition": step.partition,
        "name": step.name,
    }


def core_step(entry: dict) -> dict:
    """The same shape from the core's entry: an opacity left unset is the effect's own default (Rust keeps None until
    the runner resolves it), 1 for an each step."""
    if "each" in entry:
        return {
            "each": entry["each"],
            "looks": [None if look is None else core_step(look) for look in entry["looks"]],
            "assign": entry.get("assign", "cycle"),
            "seed": entry.get("seed", 0),
            "feather": entry.get("feather", 0.5),
            "mask": entry.get("mask"),
            "opacity": 1.0 if entry.get("opacity") is None else entry["opacity"],
            "blend_mode": entry.get("blend_mode", "normal"),
            "name": entry.get("name"),
        }
    opacity = registry.opacity(entry["effect"]) if entry.get("opacity") is None else entry["opacity"]
    return {
        "effect": entry["effect"],
        "mask": entry.get("mask"),
        "opacity": opacity,
        "blend_mode": entry.get("blend_mode", "normal"),
        "partition": entry.get("partition"),
        "name": entry.get("name"),
    }


@pytest.mark.parametrize("path", FILES)
def test_a_shipped_look_or_example_reads_into_the_steps_python_loads(path):
    loaded = load_recipe(path)
    read = _core.read_recipe(path.read_text(encoding="utf-8"), "toml")
    assert [core_step(entry) for entry in read["steps"]] == [python_step(step) for step in loaded.steps]
    assert read.get("name", path.stem) == loaded.name
    assert read.get("description") == loaded.description
    assert read.get("size") == loaded.size
    assert list(read.get("regions", {})) == list(loaded.regions)  # in the order the file declares them
    assert bool(read.get("frame")) == (loaded.frame != Framing())


def test_the_regions_of_a_file_are_read_in_the_order_it_declares_them(tmp_path):
    text = "format = 2\n[region.zeta]\nkind = 'grid'\n[region.alpha]\nkind = 'grid'\n" + STEP
    path = tmp_path / "order.toml"
    path.write_text(text, encoding="utf-8")
    assert list(_core.read_recipe(text, "toml")["regions"]) == list(load_recipe(path).regions) == ["zeta", "alpha"]


def test_a_python_dict_keeps_its_order_into_the_core_and_back():
    given: dict[str, Any] = {
        "steps": [{"effect": "grade", "params": {"zebra": 1, "apple": 2}}],
        "regions": {"zeta": {"kind": "given"}, "alpha": {"kind": "given"}},
    }
    read = _core.read_recipe(json.dumps(given), "json")
    assert list(read["regions"]) == ["zeta", "alpha"]
    assert list(read["steps"][0]["params"]) == ["zebra", "apple"]
    given["steps"][0]["mask"] = "nowhere"
    with pytest.raises(ValueError, match=r"valid selections: subject, background, depth, light, zeta, alpha"):
        _core.check_recipe(given)


def test_there_are_looks_and_examples_to_compare():
    assert SHIPPED
    assert EXAMPLES


STEP = "[[step]]\neffect = 'grade'\n"
DRIVE = "radius = { by = 'depth', start = 0, end = 9 }\n"
FORMAT_2_EACH = "format = 2\n[region.g]\nkind = 'grid'\n[[step]]\neach = 'g'\n"

# files Python refuses for their shape or format version: the core must too (the file's own TOML, keys, format,
# and what a later format added). The rules of a look (an unknown effect, a value out of range) are not read here.
BAD_FILES = {
    "a TOML syntax error": "name = 'ok'\nsize = 64\ncolor = '#fff\n" + STEP,
    "a misspelt size": "sise = 2\n" + STEP,
    "a misspelt format": "formt = 2\n" + STEP,
    "steps for step": "steps = 2\n" + STEP,
    "an [animation] table": "[animation]\nseconds = 2\n" + STEP,
    "a single [step] table": "[step]\neffect = 'grade'\n",
    "a region without a kind": "format = 3\n[region.g]\ncolumns = 2\n" + STEP,
    "a region of an unknown kind": "format = 3\n[region.g]\nkind = 'gird'\n" + STEP,
    "a region with a field of another kind": "format = 3\n[region.g]\nkind = 'grid'\ncount = 2\n" + STEP,
    "a region field of the wrong type": "format = 3\n[region.g]\nkind = 'band'\nlow = 'high'\n" + STEP,
    "a size of -5": "size = -5\n" + STEP,
    "a size as text": "size = '64'\n" + STEP,
    "a size as a decimal": "size = 64.0\n" + STEP,
    "a size as a switch": "size = true\n" + STEP,
    "a format of 4": "format = 4\n" + STEP,
    "a format of 0": "format = 0\n" + STEP,
    "a format as text": "format = '1'\n" + STEP,
    "a format as a decimal": "format = 1.0\n" + STEP,
    "a format as a switch": "format = true\n" + STEP,
    "a blank description": "description = '  '\n" + STEP,
    "a description that is no text": "description = 3\n" + STEP,
    "a region in the first format": "[region.g]\nkind = 'grid'\n" + STEP,
    "a region in format 1": "format = 1\n[region.g]\nkind = 'grid'\n" + STEP,
    "an each step in the first format": "[[step]]\neach = 'subject'\nlooks = ['none']\n",
    "a partition in format 1": "format = 1\n[[step]]\neffect = 'grade'\npartition = 'subject'\n",
    "a step name in the first format": STEP + "name = 'warm'\n",
    "a step name in format 1": "format = 1\n" + STEP + "name = 'warm'\n",
    "a step name in format 2": "format = 2\n" + STEP + "name = 'warm'\n",
    "a drive in the first format": "[[step]]\neffect = 'bloom'\n" + DRIVE,
    "a drive in format 2": "format = 2\n[[step]]\neffect = 'bloom'\n" + DRIVE,
    "a drive in a look of format 2": (
        "format = 2\n[region.g]\nkind = 'grid'\n[[step]]\neach = 'g'\nlooks = ['none', { effect = 'bloom', "
        + DRIVE.strip()
        + " }]\n"
    ),
    "a band region in format 2": "format = 2\n[region.n]\nkind = 'band'\nof = 'depth'\n" + STEP,
    "a bands region in format 2": "format = 2\n[region.b]\nkind = 'bands'\nof = 'depth'\n" + STEP,
    "a combine region in format 2": "format = 2\n[region.c]\nkind = 'combine'\nof = ['depth', 'subject']\n" + STEP,
    "a linear gradient in format 2": "format = 2\n[region.g]\nkind = 'linear_gradient'\n" + STEP,
    "a radial gradient in format 2": "format = 2\n[region.g]\nkind = 'radial_gradient'\n" + STEP,
    "a given region in format 2": "format = 2\n[region.g]\nkind = 'given'\n" + STEP,
    "the replaced kind depth": "format = 3\n[region.n]\nkind = 'depth'\nnear = 0.9\nfar = 0.6\n" + STEP,
    "the replaced kind depth_bands": "format = 3\n[region.b]\nkind = 'depth_bands'\ncount = 4\n" + STEP,
    "looks that are not a list": FORMAT_2_EACH + "looks = 'none'\n",
    "an each step without looks": FORMAT_2_EACH,
    "looks without each": "format = 2\n[[step]]\nlooks = ['none']\n",
    "an effect beside each": FORMAT_2_EACH + "looks = ['none']\neffect = 'grade'\n",
    "an effect parameter on the each step": FORMAT_2_EACH + "looks = ['none']\nexposure = 1\n",
    "a look that is a word other than none": FORMAT_2_EACH + "looks = ['grade']\n",
    "a look that is a number": FORMAT_2_EACH + "looks = [3]\n",
    "a look without an effect": FORMAT_2_EACH + "looks = ['none', { exposure = 1 }]\n",
    "an assignment that does not exist": FORMAT_2_EACH + "looks = ['none']\nassign = 'shuffle'\n",
    "a step without an effect": "[[step]]\nmask = 'subject'\n",
    "a step opacity of nan": STEP + "opacity = nan\n",
    "a step opacity of infinity": STEP + "opacity = inf\n",
    "a parameter of infinity": STEP + "exposure = -inf\n",
    "a parameter that is a date": STEP + "exposure = 1979-05-27\n",
    "a parameter that is a date and time": STEP + "exposure = 1979-05-27T07:32:00Z\n",
}

GOOD_FILES = {
    "no format": STEP,
    "format 1": "format = 1\n" + STEP,
    "format 2 with an each step and a none look": FORMAT_2_EACH + "looks = [{ effect = 'grade' }, 'none']\n",
    "format 3 with a step name": "format = 3\n" + STEP + "name = 'warm'\n",
    "format 3 with a drive": "format = 3\n[[step]]\neffect = 'bloom'\n" + DRIVE,
    "format 3 with a band region": "format = 3\n[region.n]\nkind = 'band'\nof = 'depth'\n" + STEP,
    "a size": "size = 64\n" + STEP,
}


@pytest.mark.parametrize("text", BAD_FILES.values(), ids=BAD_FILES.keys())
def test_a_file_python_refuses_for_its_shape_or_format_version_is_refused_by_the_core_too(tmp_path, text):
    path = tmp_path / "bad.toml"
    path.write_text(text, encoding="utf-8")
    with pytest.raises(ValueError, match=r"\w"):
        load_recipe(path)
    with pytest.raises(ValueError, match=r"\w"):
        _core.read_recipe(text, "toml")


@pytest.mark.parametrize("text", GOOD_FILES.values(), ids=GOOD_FILES.keys())
def test_a_file_python_loads_is_read_by_the_core_into_the_same_steps(tmp_path, text):
    path = tmp_path / "good.toml"
    path.write_text(text, encoding="utf-8")
    loaded = load_recipe(path)
    read = _core.read_recipe(text, "toml")
    assert [core_step(entry) for entry in read["steps"]] == [python_step(step) for step in loaded.steps]


# --- the frame: Rust's `Frame` against Python's `Framing` -----------------------------------------------------------

# (the frame as a recipe writes it, whether it is valid): the bounds of every number on both sides, every choice, the
# colours, and what the shape refuses. The `valid` column is written by hand; Python's `Framing` and the core must both
# agree with it.
FRAME_ROWS = [
    pytest.param({}, True, id="no field"),
    pytest.param({"flip": "both", "angle": 30.5, "aspect": 1.5, "anchor": "top", "pad_left": 0.5}, True, id="distinct"),
    *[pytest.param({"flip": choice}, True, id=f"flip {choice}") for choice in ("none", "x", "y", "both")],
    pytest.param({"flip": "z"}, False, id="flip not listed"),
    pytest.param({"flip": 1}, False, id="flip a number"),
    *[pytest.param({"fit": choice}, True, id=f"fit {choice}") for choice in ("crop", "expand")],
    pytest.param({"fit": "stretch"}, False, id="fit not listed"),
    *[pytest.param({"resample": c}, True, id=f"resample {c}") for c in ("nearest", "bilinear", "bicubic")],
    pytest.param({"resample": "lanczos"}, False, id="resample not listed"),
    *[
        pytest.param({"anchor": c}, True, id=f"anchor {c}")
        for c in ("center", "top", "bottom", "left", "right", "subject")
    ],
    pytest.param({"anchor": "middle"}, False, id="anchor not listed"),
    *[
        pytest.param({"transparent": c}, True, id=f"transparent {c}")
        for c in ("none", "background", "circle", "rounded")
    ],
    pytest.param({"transparent": "oval"}, False, id="transparent not listed"),
    # an angle is any real: it wraps into -180..180
    pytest.param({"angle": 270}, True, id="angle past the turn"),
    pytest.param({"angle": -1e9}, True, id="angle far below"),
    pytest.param({"angle": "up"}, False, id="angle text"),
    pytest.param({"angle": True}, False, id="angle a bool"),
    pytest.param({"aspect": 0.1}, True, id="aspect lowest"),
    pytest.param({"aspect": 10}, True, id="aspect highest"),
    pytest.param({"aspect": None}, True, id="aspect none"),
    pytest.param({"aspect": 0.09}, False, id="aspect below"),
    pytest.param({"aspect": 10.5}, False, id="aspect above"),
    pytest.param({"aspect": "wide"}, False, id="aspect text"),
    pytest.param({"margin": 0}, True, id="margin lowest"),
    pytest.param({"margin": 1}, True, id="margin highest"),
    pytest.param({"margin": -0.25}, False, id="margin below"),
    pytest.param({"margin": 1.25}, False, id="margin above"),
    pytest.param({"margin": True}, False, id="margin a bool"),
    *[
        pytest.param({side: value}, valid, id=f"{side} {value}")
        for side in ("pad_top", "pad_right", "pad_bottom", "pad_left")
        for value, valid in ((0, True), (2, True), (-0.5, False), (2.5, False))
    ],
    pytest.param({"corner_radius": 0}, True, id="corner_radius lowest"),
    pytest.param({"corner_radius": 0.5}, True, id="corner_radius highest"),
    pytest.param({"corner_radius": -0.25}, False, id="corner_radius below"),
    pytest.param({"corner_radius": 0.75}, False, id="corner_radius above"),
    pytest.param({"fill": "#abc"}, True, id="fill short hex"),
    pytest.param({"fill": " #ABCDEF "}, True, id="fill capital hex with spaces"),
    pytest.param({"fill": [0, 0.5, 1]}, True, id="fill three numbers"),
    pytest.param({"fill": "#abcd"}, False, id="fill four hex digits"),
    pytest.param({"fill": [0, 0, 2]}, False, id="fill a channel above 1"),
    pytest.param({"fill": [0, 0]}, False, id="fill two channels"),
    pytest.param({"fill": 7}, False, id="fill a number"),
    pytest.param({"zoom": 2}, False, id="a field the frame lacks"),
]


def python_accepts_frame(frame: dict) -> bool:
    try:
        Framing(**frame)
    except ValueError:  # RecipeError, which is one
        return False
    return True


def core_accepts_frame(frame: dict) -> bool:
    """Whether the core's check of a recipe takes `frame` (it does not run it)."""
    try:
        _core.check_recipe({"steps": [{"effect": "grade"}], "frame": frame})
    except ValueError:  # CoreError, which is one
        return False
    return True


@pytest.mark.parametrize(("frame", "valid"), FRAME_ROWS)
def test_the_core_and_python_accept_and_refuse_the_same_frames(frame, valid):
    assert python_accepts_frame(frame) == valid, "Python's Framing"
    assert core_accepts_frame(frame) == valid, "the core"


def core_frame(frame: dict) -> dict:
    """The frame as the core reads and writes it (every field), from a recipe file."""
    return _core.read_recipe(json.dumps({"steps": [{"effect": "grade"}], "frame": frame}), "json").get("frame", {})


def python_frame(frame: dict) -> dict:
    """The same frame as Python's model dumps it, a colour as a list, like JSON."""
    return {**Framing(**frame).model_dump(), "fill": list(Framing(**frame).fill)}


ALL_FIELDS = {
    "flip": "y",
    "angle": -30.5,
    "fit": "expand",
    "resample": "nearest",
    "aspect": 1.5,
    "anchor": "subject",
    "margin": 0.25,
    "pad_top": 0.5,
    "pad_right": 0.75,
    "pad_bottom": 1.0,
    "pad_left": 1.5,
    "fill": [0.25, 0.5, 0.75],
    "transparent": "rounded",
    "corner_radius": 0.4,
}


@pytest.mark.parametrize(
    "frame",
    [
        pytest.param(ALL_FIELDS, id="every field set to its own value"),
        pytest.param({"aspect": 1}, id="one field set, the others at their defaults"),
        pytest.param({"fill": "#a12"}, id="a colour written as short hex"),
    ],
)
def test_the_core_names_the_fields_of_a_frame_and_defaults_them_as_python_does(frame):
    core, python = core_frame(frame), python_frame(frame)
    assert set(core) == set(Framing.model_fields)
    assert {name: value for name, value in core.items() if name != "fill"} == {
        name: value for name, value in python.items() if name != "fill"
    }
    assert core["fill"] == pytest.approx(python["fill"], abs=1e-6)


def test_the_core_does_not_write_the_default_frame():
    assert "frame" not in _core.read_recipe(json.dumps({"steps": [{"effect": "grade"}], "frame": {}}), "json")


def test_the_core_names_the_vocabulary_of_recipes_in_the_order_the_package_lists_it():
    vocabulary = _core.vocabulary()
    assert vocabulary["categories"] == list(CATEGORIES)
    assert vocabulary["inputs"] == list(INPUTS)
    assert vocabulary["step_keys"] == list(STEP_KEYS)
    assert vocabulary["blend_modes"] == list(get_args(BlendMode))


# --- regions: Rust's `Region` against Python's declarations -----------------------------------------------------------

PYTHON_KINDS: dict[str, Any] = {
    "grid": regions.Grid,
    "shards": regions.Shards,
    "cuts": regions.Cuts,
    "path": regions.Path,
    "svg": regions.Svg,
    "pick": regions.Pick,
    "subject": regions.Subject,
    "band": regions.Band,
    "bands": regions.Bands,
    "combine": regions.Combine,
    "linear_gradient": regions.LinearGradient,
    "radial_gradient": regions.RadialGradient,
    "given": regions.Given,
}
CUT = {"x": 0.5, "y": 0.5, "angle": 0}


def edge(**fields: object) -> dict:
    return {"edge": fields}


# (what the row is about, the kind, the fields as a recipe writes them, whether they are valid): both ends of every
# range and the first value beyond each, and the shapes each kind refuses. The `valid` column is written by hand;
# Python's declaration and the core must both agree with it. Left out because Python's declaration is laxer
# than the rule and is replaced (R4e): a share written as text ("2") and a piece that is a switch (`True` is 1 to it);
# the core refuses both (Rust tests). `Svg.file` is read when the region is made, so any text passes the check.
REGION_ROWS = [
    # the borders every laid kind has (a grid stands for them)
    ("edge amount, lowest", "grid", edge(amount=0), True),
    ("edge amount, highest", "grid", edge(amount=48), True),
    ("edge amount, below", "grid", edge(amount=-0.1), False),
    ("edge amount, above", "grid", edge(amount=48.5), False),
    ("edge scale, lowest", "grid", edge(scale=1), True),
    ("edge scale, highest", "grid", edge(scale=480), True),
    ("edge scale, below", "grid", edge(scale=0.5), False),
    ("edge scale, above", "grid", edge(scale=481), False),
    ("edge octaves, lowest", "grid", edge(octaves=1), True),
    ("edge octaves, highest", "grid", edge(octaves=8), True),
    ("edge octaves, below", "grid", edge(octaves=0), False),
    ("edge octaves, above", "grid", edge(octaves=9), False),
    ("edge persistence, lowest", "grid", edge(persistence=0), True),
    ("edge persistence, highest", "grid", edge(persistence=1), True),
    ("edge persistence, below", "grid", edge(persistence=-0.25), False),
    ("edge persistence, above", "grid", edge(persistence=1.25), False),
    ("edge seed, highest", "grid", edge(seed=2**32 - 1), True),
    ("edge seed, above", "grid", edge(seed=2**32), False),
    ("edge seed, below", "grid", edge(seed=-1), False),
    ("edge style torn", "grid", edge(style="torn"), True),
    ("edge style unknown", "grid", edge(style="wavy"), False),
    ("edge field unknown", "grid", edge(width=3), False),
    ("edge not a table", "grid", {"edge": "torn"}, False),
    ("edge on shards", "shards", edge(amount=49), False),
    ("edge on cuts", "cuts", edge(amount=49), False),
    ("edge on path", "path", edge(amount=49), False),
    ("edge on svg", "svg", edge(amount=49), False),
    ("fit a name", "grid", {"fit": "subject"}, True),
    ("fit a number", "grid", {"fit": 3}, False),
    # grid
    ("a count of columns", "grid", {"columns": 3, "rows": 2}, True),
    ("1000 columns", "grid", {"columns": 1000}, True),
    ("1001 columns", "grid", {"columns": 1001}, False),
    ("no column", "grid", {"columns": 0}, False),
    ("a negative count", "grid", {"rows": -2}, False),
    ("shares", "grid", {"columns": [1, 2.5, 1], "rows": [0.001]}, True),
    ("1000 shares", "grid", {"rows": [1] * 1000}, True),
    ("1001 shares", "grid", {"rows": [1] * 1001}, False),
    ("no shares", "grid", {"columns": []}, False),
    ("a share of 0", "grid", {"columns": [1, 0]}, False),
    ("a negative share", "grid", {"rows": [2, -1]}, False),
    ("columns as text", "grid", {"columns": "3"}, False),
    ("a decimal count", "grid", {"columns": 2.5}, False),
    ("columns as a switch", "grid", {"columns": True}, False),
    # shards
    ("a preset", "shards", {"preset": "glass"}, True),
    ("a preset and a field given beside it", "shards", {"preset": "torn", "count": 3, "edge": {"amount": 1}}, True),
    ("a preset that is no preset", "shards", {"preset": "shattered"}, False),
    ("a preset's values are checked with the given ones", "shards", {"preset": "glass", "count": 501}, False),
    ("one shard", "shards", {"count": 1}, True),
    ("500 shards", "shards", {"count": 500}, True),
    ("no shard", "shards", {"count": 0}, False),
    ("501 shards", "shards", {"count": 501}, False),
    ("a count as a decimal", "shards", {"count": 12.0}, False),
    ("jitter, lowest", "shards", {"jitter": 0}, True),
    ("jitter, highest", "shards", {"jitter": 1}, True),
    ("jitter, below", "shards", {"jitter": -0.5}, False),
    ("jitter, above", "shards", {"jitter": 1.5}, False),
    ("impact, the corners", "shards", {"impact": [0, 1]}, True),
    ("impact x below", "shards", {"impact": [-0.5, 0.5]}, False),
    ("impact x above", "shards", {"impact": [1.5, 0.5]}, False),
    ("impact y below", "shards", {"impact": [0.5, -0.5]}, False),
    ("impact y above", "shards", {"impact": [0.5, 1.5]}, False),
    ("impact of three", "shards", {"impact": [0, 0, 0]}, False),
    ("impact of one", "shards", {"impact": [0]}, False),
    ("focus, lowest", "shards", {"focus": 0}, True),
    ("focus, highest", "shards", {"focus": 5}, True),
    ("focus, below", "shards", {"focus": -0.5}, False),
    ("focus, above", "shards", {"focus": 5.5}, False),
    ("seed, highest", "shards", {"seed": 2**32 - 1}, True),
    ("seed, above", "shards", {"seed": 2**32}, False),
    ("seed, below", "shards", {"seed": -1}, False),
    # cuts
    ("a line and a curve", "cuts", {"lines": [{"x": 0.25, "y": 0.75, "angle": 45}, {"path": "M 0 0 L 1 1"}]}, True),
    ("32 cuts", "cuts", {"lines": [CUT] * 32}, True),
    ("33 cuts", "cuts", {"lines": [CUT] * 33}, False),
    ("no cut", "cuts", {"lines": []}, False),
    ("a line at the corners", "cuts", {"lines": [{"x": 0, "y": 1}, {"x": 1, "y": 0}]}, True),
    ("a line, x below", "cuts", {"lines": [{"x": -0.5}]}, False),
    ("a line, x above", "cuts", {"lines": [{"x": 1.5}]}, False),
    ("a line, y below", "cuts", {"lines": [{"y": -0.5}]}, False),
    ("a line, y above", "cuts", {"lines": [{"y": 1.5}]}, False),
    ("a line past the turn", "cuts", {"lines": [{"angle": 725}, {"angle": -90}]}, True),
    ("a line with a path", "cuts", {"lines": [{"x": 0.5, "path": "M 0 0 L 1 1"}]}, False),
    ("a curve with an angle", "cuts", {"lines": [{"path": "M 0 0 L 1 1", "angle": 4}]}, False),
    ("a cut that is not a table", "cuts", {"lines": [3]}, False),
    ("a curve of two subpaths", "cuts", {"lines": [{"path": "M 0 0 L 1 1 M 0 1 L 1 0"}]}, False),
    ("a curve that is no path data", "cuts", {"lines": [{"path": "M 0 0 L foo"}]}, False),
    (
        "a curve with arcs and Beziers",
        "cuts",
        {"lines": [{"path": "M 0.4 0 C 0.1 0.4 0.9 0.6 0.6 1 A 1 1 0 0 1 0.6 1"}]},
        True,
    ),
    # path, svg
    ("samples, lowest", "path", {"samples": 1}, True),
    ("samples, highest", "path", {"samples": 16}, True),
    ("samples, below", "path", {"samples": 0}, False),
    ("samples, above", "path", {"samples": 17}, False),
    ("a path of its own", "path", {"d": "M 0.1 0.4 C 0.1 0.2, 0.4 0.2, 0.4 0.4 Z"}, True),
    ("a path of relative commands", "path", {"d": "m.1.1l.4 0v.4h-.4z"}, True),
    ("a path of two subpaths", "path", {"d": "M 0 0 L 1 0 L 1 1 Z M 0.2 0.2 L 0.4 0.2 L 0.4 0.4 Z"}, True),
    ("a path of arcs", "path", {"d": "M 0.2 0.5 A 0.3 0.3 0 1 0 0.8 0.5 A 0.3 0.3 0 1 0 0.2 0.5 Z"}, True),
    ("a path of nothing", "path", {"d": ""}, False),
    ("a path of blanks", "path", {"d": "  "}, False),
    ("a path that does not start with a move", "path", {"d": "L 0.5 0.5"}, False),
    ("a path with a stray word", "path", {"d": "M 0 0 L foo"}, False),
    ("a path with an unknown command", "path", {"d": "M 0 0 X 1 1"}, False),
    ("a path with a missing coordinate", "path", {"d": "M 0 0 L 1"}, False),
    ("a path with an infinite number", "path", {"d": "M 0 0 L 1e999 0"}, False),
    ("a path with a bad arc flag", "path", {"d": "M 0 0 A 1 1 0 2 0 1 1"}, False),
    ("a path ending in a move", "path", {"d": "M 0 0 L 0.5 0.5 Z M"}, False),
    ("a path far beyond the frame", "path", {"d": "M 0 0 C 0 0 1e9 1e9 0 0"}, False),
    ("a path that is no text", "path", {"d": 3}, False),
    ("a curve of relative commands", "cuts", {"lines": [{"path": "m.5 0 l0 1"}]}, True),
    ("a curve of nothing", "cuts", {"lines": [{"path": ""}]}, False),
    ("a curve that does not start with a move", "cuts", {"lines": [{"path": "L 1 1"}]}, False),
    ("a curve far beyond the frame", "cuts", {"lines": [{"path": "M 0 0 C 0 0 1e9 1e9 1 1"}]}, False),
    ("an svg file not named yet", "svg", {"file": ""}, True),
    ("an svg file that is no text", "svg", {"file": 3}, False),
    ("an svg file", "svg", {"file": "shapes.svg", "max_shapes": 1, "max_kilobytes": 100000}, True),
    ("max shapes, highest", "svg", {"max_shapes": 100000}, True),
    ("max shapes, below", "svg", {"max_shapes": 0}, False),
    ("max shapes, above", "svg", {"max_shapes": 100001}, False),
    ("max kilobytes, lowest", "svg", {"max_kilobytes": 1}, True),
    ("max kilobytes, below", "svg", {"max_kilobytes": 0}, False),
    ("max kilobytes, above", "svg", {"max_kilobytes": 100001}, False),
    # pick
    ("pieces by number", "pick", {"of": "p", "pieces": [0, 2, 3]}, True),
    ("no piece", "pick", {"of": "p", "pieces": []}, False),
    ("a negative piece", "pick", {"of": "p", "pieces": [1, -1]}, False),
    ("a decimal piece", "pick", {"of": "p", "pieces": [1.5]}, False),
    ("pieces as a number", "pick", {"of": "p", "pieces": 3}, False),
    # subject
    ("subject per piece", "subject", {"per": "p", "context": 0, "threshold": 1}, True),
    ("context, highest", "subject", {"context": 1, "threshold": 0}, True),
    ("context, below", "subject", {"context": -0.5}, False),
    ("context, above", "subject", {"context": 1.5}, False),
    ("threshold, below", "subject", {"threshold": -0.5}, False),
    ("threshold, above", "subject", {"threshold": 1.5}, False),
    ("per a number", "subject", {"per": 3}, False),
    # band
    ("a whole scale", "band", {"of": "depth", "low": 0, "high": 1, "feather": 0}, True),
    ("a single value", "band", {"of": "depth", "low": 0.5, "high": 0.5}, True),
    ("feather, highest", "band", {"of": "depth", "feather": 1}, True),
    ("low, below", "band", {"of": "depth", "low": -0.5}, False),
    ("low, above", "band", {"of": "depth", "low": 1.5, "high": 1}, False),
    ("high, below", "band", {"of": "depth", "low": 0, "high": -0.5}, False),
    ("high, above", "band", {"of": "depth", "high": 1.5}, False),
    ("feather, below", "band", {"of": "depth", "feather": -0.5}, False),
    ("feather, above", "band", {"of": "depth", "feather": 1.5}, False),
    ("low above high", "band", {"of": "depth", "low": 0.75, "high": 0.5}, False),
    ("low a hair above high", "band", {"of": "depth", "low": 0.5, "high": 0.4999}, False),
    # bands, combine
    ("1 band", "bands", {"of": "depth", "count": 1}, True),
    ("100 bands", "bands", {"of": "depth", "count": 100}, True),
    ("no band", "bands", {"of": "depth", "count": 0}, False),
    ("101 bands", "bands", {"of": "depth", "count": 101}, False),
    ("combine of two", "combine", {"of": ["subject", "depth"]}, True),
    ("combine of three, max", "combine", {"op": "max", "of": ["subject", "depth", "light"]}, True),
    ("combine of one", "combine", {"of": ["subject"]}, False),
    ("combine of none", "combine", {"of": []}, False),
    ("combine op unknown", "combine", {"op": "xor", "of": ["subject", "depth"]}, False),
    # gradients
    ("linear angle 0", "linear_gradient", {"angle": 0}, True),
    ("linear angle past the turn", "linear_gradient", {"angle": 725}, True),
    ("linear angle negative", "linear_gradient", {"angle": -90}, True),
    ("linear angle as text", "linear_gradient", {"angle": "up"}, False),
    ("radial at the corners", "radial_gradient", {"x": 0, "y": 1}, True),
    ("radial x below", "radial_gradient", {"x": -0.5}, False),
    ("radial x above", "radial_gradient", {"x": 1.5}, False),
    ("radial y below", "radial_gradient", {"y": -0.5}, False),
    ("radial y above", "radial_gradient", {"y": 1.5}, False),
    ("radius, lowest", "radial_gradient", {"radius": 0.01}, True),
    ("radius, highest", "radial_gradient", {"radius": 3}, True),
    ("radius, below", "radial_gradient", {"radius": 0.005}, False),
    ("radius, above", "radial_gradient", {"radius": 3.5}, False),
    # given, and what every kind refuses
    ("given", "given", {}, True),
    ("given with a field", "given", {"of": "x"}, False),
    ("a field of another kind", "grid", {"count": 3}, False),
    ("a misspelt field of band", "band", {"of": "depth", "lo": 0.1}, False),
    ("a misspelt field of radial", "radial_gradient", {"radius": 0.5, "raduis": 1}, False),
    ("a field of pick on subject", "subject", {"of": "p"}, False),
]


def python_accepts_region(kind: str, fields: dict) -> bool:
    """Whether Python makes the region and a recipe that declares it (the recipe's check is the core's, which alone
    reads a path)."""
    try:
        region = PYTHON_KINDS[kind].parse(fields, "region 'r'")
        Recipe([Step("grade")], regions={"p": regions.Grid(), "r": region})
    except ValueError:  # RecipeError, which is one
        return False
    return True


def core_accepts_region(kind: str, fields: dict) -> bool:
    """Whether the core's check of a recipe takes the region (it does not run it)."""
    try:
        _core.check_recipe(
            {
                "steps": [{"effect": "grade"}],
                # `p` is a partition for a pick or a subject to refer to
                "regions": {"p": {"kind": "grid"}, "r": {"kind": kind, **fields}},
            }
        )
    except ValueError:  # CoreError, which is one
        return False
    return True


@pytest.mark.parametrize(("what", "kind", "fields", "valid"), REGION_ROWS, ids=[row[0] for row in REGION_ROWS])
def test_the_core_and_python_accept_and_refuse_the_same_regions(what, kind, fields, valid):
    assert python_accepts_region(kind, fields) == valid, "Python's declaration"
    assert core_accepts_region(kind, fields) == valid, "the core"


def test_every_kind_has_rows_with_a_known_good_and_a_known_bad_one():
    for kind in PYTHON_KINDS:
        assert {valid for _, row_kind, _, valid in REGION_ROWS if row_kind == kind} == {True, False}, kind


def test_the_core_has_the_kinds_python_has():
    names: dict[str, dict] = {
        "pick": {"of": "p"},
        "band": {"of": "depth"},
        "bands": {"of": "depth"},
        "combine": {"of": ["subject", "depth"]},
    }
    refused = [kind for kind in PYTHON_KINDS if not core_accepts_region(kind, names.get(kind, {}))]
    assert refused == []
    assert not core_accepts_region("lattice", {})


def plain(value: object) -> Any:
    """A dumped model as JSON holds it: tuples as lists, nothing set left out."""
    return json.loads(json.dumps(value))


def without_none(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: without_none(item) for key, item in value.items() if item is not None}
    if isinstance(value, list):
        return [without_none(item) for item in value]
    return value


@pytest.mark.parametrize("kind", PYTHON_KINDS)
def test_the_core_names_the_fields_of_a_region_and_defaults_them_as_python_does(kind):
    read = _core.read_recipe(json.dumps({"steps": [], "regions": {"r": {"kind": kind}}}), "json")["regions"]["r"]
    python = without_none(plain(PYTHON_KINDS[kind]().model_dump()))
    assert read == {"kind": kind, **python}


@pytest.mark.parametrize("preset", ["glass", "torn", "stone", "mosaic"])
def test_a_shards_preset_expands_when_read_to_the_fields_of_python_s_model_and_a_given_field_wins(preset):
    read = _core.read_recipe(json.dumps({"steps": [], "regions": {"r": {"kind": "shards", "preset": preset}}}), "json")
    expanded = read["regions"]["r"]
    python = without_none(plain(regions.Shards.parse({"preset": preset}, "region 'r'").model_dump()))
    assert {key: value for key, value in expanded.items() if key != "kind"} == python
    given = _core.read_recipe(
        json.dumps({"steps": [], "regions": {"r": {"kind": "shards", "preset": preset, "count": 7}}}), "json"
    )
    assert given["regions"]["r"]["count"] == 7


def test_a_count_of_shares_is_the_same_list_in_the_core_and_in_python():
    read = _core.read_recipe(json.dumps({"steps": [], "regions": {"r": {"kind": "grid", "columns": 3}}}), "json")
    assert read["regions"]["r"]["columns"] == list(regions.Grid(columns=3).columns) == [1.0, 1.0, 1.0]


# --- references: what a name must name, in Python's checks and the core's ---------------------------------------------

OPEN = "format = 3\n[region.panes]\nkind = 'grid'\n[region.cutout]\nkind = 'path'\n"
GRADE = "[[step]]\neffect = 'grade'\n"
SEAMS = "[[step]]\neffect = 'seams'\n"


def laid(kind_and_fields: str) -> str:
    """The recipe with the regions panes (a partition) and cutout (a selection), and `r` as written, over one step."""
    return OPEN + f"[region.r]\n{kind_and_fields}" + GRADE


# (what the row is about, the recipe file, whether every name in it names a region of the form its place needs)
REFERENCE_ROWS = [
    ("mask a selection", OPEN + GRADE + "mask = 'cutout'\n", True),
    ("mask a built-in selection", OPEN + GRADE + "mask = 'subject'\n", True),
    ("mask a partition", OPEN + GRADE + "mask = 'panes'\n", False),
    ("mask nothing", OPEN + GRADE + "mask = 'nowhere'\n", False),
    ("a drive by a selection", OPEN + GRADE + "exposure = { by = 'cutout', start = 0, end = 1 }\n", True),
    ("a drive by a partition", OPEN + GRADE + "exposure = { by = 'panes', start = 0, end = 1 }\n", False),
    ("a drive by nothing", OPEN + GRADE + "exposure = { by = 'nowhere', start = 0, end = 1 }\n", False),
    ("a partition that is one", OPEN + SEAMS + "partition = 'panes'\n", True),
    ("a partition that is a selection (two pieces)", OPEN + SEAMS + "partition = 'cutout'\n", True),
    ("a partition that is a built-in", OPEN + SEAMS + "partition = 'subject'\n", True),
    ("a partition that is nothing", OPEN + SEAMS + "partition = 'nowhere'\n", False),
    ("each over a partition", OPEN + "[[step]]\neach = 'panes'\nlooks = ['none']\n", True),
    ("each over a selection", OPEN + "[[step]]\neach = 'cutout'\nlooks = ['none']\n", False),
    ("each over a built-in", OPEN + "[[step]]\neach = 'subject'\nlooks = ['none']\n", False),
    ("each over nothing", OPEN + "[[step]]\neach = 'nowhere'\nlooks = ['none']\n", False),
    ("each masked by a selection", OPEN + "[[step]]\neach = 'panes'\nmask = 'cutout'\nlooks = ['none']\n", True),
    ("each masked by a partition", OPEN + "[[step]]\neach = 'panes'\nmask = 'panes'\nlooks = ['none']\n", False),
    (
        "a look masked by a partition",
        OPEN + "[[step]]\neach = 'panes'\nlooks = [{ effect = 'grade', mask = 'panes' }]\n",
        False,
    ),
    (
        "a look masked by a selection",
        OPEN + "[[step]]\neach = 'panes'\nlooks = [{ effect = 'grade', mask = 'cutout' }]\n",
        True,
    ),
    ("a grid fit to a selection", laid("kind = 'grid'\nfit = 'cutout'\n"), True),
    ("a grid fit to a built-in", laid("kind = 'grid'\nfit = 'subject'\n"), True),
    ("a grid fit to a partition", laid("kind = 'grid'\nfit = 'panes'\n"), False),
    ("a grid fit to nothing", laid("kind = 'grid'\nfit = 'nowhere'\n"), False),
    ("a band of a selection", laid("kind = 'band'\nof = 'cutout'\n"), True),
    ("a band of the depth", laid("kind = 'band'\nof = 'depth'\n"), True),
    ("a band of a partition", laid("kind = 'band'\nof = 'panes'\n"), False),
    ("a band of nothing", laid("kind = 'band'\nof = 'nowhere'\n"), False),
    ("a band with no `of`", laid("kind = 'band'\n"), False),
    ("bands of a selection", laid("kind = 'bands'\nof = 'cutout'\n"), True),
    ("bands of a partition", laid("kind = 'bands'\nof = 'panes'\n"), False),
    ("combine of selections", laid("kind = 'combine'\nof = ['cutout', 'subject']\n"), True),
    ("combine with a partition", laid("kind = 'combine'\nof = ['cutout', 'panes']\n"), False),
    ("combine with nothing", laid("kind = 'combine'\nof = ['cutout', 'nowhere']\n"), False),
    ("a pick of a partition", laid("kind = 'pick'\nof = 'panes'\n"), True),
    ("a pick of a selection", laid("kind = 'pick'\nof = 'cutout'\n"), False),
    ("a pick of a built-in", laid("kind = 'pick'\nof = 'subject'\n"), False),
    ("a pick of nothing", laid("kind = 'pick'\nof = 'nowhere'\n"), False),
    ("the subject found in a partition", laid("kind = 'subject'\nper = 'panes'\n"), True),
    ("the subject found in a selection", laid("kind = 'subject'\nper = 'cutout'\n"), False),
    ("the subject found in nothing", laid("kind = 'subject'\nper = 'nowhere'\n"), False),
    ("a region called subject", OPEN + "[region.subject]\nkind = 'grid'\n" + GRADE, False),
    ("a region called light", OPEN + "[region.light]\nkind = 'given'\n" + GRADE, False),
    ("a region called like a built-in in capitals", OPEN + "[region.Subject]\nkind = 'grid'\n" + GRADE, True),
    ("a band of itself", laid("kind = 'band'\nof = 'r'\n"), False),
    (
        "two bands of each other",
        OPEN + "[region.a]\nkind = 'band'\nof = 'b'\n[region.b]\nkind = 'band'\nof = 'a'\n" + GRADE,
        False,
    ),
    (
        "a chain that ends",
        OPEN + "[region.a]\nkind = 'band'\nof = 'b'\n[region.b]\nkind = 'band'\nof = 'depth'\n" + GRADE,
        True,
    ),
    (
        "a circle through fit and per",
        OPEN + "[region.s]\nkind = 'subject'\nper = 'g'\n[region.g]\nkind = 'grid'\nfit = 's'\n" + GRADE,
        False,
    ),
    ("a bad reference in a region no step uses", laid("kind = 'band'\nof = 'nowhere'\n"), False),
]


def python_accepts_file(tmp_path, text: str) -> bool:
    path = tmp_path / "recipe.toml"
    path.write_text(text, encoding="utf-8")
    try:
        load_recipe(path)
    except ValueError:  # RecipeError, which is one
        return False
    return True


def core_accepts_file(text: str, presets: dict | None = None) -> bool:
    registry.all_effects()  # loads the effects, which mirrors the Python ones into the core
    try:
        _core.check_recipe(_core.read_recipe(text, "toml"), presets=presets)
    except ValueError:  # CoreError, which is one
        return False
    return True


@pytest.mark.parametrize(("what", "text", "valid"), REFERENCE_ROWS, ids=[row[0] for row in REFERENCE_ROWS])
def test_the_core_and_python_accept_and_refuse_the_same_references(tmp_path, what, text, valid):
    assert python_accepts_file(tmp_path, text) == valid, "Python's checks"
    assert core_accepts_file(text) == valid, "the core"


def test_the_references_have_known_good_and_known_bad_rows():
    assert {valid for _, _, valid in REFERENCE_ROWS} == {True, False}


@pytest.mark.parametrize("path", FILES)
def test_the_core_accepts_every_shipped_look_and_example_as_python_does(path):
    presets = {file.stem: file.read_text(encoding="utf-8") for file in SHIPPED_PRESETS.glob("*.toml")}
    assert core_accepts_file(path.read_text(encoding="utf-8"), presets)
