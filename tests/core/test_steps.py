"""A step's values are validated by its effect's parameter model, at once; the recipe objects (Step, Recipe) are
validated, frozen models wherever they are made."""

import inspect
import tomllib
from importlib.resources import files

import numpy as np
import pytest

from bullterrier_image import fx
from bullterrier_image.core import registry
from bullterrier_image.core.framing import Framing
from bullterrier_image.core.params import EffectParams, Picture, param
from bullterrier_image.core.registry import effect
from bullterrier_image.core.steps import Recipe, Step
from bullterrier_image.errors import RecipeError, UnknownEffectError
from bullterrier_image.loader import load_recipe
from bullterrier_image.recipe import Each, Grid, Pick, Shards


class GainParams(EffectParams):
    """Gain of the probe."""

    gain: float = param(1.0, unit="ratio", min=0, max=4, doc="Multiplies every value. Raising it brightens.")
    offset: float = param(0.0, unit="fraction", min=-1, max=1, doc="Added after the gain.")


@pytest.fixture
def gain(throwaway_effects):
    def gain_probe(rgb: np.ndarray, params: GainParams) -> np.ndarray:
        """Brightens by a gain."""
        return np.clip(rgb * params.gain + params.offset, 0, 1).astype(np.float32)

    return effect(category="light", params=GainParams)(gain_probe)


# --- values through the model ---------------------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("values", "complaint"),
    [
        ({"gain": 9}, r"`gain`.*<= 4.*\b9\b"),  # the field, the limit and the value
        ({"gian": 2}, r"`gian`.*gain, offset"),  # an unknown value lists the valid ones
        ({"gain": "2"}, r"`gain`.*number"),
        ({"gain": {"from": 0.0, "to": 1.0}}, r"`gain`.*number"),  # the table of an animated value, no longer a value
    ],
    ids=["out of range", "unknown", "wrong kind", "a table"],
)
def test_a_bad_value_is_refused_naming_the_effect_and_the_problem(gain, values, complaint):
    with pytest.raises(ValueError, match=f"gain_probe.*{complaint}"):
        Step("gain_probe", values)


def test_valid_values_make_a_step_that_keeps_them_as_written(gain):
    step = Step("gain_probe", {"gain": 2})
    assert step.params == {"gain": 2}
    assert step.describe() == "gain_probe(gain=2)"


def test_a_value_out_of_range_in_a_recipe_file_names_the_file_step_field_and_value(gain, tmp_path):
    path = tmp_path / "hot.toml"
    path.write_text("[[step]]\neffect = 'gain_probe'\ngain = 7.5\n", encoding="utf-8")
    with pytest.raises(ValueError, match=r"hot\.toml.*step 1.*gain_probe.*`gain`.*\b4\b.*7\.5"):
        load_recipe(path)


# --- the effect's own default opacity --------------------------------------------------------------------------------


@pytest.fixture
def faint(throwaway_effects):
    def faint_probe(rgb: np.ndarray, params: GainParams) -> np.ndarray:
        """Brightens by a gain, mixed in at 0.4 unless a step says otherwise."""
        return rgb

    return effect(category="light", params=GainParams, opacity=0.4)(faint_probe)


def recipe_step(folder, keys: str) -> Step:
    path = folder / "faint.toml"
    path.write_text(f"[[step]]\neffect = 'faint_probe'\n{keys}", encoding="utf-8")
    return load_recipe(path).steps[0].runs[0]


@pytest.mark.parametrize(
    ("make", "opacity"),
    [
        pytest.param(lambda folder: Step("faint_probe"), 0.4, id="Step takes the effect's default"),
        pytest.param(lambda folder: fx.faint_probe(), 0.4, id="fx takes it"),
        pytest.param(lambda folder: recipe_step(folder, ""), 0.4, id="a recipe file takes it"),
        pytest.param(lambda folder: Step("faint_probe", opacity=1.0), 1.0, id="Step: a given opacity replaces it"),
        pytest.param(lambda folder: fx.faint_probe(opacity=0.5), 0.5, id="fx: replaces it, does not multiply"),
        pytest.param(lambda folder: recipe_step(folder, "opacity = 0.5\n"), 0.5, id="a recipe file: replaces it"),
    ],
)
def test_a_step_has_its_effects_default_opacity_unless_it_gives_one(faint, tmp_path, make, opacity):
    assert make(tmp_path).opacity == opacity


