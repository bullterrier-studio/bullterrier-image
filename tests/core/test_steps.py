"""A step's values are validated by its effect's parameter model, at once; the recipe objects (Step, Recipe) are
validated, frozen models wherever they are made."""

import inspect
import tomllib
from importlib.resources import files
from typing import Literal

import numpy as np
import pytest
from pydantic import model_validator

from bullterrier_image import fx
from bullterrier_image.core import registry
from bullterrier_image.core.framing import Framing
from bullterrier_image.core.params import Drive, EffectParams, Picture, param
from bullterrier_image.core.regions import Kind
from bullterrier_image.core.registry import effect
from bullterrier_image.core.steps import Recipe, Step
from bullterrier_image.errors import RecipeError, UnknownEffectError
from bullterrier_image.loader import load_recipe
from bullterrier_image.recipe import Band, Bands, Each, Edge, Given, Grid, Pick, Shards, Subject, render


class GainParams(EffectParams):
    """Gain of the probe."""

    gain: float = param(
        1.0, unit="ratio", min=0, max=4, drivable=True, doc="Multiplies every value. Raising it brightens."
    )
    offset: float = param(0.0, unit="fraction", min=-1, max=1, doc="Added after the gain.")
    taps: int = param(1, unit="count", min=1, max=9, doc="How many times the gain is applied.")
    mode: Literal["clip", "wrap"] = param("clip", doc="What happens past 1: 'clip' stops, 'wrap' starts again.")


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
        ({"gian": 2}, r"`gian`.*gain, mode, offset, taps"),  # an unknown value lists the valid ones
        ({"gain": "2"}, r"`gain`.*number"),
        ({"gain": {"from": 0.0, "to": 1.0}}, r"`gain`.*number"),  # the table of an animated value, no longer a value
    ],
    ids=["out of range", "unknown", "wrong kind", "a table"],
)
def test_a_bad_value_is_refused_naming_the_effect_and_the_problem(gain, values, complaint):
    with pytest.raises(ValueError, match=f"gain_probe.*{complaint}"):
        Step("gain_probe", params=values)


def test_valid_values_make_a_step_that_keeps_them_as_written(gain):
    step = Step("gain_probe", params={"gain": 2})
    assert step.params == {"gain": 2}
    assert step.describe() == "gain_probe(gain=2)"


def test_a_value_out_of_range_in_a_recipe_file_names_the_file_step_field_and_value(gain, tmp_path):
    path = tmp_path / "hot.toml"
    path.write_text("[[step]]\neffect = 'gain_probe'\ngain = 7.5\n", encoding="utf-8")
    with pytest.raises(ValueError, match=r"hot\.toml.*step 1.*gain_probe.*`gain`.*\b4\b.*7\.5"):
        load_recipe(path)


# --- driven parameters: a field varies a parameter pixel by pixel ----------------------------------------------------

NEAR = Band(of="depth")


def drive(**keys) -> Drive:
    return Drive(**{"by": "near", "start": 0.0, "end": 2.0, **keys})


def test_a_step_takes_a_drive_for_a_drivable_parameter_and_keeps_it_as_written(gain):
    step = Step("gain_probe", params={"gain": drive(), "offset": 0.1})
    assert step.params == {"gain": drive(), "offset": 0.1}
    assert step.drives == {"gain": drive()}
    assert step.fixed == {"offset": 0.1}
    assert step.describe() == "gain_probe(gain=by near 0..2, offset=0.1)"


def test_a_drive_written_as_a_table_is_the_drive_written_in_python(gain):
    from_table = Step("gain_probe", params={"gain": {"by": "near", "start": 0, "end": 2, "levels": 7}})
    assert from_table == Step("gain_probe", params={"gain": Drive(by="near", start=0, end=2, levels=7)})


def test_a_step_with_no_drive_has_none(gain):
    step = Step("gain_probe", params={"gain": 2})
    assert (step.drives, step.fixed) == ({}, {"gain": 2})


DRIVEN_MISTAKES = {
    "a parameter the effect does not declare drivable": (
        {"offset": drive(end=0.5)},
        r"gain_probe.*`offset` cannot be driven.*drivable parameters: gain",
    ),
    "a whole number": ({"taps": drive(end=5)}, r"gain_probe.*`taps` cannot be driven.*whole number.*gain"),
    "a choice": ({"mode": drive(end=1)}, r"gain_probe.*`mode` cannot be driven.*not a number.*gain"),
    "a parameter the effect does not have": ({"gian": drive()}, r"gain_probe.*`gian`.*gain, mode, offset, taps"),
    "a start below the range": ({"gain": drive(start=-0.5)}, r"gain_probe.*`gain`.*>= 0.*-0\.5"),
    "an end above the range": ({"gain": drive(end=9)}, r"gain_probe.*`gain`.*<= 4.*\b9\b"),
    "a start and an end both out": ({"gain": drive(start=-1, end=9)}, r"gain_probe.*`gain`"),
    "a table with a misspelt key": (
        {"gain": {"by": "near", "start": 0, "end": 1, "step": 2}},
        r"gain_probe.*`gain`.*step",
    ),
    "a table missing its end": ({"gain": {"by": "near", "start": 0}}, r"gain_probe.*`gain`.*end"),
}


@pytest.mark.parametrize(("values", "complaint"), DRIVEN_MISTAKES.values(), ids=DRIVEN_MISTAKES.keys())
def test_a_drive_that_cannot_be_is_refused_naming_the_effect_the_parameter_and_why(gain, values, complaint):
    with pytest.raises(RecipeError, match=complaint):
        Step("gain_probe", params=values)


def test_a_drivable_effect_with_none_to_offer_says_none_are_drivable(throwaway_effects):
    class Plain(EffectParams):
        """No drivable field."""

        amount: float = param(0.5, unit="fraction", min=0, max=1, doc="How much.")

    @effect(category="light", params=Plain)
    def plain_probe(rgb: np.ndarray, params: Plain) -> np.ndarray:
        """Nothing."""
        return rgb

    with pytest.raises(RecipeError, match=r"`amount` cannot be driven.*drivable parameters: none"):
        Step("plain_probe", params={"amount": drive(end=1)})


def test_every_value_a_drive_goes_through_is_checked_not_only_its_ends(throwaway_effects):
    class Gapped(EffectParams):
        """A number with a gap in the middle of its range."""

        level: float = param(0.2, unit="fraction", min=0, max=1, drivable=True, doc="Anything but 0.45 to 0.55.")

        @model_validator(mode="after")
        def _outside_the_gap(self) -> "Gapped":
            """`level` is not in the gap."""
            if 0.45 < self.level < 0.55:
                raise ValueError(f"level {self.level} is in the gap 0.45..0.55")
            return self

    @effect(category="light", params=Gapped)
    def gapped_probe(rgb: np.ndarray, params: Gapped) -> np.ndarray:
        """Nothing."""
        return rgb

    assert Step(
        "gapped_probe", params={"level": Drive(by="near", start=0.0, end=0.4, levels=5)}
    ).drives  # all clear of it
    with pytest.raises(
        RecipeError, match=r"gapped_probe.*level 0\.5 is in the gap"
    ):  # both ends valid, a level between not
        Step("gapped_probe", params={"level": Drive(by="near", start=0.2, end=0.8, levels=5)})


