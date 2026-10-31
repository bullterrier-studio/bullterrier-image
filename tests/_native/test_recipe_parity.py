"""The Rust core's checks of parameters and recipe files against the Python engine, which is the oracle.

The extension is the public contract here (`_core.check`, `_core.read_recipe`, `_core.vocabulary`); Python's own models
and loader say what is right. Parameters: for each row both accept it and give the same complete values, or both
refuse it. Files: `read_recipe` only reads (shape and format versions; the rules of a look are `Recipe.checked` in
Rust, tested there), so a Python-valid file must read into the steps Python loads, and a file Python refuses for its
shape or its format version must be refused by the core too.
"""

import json
from pathlib import Path
from typing import get_args

import pytest

from bullterrier_image._native import _core  # noqa: PLC2701  architecture: the Rust core is compared with Python's engine
from bullterrier_image.core import registry
from bullterrier_image.core.framing import Framing
from bullterrier_image.core.registry import CATEGORIES, INPUTS, STEP_KEYS
from bullterrier_image.core.steps import Each, Step
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
]


def python_answer(effect: str, given: dict) -> dict | None:
    """The complete parameters Python's model makes of `given`, or None when it refuses them."""
    try:
        return registry.get(effect).validate(given).model_dump()
    except ValueError:  # RecipeError, which is one
        return None


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
    assert set(read.get("regions", {})) == set(loaded.regions)  # the core keeps no file order
    assert bool(read.get("frame")) == (loaded.frame != Framing())


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


def test_the_core_names_the_vocabulary_of_recipes_in_the_order_the_package_lists_it():
    vocabulary = _core.vocabulary()
    assert vocabulary["categories"] == list(CATEGORIES)
    assert vocabulary["inputs"] == list(INPUTS)
    assert vocabulary["step_keys"] == list(STEP_KEYS)
    assert vocabulary["blend_modes"] == list(get_args(BlendMode))