def test_the_fx_signature_shows_the_effects_default_opacity(faint):
    assert inspect.signature(fx.faint_probe).parameters["opacity"].default == 0.4
    (stub,) = [line for line in fx.stub_text().splitlines() if line.startswith("def faint_probe(")]
    assert ", opacity: float = 0.4, blend_mode: " in stub


# --- the recipe objects are validated, frozen models -----------------------------------------------------------------


def test_a_recipe_needs_a_positive_size_or_none():
    assert Recipe("r", [Step("grade")], size=None).size is None
    with pytest.raises(ValueError, match="size"):
        Recipe("r", [Step("grade")], size=0)


# A recipe's name becomes an output file name, and recipes are shared as data: a name that could leave the output folder
# (or hide a file there) is refused when the recipe is made (CWE-22, path traversal).
@pytest.mark.parametrize(
    "name", ["../../x", "/abs/x", "a/b", "", ".hidden", "a" * 65, "a\\b", "look\n", "..", "-rf", "my look"]
)
def test_a_recipe_name_that_is_not_a_safe_file_name_is_refused_naming_it_and_the_allowed_pattern(name):
    with pytest.raises(RecipeError) as raised:
        Recipe(name, [Step("grade")])
    message = str(raised.value)
    assert repr(name)[:20] in message  # the value given
    assert "A-Za-z0-9" in message  # the pattern a name must match


# Windows reserves these device names in any case, with or without an extension: a file so named cannot be made there.
@pytest.mark.parametrize(
    "name", ["CON", "con", "Prn", "AUX", "nul", "COM1", "com9", "LPT1", "lpt9", "CON.png", "nul.tar.gz", "Aux.v2"]
)
def test_a_name_windows_reserves_for_a_device_is_refused_naming_it(name):
    with pytest.raises(RecipeError, match="reserved") as raised:
        Recipe(name, [Step("grade")])
    assert repr(name) in str(raised.value)


@pytest.mark.parametrize(
    "name",
    [
        *("my-look", "look_2.v1", "a" * 64, "7", "Neon.Pulse-2"),  # safe file names
        *("CONSOLE", "com10", "nullify", "aux-look", "my.con", "LPT", "printer"),  # only resembling a device name
    ],
)
def test_a_safe_name_is_kept_as_the_recipe_name(name):
    assert Recipe(name, [Step("grade")]).name == name


def test_an_explicit_name_in_a_file_is_not_repaired(tmp_path):
    path = tmp_path / "fine.toml"
    path.write_text("name = 'my look'\n[[step]]\neffect = 'grade'\n", encoding="utf-8")
    with pytest.raises(RecipeError, match=r"^fine\.toml: .*'my look'"):
        load_recipe(path)


def test_a_recipe_holds_the_framing_it_is_given_and_by_default_none():
    framing = Framing(aspect=1.0, anchor="subject")
    assert Recipe("framed", [Step("grade")], frame=framing).frame == framing
    assert Recipe("plain", [Step("grade")]).frame == Framing()


def test_a_recipe_refuses_a_frame_that_is_not_a_framing():
    with pytest.raises(RecipeError, match="frame"):
        Recipe("framed", [Step("grade")], frame="square")  # type: ignore[arg-type]  # the point of the test


@pytest.mark.parametrize(
    ("steps", "frame", "needs_matte"),
    [
        pytest.param([Step("grade")], Framing(), False, id="nothing masked, nothing framed"),
        pytest.param([Step("grade", mask="subject")], Framing(), True, id="a masked step"),
        pytest.param([Step("grade")], Framing(anchor="subject"), True, id="a crop around the subject"),
        pytest.param([Step("grade")], Framing(transparent="background"), True, id="the subject as the alpha"),
        pytest.param([Step("grade")], Framing(aspect=1.0, transparent="circle"), False, id="a crop and a circle"),
    ],
)
def test_a_recipe_needs_the_subject_when_a_step_or_its_framing_does(steps, frame, needs_matte):
    assert Recipe("look", steps, frame=frame).needs_matte is needs_matte