def test_a_drive_whose_end_breaks_a_rule_between_two_numbers_is_refused_naming_the_end(throwaway_effects):
    class Paired(EffectParams):
        """Two numbers that must stay ordered."""

        low: float = param(0.2, unit="fraction", min=0, max=1, drivable=True, doc="The lower number.")
        high: float = param(0.8, unit="fraction", min=0, max=1, doc="The upper number, at least `low`.")

        @model_validator(mode="after")
        def _ordered(self) -> "Paired":
            """`low` is at most `high`."""
            if self.low > self.high:
                raise ValueError(f"low {self.low} is above high {self.high}")
            return self

    @effect(category="light", params=Paired)
    def paired_probe(rgb: np.ndarray, params: Paired) -> np.ndarray:
        """Nothing."""
        return rgb

    assert Step("paired_probe", params={"low": Drive(by="near", start=0.0, end=0.8)}).drives
    with pytest.raises(RecipeError, match=r"paired_probe.*low 0\.9.*above high 0\.8"):
        Step("paired_probe", params={"low": Drive(by="near", start=0.0, end=0.9)})


@pytest.mark.parametrize("levels", range(2, 17))
@pytest.mark.parametrize(("start", "end"), [(0.2, 4.0), (4.0, 0.2), (0.0, 4.0), (4.0, 0.0), (0.32, 4.0)])
def test_a_drive_from_one_end_of_the_range_to_the_other_is_valid_whatever_the_number_of_levels(
    gain, start, end, levels
):
    assert Step("gain_probe", params={"gain": Drive(by="near", start=start, end=end, levels=levels)}).drives


def test_drives_that_are_valid_together_are_not_refused_for_one_end_with_the_others_at_their_defaults(
    throwaway_effects,
):
    class Ordered(EffectParams):
        """Two drivable numbers that must stay ordered."""

        low: float = param(0.1, unit="fraction", min=0, max=1, drivable=True, doc="The lower number.")
        high: float = param(0.8, unit="fraction", min=0, max=1, drivable=True, doc="The upper number, at least `low`.")

        @model_validator(mode="after")
        def _ordered(self) -> "Ordered":
            """`low` is at most `high`."""
            if self.low > self.high:
                raise ValueError(f"low {self.low} is above high {self.high}")
            return self

    @effect(category="light", params=Ordered)
    def ordered_probe(rgb: np.ndarray, params: Ordered) -> np.ndarray:
        """Nothing."""
        return rgb

    both = {
        "low": Drive(by="near", start=0.1, end=0.9, levels=3),
        "high": Drive(by="near", start=0.2, end=1.0, levels=3),
    }
    assert Step("ordered_probe", params=both).drives.keys() == {
        "low",
        "high",
    }  # low 0.9 would be above the default high 0.8
    crossing = {
        "low": Drive(by="near", start=0.1, end=0.9, levels=3),
        "high": Drive(by="near", start=0.5, end=0.6, levels=3),
    }
    with pytest.raises(RecipeError, match=r"ordered_probe.*low 0\.9.*above high 0\.6"):  # at the end the user wrote
        Step("ordered_probe", params=crossing)


def test_a_look_of_an_each_takes_no_name_because_nothing_could_reach_it_by_it(gain):
    with pytest.raises(RecipeError, match=r"a look takes no `name`.*each\.looks\[0\]"):
        Each("g", looks=[Step("gain_probe", name="x"), "none"])
    assert Each("g", looks=[Step("gain_probe"), "none"], name="pieces").name == "pieces"


def test_drives_of_one_step_share_a_field_and_a_number_of_levels(gain, throwaway_effects):
    class Two(EffectParams):
        """Two drivable numbers."""

        first: float = param(0.0, unit="fraction", min=0, max=1, drivable=True, doc="First.")
        second: float = param(0.0, unit="fraction", min=0, max=1, drivable=True, doc="Second.")

    @effect(category="light", params=Two)
    def two_probe(rgb: np.ndarray, params: Two) -> np.ndarray:
        """Nothing."""
        return rgb

    ok = Step(
        "two_probe", params={"first": Drive(by="near", start=0, end=1), "second": Drive(by="near", start=1, end=0)}
    )
    assert ok.drives.keys() == {"first", "second"}
    with pytest.raises(RecipeError, match=r"two_probe.*one field.*'light' and 'near'"):
        Step(
            "two_probe", params={"first": Drive(by="near", start=0, end=1), "second": Drive(by="light", start=0, end=1)}
        )
    with pytest.raises(RecipeError, match=r"two_probe.*one number of levels.*5 and 8"):
        Step(
            "two_probe",
            params={"first": Drive(by="near", start=0, end=1), "second": Drive(by="near", start=0, end=1, levels=8)},
        )


def test_a_step_names_the_field_that_drives_it_as_a_region_it_refers_to(gain):
    step = Step("gain_probe", params={"gain": drive()}, mask="subject")
    assert step.references() == [("mask", "subject", "selection"), ("by", "near", "selection")]
    assert Step("gain_probe", params={"gain": 2}).references() == []


def test_a_recipe_refuses_a_field_that_is_no_region_or_not_a_selection_naming_the_step_and_the_valid_ones(gain):
    with pytest.raises(
        RecipeError, match=r"step 1: `by` names 'near', which is no region; valid selections: subject, "
    ):
        Recipe([Step("gain_probe", params={"gain": drive()})], name="look", size=None)
    with pytest.raises(RecipeError, match=r"step 1: `by` names 'panes', a partition, but it needs a selection"):
        Recipe(
            [Step("gain_probe", params={"gain": drive(by="panes")})], name="look", regions={"panes": Grid()}, size=None
        )
    assert Recipe(
        [Step("gain_probe", params={"gain": drive(by="near")})], name="look", regions={"near": NEAR}, size=None
    )


def test_a_look_of_an_each_names_its_field_too(gain):
    look = Step("gain_probe", params={"gain": drive(by="near")})
    with pytest.raises(RecipeError, match=r"step 1: `by` names 'near', which is no region"):
        Recipe([Each("g", looks=[look, "none"])], name="look", regions={"g": Grid()}, size=None)