def test_the_recipe_objects_are_frozen():
    with pytest.raises(ValueError, match="frozen"):
        Step("grade").opacity = 0.5  # type: ignore[misc]  # the point of the test


def test_the_recipe_objects_refuse_unknown_fields():
    with pytest.raises(ValueError, match="colour"):
        Step.model_validate({"effect": "grade", "colour": "red"})


# --- presets: an effect's named parameter sets, merged under the parameters a step is given ------------------------


def _shipped(effect: str, preset: str) -> dict:
    """What the shipped file says about a preset, read with tomllib, not through the library, without its text."""
    table = tomllib.loads((files("bullterrier_image") / "presets" / f"{effect}.toml").read_text(encoding="utf-8"))[
        preset
    ]
    return {key: value for key, value in table.items() if key != "description"}


EXPLICIT = {"shadow_clip": 0.2}  # a gradient_map parameter the sepia preset does not set


def test_a_steps_parameters_are_the_presets_with_the_given_ones_over_them():
    assert Step("gradient_map", preset="sepia").params == _shipped("gradient_map", "sepia")
    assert Step("gradient_map", EXPLICIT, preset="sepia").params == {**_shipped("gradient_map", "sepia"), **EXPLICIT}


def test_a_given_parameter_wins_over_the_presets():
    sepia = _shipped("gradient_map", "sepia")
    chosen = ["#000000", "#ff0000"]
    step = Step("gradient_map", {"stops": chosen}, preset="sepia")
    assert step.params["stops"] == chosen != sepia["stops"]


def test_a_preset_step_renders_as_the_step_with_the_same_parameters_written_out(photo):
    preset = Step("gradient_map", EXPLICIT, preset="sepia")
    written = Step("gradient_map", {**_shipped("gradient_map", "sepia"), **EXPLICIT})
    first = registry.apply("gradient_map", photo, **preset.params)
    np.testing.assert_array_equal(first, registry.apply("gradient_map", photo, **written.params))
    assert not np.array_equal(first, registry.apply("gradient_map", photo))  # and it is not the default look


def test_an_unknown_preset_names_the_effect_and_lists_its_presets():
    with pytest.raises(RecipeError, match=r"gradient_map.*'sepya'.*sepia.*cyanotype"):
        Step("gradient_map", preset="sepya")


def test_a_preset_for_an_effect_without_presets_says_so(gain):
    with pytest.raises(RecipeError, match=r"gain_probe.*'sepia'.*no presets"):
        Step("gain_probe", preset="sepia")


def test_a_preset_for_an_unknown_effect_lists_the_effects():
    with pytest.raises(UnknownEffectError, match=r"no_such_effect.*gradient_map"):
        Step("no_such_effect", preset="sepia")


# --- a Picture parameter -------------------------------------------------------------------------------------------


def test_steps_holding_pictures_compare_by_pixels_and_describe_without_them(layer_probe):
    rng = np.random.default_rng(5)
    pixels = rng.random((240, 320, 3), dtype=np.float32)
    first = Step("layer_probe", {"image": Picture(pixels, "tex.png")})
    assert first == Step("layer_probe", {"image": Picture(pixels.copy(), "other.png")})
    assert first != Step("layer_probe", {"image": Picture(pixels[::-1].copy(), "tex.png")})
    assert first != Step("layer_probe", {"image": "#808080"})
    assert first.describe().startswith("layer_probe(image=Picture('tex")
    assert len(first.describe()) <= 56


def test_an_effect_receives_the_picture_a_step_holds(layer_probe):
    picture = Picture(np.full((3, 3, 3), 0.25, np.float32))
    out = registry.apply("layer_probe", np.zeros((4, 4, 3), np.float32), image=picture)
    np.testing.assert_allclose(out, 0.25)
    assert Step("layer_probe", {"image": picture}, mask="subject").params["image"] is picture


# --- regions: steps name them, the recipe declares them ---------------------------------------------------------------