MAPS_OF_A_DRIVE = {
    "the depth": ("depth", {"depth"}),
    "a band of the depth": ("near", {"depth"}),
    "the subject": ("subject", {"subject"}),
    "the light of the picture needs no map": ("light", set()),
    "a given map": ("star", {"star"}),
}


@pytest.mark.parametrize(("by", "needed"), MAPS_OF_A_DRIVE.values(), ids=MAPS_OF_A_DRIVE.keys())
def test_a_recipe_needs_the_maps_its_drives_field_is_made_from(gain, by, needed):
    recipe = Recipe(
        [Step("gain_probe", params={"gain": drive(by=by)})],
        name="look",
        regions={"near": NEAR, "star": Given()},
        size=None,
    )
    assert recipe.maps_needed == frozenset(needed)
    assert recipe.derived_maps == (frozenset({"light"}) if by == "light" else frozenset())


# --- addresses: every number of a recipe has a name, and a copy of the recipe can have any of them set ----------------


@pytest.fixture
def pulse(gain):
    """A recipe with every kind of number: a named step, a driven one, an `Each` with a look, a band and a frame."""
    return Recipe(
        [
            Step("gain_probe", params={"gain": 2.0, "taps": 3}, name="pulse"),
            Step("gain_probe", params={"gain": Drive(by="near", start=0.5, end=2.5)}, opacity=0.5),
            Each("g", looks=[Step("gain_probe", params={"offset": 0.1}), "none"], feather=0.002),
        ],
        name="pulse",
        regions={"g": Grid(columns=3), "near": Band(of="depth", low=0.4)},
        frame=Framing(angle=15, aspect=1.5),
        size=256,
    )


PULSE_ADDRESSES = [
    "pulse.gain", "pulse.offset", "pulse.taps", "pulse.opacity",
    "gain_probe#2.gain.start", "gain_probe#2.gain.end", "gain_probe#2.gain.levels",
    "gain_probe#2.offset", "gain_probe#2.taps", "gain_probe#2.opacity",
    "each.feather", "each.opacity", "each.seed",
    "each.looks[0].gain", "each.looks[0].offset", "each.looks[0].taps", "each.looks[0].opacity",
    "region.near.low", "region.near.high", "region.near.feather",
]  # fmt: skip


def test_a_recipe_lists_every_number_in_it_by_address_in_the_order_of_the_recipe(pulse):
    listed = list(pulse.parameters())
    assert listed[: len(PULSE_ADDRESSES)] == PULSE_ADDRESSES  # steps, then looks, then regions
    assert [address for address in listed if address.startswith("frame.")][:2] == ["frame.angle", "frame.aspect"]
    assert listed[-1].startswith("frame.")  # the frame last
    assert "frame.fill" not in listed  # a colour is not a number
    assert "region.g.columns" not in listed  # nor is a list of them


def test_a_number_is_listed_with_its_value_unit_range_and_what_it_does(pulse):
    gain = pulse.parameters()["pulse.gain"]
    assert (gain.value, gain.unit, gain.minimum, gain.maximum, gain.whole) == (2.0, "ratio", 0, 4, False)
    assert gain.doc.startswith("Multiplies every value")
    taps = pulse.parameters()["pulse.taps"]
    assert (taps.value, taps.unit, taps.minimum, taps.maximum, taps.whole) == (3, "count", 1, 9, True)


def test_an_angle_that_wraps_is_listed_as_cyclic_with_the_turn_as_its_range_and_other_numbers_are_not(pulse):
    angle = pulse.parameters()["frame.angle"]
    assert (angle.cyclic, angle.unit, angle.minimum, angle.maximum) == (True, "degrees", -180, 180)
    assert not pulse.parameters()["pulse.gain"].cyclic
    assert not pulse.parameters()["frame.aspect"].cyclic


def test_a_cyclic_number_set_past_its_turn_is_the_same_direction_inside_it(pulse):
    assert pulse.with_values({"frame.angle": 725}).parameters()["frame.angle"].value == 5  # 725 - 2 * 360


def test_a_number_nobody_wrote_is_listed_at_its_default_and_one_that_was_written_at_what_was_written(pulse):
    listed = pulse.parameters()
    assert (listed["pulse.offset"].value, listed["pulse.gain"].value) == (0.0, 2.0)
    assert listed["region.near.low"].value == 0.4
    assert listed["region.near.high"].value == 1.0  # the default
    assert listed["frame.angle"].value == 15
    assert listed["each.feather"].value == 0.002


def test_a_driven_number_is_listed_by_its_two_ends_and_its_levels_not_by_itself(pulse):
    listed = pulse.parameters()
    assert "gain_probe#2.gain" not in listed
    start, end, levels = (listed[f"gain_probe#2.gain.{part}"] for part in ("start", "end", "levels"))
    assert (start.value, end.value, levels.value) == (0.5, 2.5, 5)
    assert (start.unit, start.minimum, start.maximum) == ("ratio", 0, 4)  # the ends have the range of the number itself
    assert (end.unit, end.minimum, end.maximum) == ("ratio", 0, 4)
    assert (levels.unit, levels.minimum, levels.maximum, levels.whole) == ("count", 2, 16, True)


def test_a_steps_opacity_is_a_number_of_the_step_at_its_own_or_its_effects_default(pulse):
    assert pulse.parameters()["gain_probe#2.opacity"].value == 0.5
    assert pulse.parameters()["pulse.opacity"].value == 1.0
    assert pulse.parameters()["pulse.opacity"].unit == "fraction"


def test_an_effect_the_recipe_uses_once_is_addressed_by_its_name_and_one_it_uses_twice_by_its_number(gain):
    once = Recipe(
        [Step("gain_probe", params={"gain": 2.0}), Step("gradient", params={"colors": ["#000000"]})],
        name="once",
        size=None,
    )
    assert "gain_probe.gain" in once.parameters()
    twice = Recipe(
        [Step("gain_probe"), Step("gradient", params={"colors": ["#000000"]}), Step("gain_probe")],
        name="twice",
        size=None,
    )
    assert "gain_probe#1.gain" in twice.parameters()
    assert "gain_probe#2.gain" in twice.parameters()
    assert "gain_probe.gain" not in twice.parameters()


def test_a_step_may_be_reached_by_its_name_by_its_effect_or_by_its_effect_and_number(pulse):
    for address in ("pulse.gain", "gain_probe#1.gain"):
        assert pulse.with_values({address: 3.0}).parameters()["pulse.gain"].value == 3.0
    assert pulse.with_values({"gain_probe#2.gain.end": 3.0}).parameters()["gain_probe#2.gain.end"].value == 3.0


WITH_VALUES = {
    "a number of a step": ("pulse.gain", 3.5, "pulse.gain"),
    "a whole number written as a float": ("pulse.taps", 4.0, "pulse.taps"),
    "the start of a drive": ("gain_probe#2.gain.start", 1.0, "gain_probe#2.gain.start"),
    "the end of a drive": ("gain_probe#2.gain.end", 3.0, "gain_probe#2.gain.end"),
    "the levels of a drive": ("gain_probe#2.gain.levels", 8, "gain_probe#2.gain.levels"),
    "the opacity of a step": ("gain_probe#2.opacity", 0.25, "gain_probe#2.opacity"),
    "the feather of an each": ("each.feather", 0.004, "each.feather"),
    "the opacity of an each": ("each.opacity", 0.5, "each.opacity"),
    "a number of a look": ("each.looks[0].offset", 0.3, "each.looks[0].offset"),
    "the opacity of a look": ("each.looks[0].opacity", 0.5, "each.looks[0].opacity"),
    "a number of a region": ("region.near.low", 0.2, "region.near.low"),
    "a number of the frame": ("frame.angle", -20, "frame.angle"),
}


@pytest.mark.parametrize(("address", "value", "listed"), WITH_VALUES.values(), ids=WITH_VALUES.keys())
def test_a_copy_of_a_recipe_has_the_number_at_an_address_set_and_nothing_else_changed(pulse, address, value, listed):
    before = pulse.parameters()
    after = pulse.with_values({address: value}).parameters()
    assert after[listed].value == value
    assert {a: p.value for a, p in after.items() if a != listed} == {
        a: p.value for a, p in before.items() if a != listed
    }


def test_the_recipe_that_was_set_is_left_as_it_was_and_what_was_not_touched_is_shared(pulse):
    before = pulse.parameters()
    after = pulse.with_values({"pulse.gain": 3.0})
    assert pulse.parameters() == before
    assert after is not pulse
    assert after.steps[1] is pulse.steps[1]  # a step that was not touched is the same object
    assert after.steps[2] is pulse.steps[2]
    assert after.regions["near"] is pulse.regions["near"]  # nor is a region that was not touched
    assert after.regions["g"] is pulse.regions["g"]
    assert (after.name, after.size, after.frame) == (pulse.name, pulse.size, pulse.frame)


def test_several_numbers_of_one_step_are_set_in_one_call(pulse):
    after = pulse.with_values({"pulse.gain": 1.5, "pulse.offset": 0.2, "pulse.opacity": 0.5})
    assert [after.parameters()[f"pulse.{name}"].value for name in ("gain", "offset", "opacity")] == [1.5, 0.2, 0.5]


def test_setting_nothing_gives_an_equal_recipe(pulse):
    assert pulse.with_values({}) == pulse


def test_a_recipe_with_a_number_set_renders_as_one_written_with_it(photo, gain, subject):
    written = Recipe(
        [Step("gain_probe", params={"gain": Drive(by="light", start=0.5, end=3.0)})], name="pulse", size=None
    )
    other = Recipe(
        [Step("gain_probe", params={"gain": Drive(by="light", start=0.5, end=2.0)})], name="pulse", size=None
    )
    np.testing.assert_array_equal(
        render(photo, other.with_values({"gain_probe.gain.end": 3.0})), render(photo, written)
    )


BAD_ADDRESSES = {
    "a step the recipe has not": (
        {"missing.gain": 1.0},
        r"'missing\.gain'.*no step, region or frame called 'missing'"
        r".*pulse, gain_probe#2, each, region\.g, region\.near, frame",
    ),
    "a number the step has not": (
        {"pulse.gian": 1.0},
        r"'pulse\.gian'.*'pulse' has no number 'gian'.*gain, offset, taps, opacity",
    ),
    "a number that is not a number": ({"pulse.mode": 1.0}, r"'pulse\.mode'.*no number 'mode'"),
    "a driven number by itself": ({"gain_probe#2.gain": 1.0}, r"'gain_probe#2\.gain'.*driven.*gain\.start.*gain\.end"),
    "the end of a number that is not driven": ({"pulse.gain.end": 1.0}, r"'pulse\.gain\.end'.*not driven"),
    "a region the recipe has not": ({"region.gone.low": 0.1}, r"'region\.gone\.low'.*no region 'gone'.*g, near"),
    "a number of a region it has not": (
        {"region.near.nope": 0.1},
        r"'region\.near\.nope'.*no number 'nope'.*low, high, feather",
    ),
    "a frame number it has not": ({"frame.nope": 0.1}, r"'frame\.nope'.*no number 'nope'.*angle"),
    "a number out of its range": ({"pulse.gain": 9.0}, r"pulse_?.*`gain`.*<= 4.*9"),
    "an end out of the range of the number": ({"gain_probe#2.gain.end": 9.0}, r"gain_probe.*`gain`.*<= 4.*9"),
    "levels of one": ({"gain_probe#2.gain.levels": 1}, r"levels"),
    "a whole number that is not": ({"pulse.taps": 2.5}, r"taps"),
    "an opacity past 1": ({"pulse.opacity": 1.5}, r"opacity"),
    "a frame angle that is not a number": ({"frame.angle": "tilted"}, r"angle"),
    "a region number out of range": ({"region.near.low": 1.5}, r"low"),
    "a value that is not a number": ({"pulse.gain": "high"}, r"gain"),
    "no dot at all": ({"pulse": 1.0}, r"'pulse'.*an address"),
}


@pytest.mark.parametrize(("values", "complaint"), BAD_ADDRESSES.values(), ids=BAD_ADDRESSES.keys())
def test_an_address_or_value_that_is_wrong_is_refused_naming_the_address_and_what_is_valid(pulse, values, complaint):
    with pytest.raises(RecipeError, match=complaint):
        pulse.with_values(values)


def test_a_step_that_has_a_name_can_still_be_reached_by_its_effect_when_it_is_the_only_one_of_it(gain):
    named = Recipe(
        [Step("gain_probe", name="pulse"), Step("gradient", params={"colors": ["#000000"]})], name="look", size=None
    )
    assert named.with_values({"gain_probe.gain": 1.5}).parameters()["pulse.gain"].value == 1.5


def test_two_addresses_that_reach_one_number_are_refused_naming_both(pulse):
    with pytest.raises(RecipeError, match=r"'pulse\.gain' and 'gain_probe#1\.gain'.*same number"):
        pulse.with_values({"pulse.gain": 1.0, "gain_probe#1.gain": 2.0})


def test_a_region_address_without_a_number_says_what_it_needs(pulse):
    with pytest.raises(RecipeError, match=r"'region\.near'.*region\.near\.number"):
        pulse.with_values({"region.near": 0.5})