REGIONS: dict[str, Grid | Shards | Pick] = {
    "panes": Grid(columns=3, rows=2),
    "glass": Shards(count=5),
    "head": Pick(of="panes", pieces=[1]),
}


def test_a_recipe_declaring_regions_accepts_steps_that_name_them_in_every_place_a_step_can():
    steps: list[Step | Each] = [
        Step("grade", mask="head"),
        Step("grade", mask="subject"),
        Each("panes", [Step("grade", mask="head"), "none", Step("grade", opacity=0.5)], mask="background"),
    ]
    assert Recipe("r", steps, regions=REGIONS).regions == REGIONS
    assert Recipe("plain", [Step("grade")]).regions == {}


# Every way a step can name a region wrongly: each is refused when the recipe is made, naming the step (counted from 1),
# the key, the name given and what would have been valid.
BAD_REFERENCES = {
    "a mask no region has": (
        lambda: Recipe("r", [Step("grade"), Step("grade", mask="nope")], regions=REGIONS),
        r"step 2: `mask` names 'nope', which is no region; valid selections: subject, background, head",
    ),
    "a mask that is a partition": (
        lambda: Recipe("r", [Step("grade", mask="panes")], regions=REGIONS),
        r"step 1: `mask` names 'panes', a partition, but it needs a selection; valid selections: .*head",
    ),
    "an each no region has": (
        lambda: Recipe("r", [Each("nope", ["none"])], regions=REGIONS),
        r"step 1: `each` names 'nope', which is no region; valid partitions: panes, glass",
    ),
    "an each that is a selection": (
        lambda: Recipe("r", [Each("head", ["none"])], regions=REGIONS),
        r"step 1: `each` names 'head', a selection, but it needs a partition; valid partitions: panes, glass",
    ),
    "an each that is the subject": (
        lambda: Recipe("r", [Each("subject", ["none"])], regions=REGIONS),
        r"step 1: `each` names 'subject', a selection",
    ),
    "a mask on an each": (
        lambda: Recipe("r", [Each("panes", ["none"], mask="glass")], regions=REGIONS),
        r"step 1: `mask` names 'glass', a partition",
    ),
    "a look with a mask no region has": (
        lambda: Recipe("r", [Each("panes", [Step("grade", mask="nope")])], regions=REGIONS),
        r"step 1: `mask` names 'nope'",
    ),
    "a partition no region has": (
        lambda: Recipe("r", [Step("partition_probe", partition="nope")], regions=REGIONS),
        r"step 1: `partition` names 'nope', which is no region; valid regions: subject, background, panes, glass, head",
    ),
    "a recipe that declares no regions": (
        lambda: Recipe("r", [Step("grade", mask="glass")]),
        r"valid selections: subject, background .*declares no regions of its own.*regions=",
    ),
    "a region fitted to a partition": (
        lambda: Recipe("r", [Step("grade")], regions={"a": Grid(), "b": Shards(fit="a")}),
        r"region 'b': `fit` names 'a', a partition, but it needs a selection",
    ),
    "regions in a cycle": (
        lambda: Recipe("r", [Step("grade")], regions={"a": Shards(fit="b"), "b": Pick(of="a")}),
        r"cycle: 'a' -> 'b' -> 'a'",
    ),
    "a region called like a built-in": (
        lambda: Recipe("r", [Step("grade")], regions={"subject": Grid()}),
        r"region 'subject' is a built-in",
    ),
}


@pytest.mark.parametrize(("make", "complaint"), BAD_REFERENCES.values(), ids=BAD_REFERENCES.keys())
def test_a_recipe_refuses_a_bad_region_reference_when_it_is_made(partition_probe, make, complaint):
    with pytest.raises(RecipeError, match=complaint):
        make()


BAD_EACH = {
    "no looks": (lambda: Each("panes", []), r"looks"),
    "a look that is a word other than none": (lambda: Each("panes", ["nothing"]), r"looks"),
    "a look that is a number": (lambda: Each("panes", [3]), r"looks"),  # type: ignore[list-item]
    "an assignment that does not exist": (
        lambda: Each("panes", ["none"], assign="shuffle"),
        r"`assign`.*cycle.*random",
    ),
    "a negative seed": (lambda: Each("panes", ["none"], seed=-1), r"`seed`.*0"),
    "a feather below 0": (lambda: Each("panes", ["none"], feather=-0.5), r"`feather`.*0"),
    "an infinite feather": (lambda: Each("panes", ["none"], feather=float("inf")), r"`feather`"),
    "an unknown key": (lambda: Each("panes", ["none"], effect="grade"), r"`effect`"),
    "a mask that is not a name": (lambda: Each("panes", ["none"], mask=3), r"`mask`"),  # type: ignore[arg-type]
}


@pytest.mark.parametrize(("make", "complaint"), BAD_EACH.values(), ids=BAD_EACH.keys())
def test_an_each_step_is_refused_naming_what_is_wrong(make, complaint):
    with pytest.raises(RecipeError, match=complaint):
        make()


PARTITIONS = {
    "an effect that draws a partition needs one named": (
        lambda: Step("partition_probe"),
        r"partition_probe.*`partition`",
    ),
    "an effect that takes none refuses one": (
        lambda: Step("grade", partition="panes"),
        r"grade.*takes no partition",
    ),
}


@pytest.mark.parametrize(("make", "complaint"), PARTITIONS.values(), ids=PARTITIONS.keys())
def test_a_step_names_a_partition_exactly_when_its_effect_takes_one(partition_probe, make, complaint):
    with pytest.raises(RecipeError, match=complaint):
        make()
    assert Step("partition_probe", partition="panes").partition == "panes"


@pytest.mark.parametrize(
    ("steps", "regions", "needs_matte"),
    [
        pytest.param([Step("grade", mask="head")], REGIONS, False, id="a mask from a grid needs no subject"),
        pytest.param(
            [Step("grade", mask="head")], {**REGIONS, "unused": Shards(fit="subject")}, False, id="an unused one"
        ),
        pytest.param(
            [Step("grade", mask="head")],
            {"panes": Grid(fit="subject"), "head": Pick(of="panes")},
            True,
            id="a grid laid over the subject",
        ),
        pytest.param(
            [Each("cut", ["none"])], {"cut": Shards(fit="subject")}, True, id="an each over shards on the subject"
        ),
        pytest.param([Each("cut", ["none"])], {"cut": Shards(fit="background")}, True, id="on the background"),
        pytest.param(
            [Each("cut", ["none"])],
            {"cut": Shards(fit="aura"), "aura": Pick(of="g"), "g": Grid(fit="subject")},
            True,
            id="through two regions",
        ),
        pytest.param([Each("g", [Step("evaporate"), "none"])], {"g": Grid()}, True, id="a look that takes the subject"),
        pytest.param([Each("g", [Step("grade"), "none"])], {"g": Grid()}, False, id="looks that do not"),
        pytest.param([Each("g", ["none"], mask="subject")], {"g": Grid()}, True, id="an each masked to the subject"),
    ],
)
def test_a_recipe_needs_the_subject_only_when_a_region_it_uses_does(steps, regions, needs_matte):
    assert Recipe("look", steps, regions=regions).needs_matte is needs_matte


def test_an_each_step_describes_itself_with_its_partition_and_looks():
    step = Each("panes", [Step("grade", {"exposure": 0.5}), "none"], mask="subject", opacity=0.5)
    assert step.describe() == "each panes [grade(exposure=0.5), none] on subject x0.5"
    assert len(Each("panes", [Step("ascii", {"chars": "xy" * 50})] * 3).describe(width=30)) == 30


def test_an_each_step_shows_fully_unless_it_gives_an_opacity():
    assert Each("panes", [Step("grade")]).opacity == 1.0
    assert Each("panes", [Step("grade")], opacity=0.3).opacity == 0.3


def test_the_each_step_is_a_frozen_value_and_keeps_its_looks_as_a_tuple():
    step = Each("panes", [Step("grade"), "none"])
    assert step.looks == (Step("grade"), "none")
    with pytest.raises(ValueError, match="frozen"):
        step.feather = 3.0  # type: ignore[misc]  # the point of the test