def test_a_bad_value_blames_its_own_address_not_the_others_set_beside_it(pulse):
    with pytest.raises(RecipeError) as raised:
        pulse.with_values({"pulse.gain": 2.0, "pulse.offset": 9.0})
    assert "'pulse.offset'" in str(raised.value)
    assert "pulse.gain" not in str(raised.value)


def test_a_rule_between_two_numbers_that_neither_breaks_alone_names_both_addresses(throwaway_effects):
    class Pair(EffectParams):
        """Two numbers that must stay ordered."""

        low: float = param(0.2, unit="fraction", min=0, max=1, doc="The lower number.")
        high: float = param(0.8, unit="fraction", min=0, max=1, doc="The upper number, at least `low`.")

        @model_validator(mode="after")
        def _ordered(self) -> "Pair":
            """`low` is at most `high`."""
            if self.low > self.high:
                raise ValueError(f"low {self.low} is above high {self.high}")
            return self

    @effect(category="light", params=Pair)
    def pair_probe(rgb: np.ndarray, params: Pair) -> np.ndarray:
        """Nothing."""
        return rgb

    recipe = Recipe([Step("pair_probe")], name="look", size=None)
    with pytest.raises(RecipeError, match=r"'pair_probe\.low', 'pair_probe\.high'.*low 0\.9 is above high 0\.1"):
        recipe.with_values({"pair_probe.low": 0.9, "pair_probe.high": 0.1})


def test_an_each_has_its_seed_as_a_number_and_it_is_a_whole_one(pulse):
    seed = pulse.parameters()["each.seed"]
    assert (seed.value, seed.unit, seed.minimum, seed.whole) == (0, "count", 0, True)
    assert pulse.with_values({"each.seed": 7}).parameters()["each.seed"].value == 7


def test_numbers_from_numpy_are_taken_as_the_python_numbers_they_hold(pulse):
    after = pulse.with_values({"pulse.taps": np.float32(4.0), "pulse.gain": np.float32(0.5), "each.seed": np.int64(3)})
    numbers = after.parameters()
    assert (numbers["pulse.taps"].value, numbers["pulse.gain"].value, numbers["each.seed"].value) == (4, 0.5, 3)


def test_an_effect_the_recipe_uses_twice_cannot_be_named_without_its_number(gain):
    twice = Recipe(
        [Step("gain_probe"), Step("gradient", params={"colors": ["#000000"]}), Step("gain_probe")],
        name="twice",
        size=None,
    )
    with pytest.raises(
        RecipeError, match=r"'gain_probe\.gain'.*2 'gain_probe' steps.*gain_probe#1\.gain.*gain_probe#2\.gain"
    ):
        twice.with_values({"gain_probe.gain": 1.0})
    with pytest.raises(RecipeError, match=r"'gain_probe#3\.gain'.*no step"):
        twice.with_values({"gain_probe#3.gain": 1.0})
    with pytest.raises(RecipeError, match=r"'gain_probe#0\.gain'.*no step"):
        twice.with_values({"gain_probe#0.gain": 1.0})


def test_a_misspelt_address_suggests_the_nearest_ones(pulse):
    with pytest.raises(RecipeError, match=r"did you mean.*pulse\.gain"):
        pulse.with_values({"pulse.gaim": 1.0})


def test_one_parameter_can_be_asked_for_by_any_address_that_reaches_it(pulse):
    assert pulse.parameter("gain_probe#1.gain") == pulse.parameters()["pulse.gain"]
    with pytest.raises(RecipeError, match=r"'pulse\.gian'"):
        pulse.parameter("pulse.gian")


NAMES = {
    "letters, digits and underscores": ("pulse_2", None),
    "a hyphen": ("my-pulse", "name"),
    "a dot": ("my.pulse", "name"),
    "a hash": ("pulse#2", "name"),
    "a space": ("my pulse", "name"),
    "empty": ("", "name"),
    "a digit first": ("2pulse", "name"),
    "region, which starts an address": ("region", "reserved"),
    "frame, which starts an address": ("frame", "reserved"),
}


@pytest.mark.parametrize(("name", "complaint"), NAMES.values(), ids=NAMES.keys())
def test_a_step_name_is_a_word_that_cannot_be_taken_for_the_rest_of_an_address(gain, name, complaint):
    if complaint is None:
        assert Step("gain_probe", name=name).name == name
        return
    with pytest.raises(RecipeError, match=complaint):
        Step("gain_probe", name=name)


def test_two_steps_of_a_recipe_cannot_share_a_name_or_take_the_name_of_an_effect_it_uses(gain):
    with pytest.raises(RecipeError, match=r"'pulse'.*steps 1 and 2"):
        Recipe([Step("gain_probe", name="pulse"), Step("gain_probe", name="pulse")], name="look", size=None)
    with pytest.raises(RecipeError, match=r"'gain_probe'.*effect"):
        Recipe([Step("gain_probe", name="gain_probe")], name="look", size=None)
    with pytest.raises(RecipeError, match=r"'each'.*effect|reserved"):
        Recipe([Each("g", looks=["none"], name="each")], name="look", regions={"g": Grid()}, size=None)
    assert Recipe([Step("gain_probe", name="a"), Step("gain_probe", name="b")], name="look", size=None)


def test_a_step_named_describes_itself_with_its_name_and_an_each_too(gain):
    assert Step("gain_probe", params={"gain": 2}, name="pulse").describe() == "pulse: gain_probe(gain=2)"
    assert Each("g", looks=["none"], name="pieces").describe() == "pieces: each g [none]"
    assert Step("gain_probe").name is None


def test_a_step_name_is_a_step_key_so_fx_takes_it_and_no_effect_may_have_a_parameter_called_it(gain, throwaway_effects):
    assert fx.gain_probe(gain=2, name="pulse").name == "pulse"

    class Clash(EffectParams):
        """A parameter called like a step key."""

        name: str = param("x", doc="A name.")

    with pytest.raises(ValueError, match=r"`name` is supplied by the recipe"):
        effect(category="light", params=Clash)(lambda rgb, params: rgb)


def rich(**changed: float) -> Recipe:
    """A recipe with every kind of thing around its numbers (masks, blend modes, a partition, an each with an
    assignment and a seed, regions with tuples, presets and torn edges, a frame with a fill), with some numbers
    written as given."""
    return Recipe(
        [
            Step(
                "gain_probe",
                params={"gain": changed.get("gain", 2.0)},
                mask="near",
                opacity=0.5,
                blend_mode="screen",
                name="a",
            ),
            Step("seams", params={"width": changed.get("width", 0.5), "color": "#ff0000"}, partition="s"),
            Each(
                "s",
                looks=[
                    Step(
                        "gain_probe", params={"offset": changed.get("offset", 0.1)}, blend_mode="multiply", opacity=0.9
                    ),
                    "none",
                ],
                mask="near",
                opacity=0.8,
                blend_mode="overlay",
                assign="random",
                seed=7,
                feather=changed.get("feather", 0.003),
                name="p",
            ),
        ],
        name="rich",
        regions={
            "s": Shards(preset="glass", count=int(changed.get("count", 24)), edge=Edge(style="torn", amount=0.02)),
            "g": Grid(columns=[1, 2, 1], rows=3),
            "near": Band(of="depth", low=changed.get("low", 0.4), feather=0.2),
        },
        frame=Framing(angle=changed.get("angle", 10), fill="#102030", anchor="top", aspect=1.5, margin=0.1),
        size=256,
    )


RICH_CASES = {
    "a number of a step": ("a.gain", 3.0, {"gain": 3.0}),
    "a number of a step with a partition": ("seams.width", 1.0, {"width": 1.0}),
    "the feather of an each": ("p.feather", 0.004, {"feather": 0.004}),
    "a number of a look": ("p.looks[0].offset", 0.3, {"offset": 0.3}),
    "a whole number of a region with a preset and a torn edge": ("region.s.count", 40, {"count": 40}),
    "a number of a band": ("region.near.low", 0.1, {"low": 0.1}),
    "a number of the frame": ("frame.angle", 20, {"angle": 20}),
}


@pytest.mark.parametrize(("address", "value", "written"), RICH_CASES.values(), ids=RICH_CASES.keys())
def test_a_recipe_with_a_number_set_equals_the_recipe_written_with_it_so_nothing_around_it_is_lost(
    gain, address, value, written
):
    assert rich().with_values({address: value}) == rich(**written)


# --- copies with changes: `replace` takes what the constructor takes and checks it the same way ---------------------


def test_a_step_copy_has_the_values_and_step_keys_given_and_keeps_the_rest():
    step = fx.bloom(radius=8, threshold=0.4, mask="subject", opacity=0.7)
    assert step.replace(radius=20, blend_mode="screen") == fx.bloom(
        radius=20, threshold=0.4, mask="subject", opacity=0.7, blend_mode="screen"
    )
    assert step == fx.bloom(radius=8, threshold=0.4, mask="subject", opacity=0.7)  # the step itself is unchanged


def test_a_recipe_copy_has_the_fields_given_and_keeps_the_rest():
    recipe = Recipe([fx.grade(exposure=0.3)], name="warm", description="Warmer.", size=64)
    assert recipe.replace(name="warmer", size=128) == Recipe(
        [fx.grade(exposure=0.3)], name="warmer", description="Warmer.", size=128
    )
    assert recipe.name == "warm"  # the recipe itself is unchanged


@pytest.mark.parametrize(
    ("make", "error", "complaint"),
    [
        pytest.param(lambda: fx.bloom().replace(radius=-1), RecipeError, r"`radius`.*>= 0", id="a value out of range"),
        pytest.param(lambda: fx.bloom().replace(radiuss=1), RecipeError, r"`radiuss`.*radius", id="a misspelt value"),
        pytest.param(lambda: fx.bloom().replace(opacity=2), RecipeError, r"`opacity`", id="a bad step key"),
        pytest.param(lambda: fx.bloom().replace(effect="grain"), RecipeError, r"`effect`.*fx\.grain", id="the effect"),
        pytest.param(lambda: Recipe([fx.grade()]).replace(steps=[]), RecipeError, "at least one step", id="no steps"),
        pytest.param(lambda: Recipe([fx.grade()]).replace(size=0), RecipeError, r"`size`", id="a size below 1"),
        pytest.param(lambda: Recipe([fx.grade()]).replace(sise=64), RecipeError, r"`sise`", id="a misspelt field"),
    ],
)
def test_a_copy_with_a_bad_change_is_refused_as_making_it_so_would_be(make, error, complaint):
    with pytest.raises(error, match=complaint):
        make()


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


def test_a_recipe_without_a_size_has_none_and_renders_at_the_photos_own_size(photo):
    assert Recipe([Step("grade")], name="r").size is None
    assert render(photo, Recipe([Step("grade")], name="r")).shape == photo.shape
    assert render(photo, Recipe([Step("grade")], name="r", size=64)).shape == (48, 64, 3)


@pytest.mark.parametrize("size", [0, -5, 64.0, "64"], ids=["zero", "negative", "float", "text"])
def test_a_recipe_size_must_be_a_positive_whole_number(size):
    with pytest.raises(ValueError, match="size"):
        Recipe([Step("grade")], name="r", size=size)


@pytest.mark.parametrize("name", ["my look", "../../etc", "日本", "CON", "a" * 200, "look\n"])
def test_a_recipe_name_is_free_text_kept_as_written(name):
    assert Recipe([Step("grade")], name=name).name == name


def test_a_recipe_needs_no_name_or_description_and_keeps_the_ones_it_is_given():
    assert (Recipe([Step("grade")]).name, Recipe([Step("grade")]).description) == (None, None)
    described = Recipe([Step("grade")], name="warm", description="A warmer picture.")
    assert (described.name, described.description) == ("warm", "A warmer picture.")


@pytest.mark.parametrize(
    "make",
    [
        pytest.param(lambda: Recipe("warm", [Step("grade")]), id="the name first"),  # type: ignore[call-arg, arg-type]  # the point of the test
        pytest.param(lambda: Recipe([Step("grade")], "warm"), id="the name second"),  # type: ignore[call-arg]  # the point of the test
        pytest.param(lambda: Recipe([Step("grade")], None, 64), id="the size by position"),  # type: ignore[call-arg, arg-type]  # the point of the test
    ],
)
def test_a_recipe_takes_only_its_steps_by_position(make):
    with pytest.raises(TypeError, match="positional"):
        make()


def test_a_recipe_holds_the_framing_it_is_given_and_by_default_none():
    framing = Framing(aspect=1.0, anchor="subject")
    assert Recipe([Step("grade")], name="framed", frame=framing).frame == framing
    assert Recipe([Step("grade")], name="plain").frame == Framing()


def test_a_recipe_refuses_a_frame_that_is_not_a_framing():
    with pytest.raises(RecipeError, match="frame"):
        Recipe([Step("grade")], name="framed", frame="square")  # type: ignore[arg-type]  # the point of the test


@pytest.mark.parametrize(
    ("steps", "frame", "needs_subject"),
    [
        pytest.param([Step("grade")], Framing(), False, id="nothing masked, nothing framed"),
        pytest.param([Step("grade", mask="subject")], Framing(), True, id="a masked step"),
        pytest.param([Step("grade")], Framing(anchor="subject"), True, id="a crop around the subject"),
        pytest.param([Step("grade")], Framing(transparent="background"), True, id="the subject as the alpha"),
        pytest.param([Step("grade")], Framing(aspect=1.0, transparent="circle"), False, id="a crop and a circle"),
    ],
)
def test_a_recipe_needs_the_subject_when_a_step_or_its_framing_does(steps, frame, needs_subject):
    assert ("subject" in Recipe(steps, name="look", frame=frame).maps_needed) is needs_subject


@pytest.mark.parametrize(
    ("steps", "needs"),
    [
        pytest.param(lambda: [Step("depth_probe")], (False, True), id="a step whose effect takes depth"),
        pytest.param(
            lambda: [Each("g", looks=[Step("grade"), Step("depth_probe")])],
            (False, True),
            id="a look whose effect takes it",
        ),
        pytest.param(
            lambda: [Step("grade", mask="subject"), Each("g", looks=[Step("disperse"), "none"])],
            (True, False),
            id="steps and looks on the subject need no depth",
        ),
        pytest.param(lambda: [Step("depth_probe", mask="subject")], (True, True), id="a step that needs both"),
        pytest.param(lambda: [Each("g", looks=[Step("grade"), "none"])], (False, False), id="neither"),
        pytest.param(lambda: [Step("grade", mask="near")], (False, True), id="a mask made from depth"),
        pytest.param(lambda: [Each("bands", looks=["none"])], (False, True), id="an each over depth bands"),
        pytest.param(lambda: [Step("grade", mask="front")], (False, True), id="a pick of depth bands"),
        pytest.param(lambda: [Each("g", looks=[Step("grade", mask="near")])], (False, True), id="a look's mask"),
        pytest.param(lambda: [Step("grade")], (False, False), id="depth regions no step uses"),
    ],
)
def test_a_recipe_needs_depth_when_an_effect_of_a_step_or_a_look_or_a_region_it_uses_takes_it(
    depth_probe, steps, needs
):
    regions: dict[str, Kind] = {
        "g": Grid(),
        "near": Band(of="depth"),
        "bands": Bands(of="depth"),
        "front": Pick(of="bands"),
    }
    recipe = Recipe(steps(), name="look", regions=regions)
    assert ("subject" in recipe.maps_needed, "depth" in recipe.maps_needed) == needs


@pytest.mark.parametrize(
    ("steps", "regions", "frame", "expected"),
    [
        pytest.param([Step("grade")], {}, Framing(), set(), id="nothing"),
        pytest.param(
            [Step("grade", mask="star")], {"star": Given()}, Framing(), {"star"}, id="a mask that is a given map"
        ),
        pytest.param([Step("grade")], {"star": Given()}, Framing(), set(), id="a given map no step uses"),
        pytest.param(
            [Each("g", looks=[Step("grade", mask="star"), "none"])],
            {"g": Grid(), "star": Given()},
            Framing(),
            {"star"},
            id="a look's mask",
        ),
        pytest.param(
            [Step("grade", mask="star"), Step("grade", mask="depth"), Step("grade", mask="subject")],
            {"star": Given()},
            Framing(),
            {"star", "depth", "subject"},
            id="the built-in maps and a given one",
        ),
        pytest.param(
            [Step("grade")], {}, Framing(anchor="subject"), {"subject"}, id="a framing that crops on the subject"
        ),
    ],
)
def test_the_maps_a_recipe_needs_are_named(steps, regions, frame, expected):
    recipe = Recipe(steps, name="look", regions=regions, frame=frame)
    assert recipe.maps_needed == frozenset(expected)


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
    assert Step("gradient_map", params=EXPLICIT, preset="sepia").params == {
        **_shipped("gradient_map", "sepia"),
        **EXPLICIT,
    }


def test_a_given_parameter_wins_over_the_presets():
    sepia = _shipped("gradient_map", "sepia")
    chosen = ["#000000", "#ff0000"]
    step = Step("gradient_map", params={"stops": chosen}, preset="sepia")
    assert step.params["stops"] == chosen != sepia["stops"]


def test_a_preset_step_renders_as_the_step_with_the_same_parameters_written_out(photo):
    preset = Step("gradient_map", params=EXPLICIT, preset="sepia")
    written = Step("gradient_map", params={**_shipped("gradient_map", "sepia"), **EXPLICIT})
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
    first = Step("layer_probe", params={"image": Picture(pixels, "tex.png")})
    assert first == Step("layer_probe", params={"image": Picture(pixels.copy(), "other.png")})
    assert first != Step("layer_probe", params={"image": Picture(pixels[::-1].copy(), "tex.png")})
    assert first != Step("layer_probe", params={"image": "#808080"})
    assert first.describe().startswith("layer_probe(image=Picture('tex")
    assert len(first.describe()) <= 56


def test_an_effect_receives_the_picture_a_step_holds(layer_probe):
    picture = Picture(np.full((3, 3, 3), 0.25, np.float32))
    out = registry.apply("layer_probe", np.zeros((4, 4, 3), np.float32), image=picture)
    np.testing.assert_allclose(out, 0.25)
    assert Step("layer_probe", params={"image": picture}, mask="subject").params["image"] is picture


# --- regions: steps name them, the recipe declares them ---------------------------------------------------------------

REGIONS: dict[str, Kind] = {
    "panes": Grid(columns=3, rows=2),
    "glass": Shards(count=5),
    "head": Pick(of="panes", pieces=[1]),
}


def test_a_recipe_declaring_regions_accepts_steps_that_name_them_in_every_place_a_step_can():
    steps: list[Step | Each] = [
        Step("grade", mask="head"),
        Step("grade", mask="subject"),
        Each("panes", looks=[Step("grade", mask="head"), "none", Step("grade", opacity=0.5)], mask="background"),
    ]
    assert Recipe(steps, name="r", regions=REGIONS).regions == REGIONS
    assert Recipe([Step("grade")], name="plain").regions == {}


# Every way a step can name a region wrongly: each is refused when the recipe is made, naming the step (counted from 1),
# the key, the name given and what would have been valid.
BAD_REFERENCES = {
    "a mask no region has": (
        lambda: Recipe([Step("grade"), Step("grade", mask="nope")], name="r", regions=REGIONS),
        r"step 2: `mask` names 'nope', which is no region; valid selections: subject, background, depth, light, head",
    ),
    "a mask that is a partition": (
        lambda: Recipe([Step("grade", mask="panes")], name="r", regions=REGIONS),
        r"step 1: `mask` names 'panes', a partition, but it needs a selection; valid selections: .*head",
    ),
    "an each no region has": (
        lambda: Recipe([Each("nope", looks=["none"])], name="r", regions=REGIONS),
        r"step 1: `each` names 'nope', which is no region; valid partitions: panes, glass",
    ),
    "an each that is a selection": (
        lambda: Recipe([Each("head", looks=["none"])], name="r", regions=REGIONS),
        r"step 1: `each` names 'head', a selection, but it needs a partition; valid partitions: panes, glass",
    ),
    "an each that is the subject": (
        lambda: Recipe([Each("subject", looks=["none"])], name="r", regions=REGIONS),
        r"step 1: `each` names 'subject', a selection",
    ),
    "a mask on an each": (
        lambda: Recipe([Each("panes", looks=["none"], mask="glass")], name="r", regions=REGIONS),
        r"step 1: `mask` names 'glass', a partition",
    ),
    "a look with a mask no region has": (
        lambda: Recipe([Each("panes", looks=[Step("grade", mask="nope")])], name="r", regions=REGIONS),
        r"step 1: `mask` names 'nope'",
    ),
    "a partition no region has": (
        lambda: Recipe([Step("partition_probe", partition="nope")], name="r", regions=REGIONS),
        r"step 1: `partition` names 'nope', which is no region; "
        r"valid regions: subject, background, depth, light, panes, glass, head",
    ),
    "a recipe that declares no regions": (
        lambda: Recipe([Step("grade", mask="glass")], name="r"),
        r"valid selections: subject, background, depth, light .*declares no regions of its own.*regions=",
    ),
    "a region fitted to a partition": (
        lambda: Recipe([Step("grade")], name="r", regions={"a": Grid(), "b": Shards(fit="a")}),
        r"region 'b': `fit` names 'a', a partition, but it needs a selection",
    ),
    "regions in a cycle": (
        lambda: Recipe([Step("grade")], name="r", regions={"a": Shards(fit="b"), "b": Pick(of="a")}),
        r"cycle: 'a' -> 'b' -> 'a'",
    ),
    "a region called like a built-in": (
        lambda: Recipe([Step("grade")], name="r", regions={"subject": Grid()}),
        r"region 'subject' is a built-in",
    ),
}


@pytest.mark.parametrize(("make", "complaint"), BAD_REFERENCES.values(), ids=BAD_REFERENCES.keys())
def test_a_recipe_refuses_a_bad_region_reference_when_it_is_made(partition_probe, make, complaint):
    with pytest.raises(RecipeError, match=complaint):
        make()


BAD_EACH = {
    "no looks": (lambda: Each("panes", looks=[]), r"looks"),
    "a look that is a word other than none": (lambda: Each("panes", looks=["nothing"]), r"looks"),
    "a look that is a number": (lambda: Each("panes", looks=[3]), r"looks"),  # type: ignore[list-item]
    "an assignment that does not exist": (
        lambda: Each("panes", looks=["none"], assign="shuffle"),
        r"`assign`.*cycle.*random",
    ),
    "a negative seed": (lambda: Each("panes", looks=["none"], seed=-1), r"`seed`.*0"),
    "a feather below 0": (lambda: Each("panes", looks=["none"], feather=-0.005), r"`feather`.*0"),
    "an infinite feather": (lambda: Each("panes", looks=["none"], feather=float("inf")), r"`feather`"),
    "an unknown key": (lambda: Each("panes", looks=["none"], effect="grade"), r"`effect`"),
    "a mask that is not a name": (lambda: Each("panes", looks=["none"], mask=3), r"`mask`"),  # type: ignore[arg-type]
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
    ("steps", "regions", "needs_subject"),
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
            [Each("cut", looks=["none"])], {"cut": Shards(fit="subject")}, True, id="an each over shards on the subject"
        ),
        pytest.param([Each("cut", looks=["none"])], {"cut": Shards(fit="background")}, True, id="on the background"),
        pytest.param(
            [Each("cut", looks=["none"])],
            {"cut": Shards(fit="aura"), "aura": Pick(of="g"), "g": Grid(fit="subject")},
            True,
            id="through two regions",
        ),
        pytest.param(
            [Each("g", looks=[Step("disperse"), "none"])], {"g": Grid()}, True, id="a look that takes the subject"
        ),
        pytest.param([Each("g", looks=[Step("grade"), "none"])], {"g": Grid()}, False, id="looks that do not"),
        pytest.param(
            [Each("g", looks=["none"], mask="subject")], {"g": Grid()}, True, id="an each masked to the subject"
        ),
    ],
)
def test_a_recipe_needs_the_subject_only_when_a_region_it_uses_does(steps, regions, needs_subject):
    assert ("subject" in Recipe(steps, name="look", regions=regions).maps_needed) is needs_subject


FOUND: dict[str, Kind] = {"found": Subject(per="panes"), "whole": Subject()}


@pytest.mark.parametrize(
    ("steps", "regions", "needs_subject", "needs_finder"),
    [
        pytest.param([Step("grade", mask="found")], FOUND, False, True, id="a mask from the subject found by pieces"),
        pytest.param([Step("grade", mask="whole")], FOUND, True, False, id="the subject without per is the subject"),
        pytest.param([Step("grade", mask="head")], REGIONS, False, False, id="a mask from a grid"),
        pytest.param([Step("grade", mask="subject")], FOUND, True, False, id="the built-in subject"),
        pytest.param([Step("grade")], FOUND, False, False, id="found by pieces but no step uses it"),
        pytest.param(
            [Each("cut", looks=["none"])],
            {**FOUND, "cut": Shards(fit="found")},
            False,
            True,
            id="shards laid over that find",
        ),
        pytest.param([Step("grade", mask="found"), Step("grade", mask="subject")], FOUND, True, True, id="both"),
    ],
)
def test_a_recipe_needs_a_finder_for_pieces_only_when_a_region_it_uses_finds_the_subject_by_pieces(
    steps, regions, needs_subject, needs_finder
):
    recipe = Recipe(steps, name="look", regions={**REGIONS, **regions})
    assert ("subject" in recipe.maps_needed, recipe.needs_finder) == (needs_subject, needs_finder)


def test_an_each_step_describes_itself_with_its_partition_and_looks():
    step = Each("panes", looks=[Step("grade", params={"exposure": 0.5}), "none"], mask="subject", opacity=0.5)
    assert step.describe() == "each panes [grade(exposure=0.5), none] on subject x0.5"
    assert len(Each("panes", looks=[Step("ascii", params={"chars": "xy" * 50})] * 3).describe(width=30)) == 30


def test_an_each_step_shows_fully_unless_it_gives_an_opacity():
    assert Each("panes", looks=[Step("grade")]).opacity == 1.0
    assert Each("panes", looks=[Step("grade")], opacity=0.3).opacity == 0.3


def test_the_each_step_is_a_frozen_value_and_keeps_its_looks_as_a_tuple():
    step = Each("panes", looks=[Step("grade"), "none"])
    assert step.looks == (Step("grade"), "none")
    with pytest.raises(ValueError, match="frozen"):
        step.feather = 3.0  # type: ignore[misc]  # the point of the test
