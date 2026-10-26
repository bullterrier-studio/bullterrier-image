"""Tests of `core/runner.py`: the runner is given the subject (a matte, or a `Finder` that finds it); it never looks
for the subject itself."""

import math
import tracemalloc
from contextlib import nullcontext

import numpy as np
import pytest

from bullterrier_image import RecipeError, SubjectNotFoundError
from bullterrier_image.core import runner
from bullterrier_image.core.framing import Framing
from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.core.steps import Each, Recipe, Step
from bullterrier_image.ops.resample import resize
from bullterrier_image.recipe import Finder, Grid, Pick, Shards, Subject

MASKED = Recipe("masked", (Step("fill", {"color": "#000000"}, "background"),), size=None)
PLAIN = Recipe("plain", (Step("grade", {"exposure": 0.5}),), size=None)


class CountingFinder:
    """A stand-in subject finder: records the photos it was asked about and returns a fixed matte."""

    def __init__(self, matte: np.ndarray):
        self.matte = matte
        self.seen: list[tuple[int, ...]] = []

    def __call__(self, rgb: np.ndarray) -> np.ndarray:
        self.seen.append(rgb.shape)
        return self.matte


@pytest.mark.parametrize(
    ("identity", "find", "outcome"),
    [
        pytest.param("stand-in v1", np.mean, nullcontext(), id="a named function is a finder"),
        pytest.param("", np.mean, pytest.raises(RecipeError, match="identity"), id="no identity is refused"),
        pytest.param(" \t", np.mean, pytest.raises(RecipeError, match="identity"), id="a blank identity is refused"),
        pytest.param(None, np.mean, pytest.raises(RecipeError, match="identity"), id="an identity not text is refused"),
        pytest.param(
            "stand-in v1", 3, pytest.raises(RecipeError, match="find must"), id="a find not callable is refused"
        ),
    ],
)
def test_a_finder_needs_an_identity_and_a_function(identity, find, outcome):
    with outcome:
        Finder(identity, find)


RUNS = {
    "render": lambda photo, recipe, **k: runner.render(photo, recipe, **k),
    "trace": lambda photo, recipe, **k: runner.trace(photo, recipe, **k),
    "render_all": lambda photo, recipe, **k: runner.render_all(photo, [PLAIN, recipe], **k),
}


@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_a_masked_recipe_without_a_matte_or_a_finder_is_refused_naming_both_ways(run, photo):
    with pytest.raises(SubjectNotFoundError, match="needs the subject") as raised:
        run(photo, MASKED)
    message = str(raised.value)  # names the recipe and both ways to give the subject
    assert f"'{MASKED.name}'" in message
    assert "`matte`" in message
    assert "`find_subject`" in message


@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_the_finder_is_called_exactly_once_per_render(run, photo, matte):
    finder = CountingFinder(matte)
    run(photo, MASKED, find_subject=Finder("stand-in", finder))
    assert len(finder.seen) == 1  # once for a render, a trace, or all the recipes of render_all


def test_render_all_asks_once_however_many_recipes_need_the_subject(photo, matte):
    finder = CountingFinder(matte)
    assert len(runner.render_all(photo, [MASKED, PLAIN, MASKED], find_subject=Finder("stand-in", finder))) == 3
    assert len(finder.seen) == 1


def test_the_finder_is_not_called_when_nothing_needs_the_subject_or_the_matte_is_given(photo, matte):
    finder = CountingFinder(matte)
    runner.render(photo, PLAIN, find_subject=Finder("stand-in", finder))
    runner.render_all(photo, [PLAIN, PLAIN], find_subject=Finder("stand-in", finder))
    runner.render(photo, MASKED, matte=matte, find_subject=Finder("stand-in", finder))
    assert finder.seen == []


def test_the_finders_matte_is_the_one_the_recipe_uses(photo, matte):
    found = runner.render(photo, MASKED, find_subject=Finder("stand-in", CountingFinder(matte)))
    np.testing.assert_array_equal(found, runner.render(photo, MASKED, matte=matte))
    assert not np.array_equal(found, runner.render(photo, MASKED, matte=1.0 - matte))


def test_the_finder_sees_the_photo_as_given_before_it_is_framed_or_scaled(photo, matte):
    finder = CountingFinder(matte)
    small = Recipe("small", MASKED.steps, size=64, frame=Framing(aspect=1.0))
    assert runner.render(photo, small, find_subject=Finder("stand-in", finder)).shape == (64, 64, 3)
    assert finder.seen == [(96, 128, 3)]  # not the 64 px square it is rendered as


def test_a_finders_matte_of_another_size_than_the_photo_is_refused(photo, matte):
    with pytest.raises(SubjectNotFoundError, match="matte is"):
        runner.render(photo, MASKED, find_subject=Finder("stand-in", CountingFinder(matte[::2, ::2])))


@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_a_finder_that_finds_nothing_is_an_error_naming_it(run, photo):
    """A finder returning None (instead of raising) would otherwise make `trace` drop the subject stage silently."""
    broken = Finder("stand-in", lambda rgb: None)  # type: ignore[arg-type,return-value]  # broken on purpose
    with pytest.raises(SubjectNotFoundError, match="`find_subject` returned None"):
        run(photo, MASKED, find_subject=broken)


# --- framing: the photo and its matte are framed together, before the steps -------------------------------------------


def test_a_recipe_without_framing_runs_its_steps_on_the_photo_scaled_to_its_size(photo):
    scaled = Recipe("scaled", PLAIN.steps, size=64)
    by_hand = runner.render(resize(photo, 64), PLAIN)
    np.testing.assert_array_equal(runner.render(photo, scaled), by_hand)


@pytest.mark.parametrize(
    ("frame", "by_hand"),
    [
        pytest.param(Framing(aspect=1.0), lambda a: a[:, 16:112], id="a square crop"),
        pytest.param(Framing(aspect=1.0, anchor="left"), lambda a: a[:, :96], id="a square crop at the left"),
        pytest.param(Framing(flip="x"), lambda a: a[:, ::-1], id="a flip"),
        pytest.param(Framing(angle=90, fit="expand"), lambda a: np.rot90(a), id="a quarter turn"),
    ],
)
def test_the_steps_run_on_the_framed_photo_and_a_masked_step_still_finds_the_subject_where_it_moved(
    photo, matte, frame, by_hand
):
    framed = Recipe("framed", MASKED.steps, size=None, frame=frame)
    expected = runner.render(by_hand(photo), MASKED, matte=by_hand(matte))  # background black, by hand
    np.testing.assert_array_equal(runner.render(photo, framed, matte=matte), expected)
    assert (expected == 0).any()
    assert np.array_equal(
        runner.render(photo, framed, matte=matte)[by_hand(matte) == 1], by_hand(photo)[by_hand(matte) == 1]
    )


def test_framing_comes_before_the_size(photo):
    framed = Recipe("framed", PLAIN.steps, size=48, frame=Framing(aspect=1.0))
    by_hand = runner.render(resize(photo[:, 16:112], 48), PLAIN)  # crop 96 px square, then scale it to 48
    np.testing.assert_array_equal(runner.render(photo, framed), by_hand)


def test_a_crop_around_the_subject_uses_the_matte_that_the_finder_found_on_the_whole_photo(photo, matte):
    framed = Recipe("framed", PLAIN.steps, size=None, frame=Framing(anchor="subject", aspect=1.0, margin=0.1))
    for result in (
        runner.render(photo, framed, matte=matte),
        runner.render(photo, framed, find_subject=Finder("stand-in", CountingFinder(matte))),
    ):
        np.testing.assert_array_equal(result, runner.render(photo[13:84, 29:100], PLAIN))  # the disc and 6 px each way


def test_a_subject_crop_needs_the_subject_even_when_no_step_does(photo):
    framed = Recipe("framed", PLAIN.steps, size=None, frame=Framing(anchor="subject"))
    with pytest.raises(SubjectNotFoundError, match="needs the subject"):
        runner.render(photo, framed)


def test_trace_shows_the_framed_photo_and_matte_and_ends_in_the_render(photo, matte):
    framed = Recipe("framed", MASKED.steps, size=None, frame=Framing(aspect=1.0))
    stages = runner.trace(photo, framed, matte=matte)
    np.testing.assert_array_equal(stages[0].image, photo[:, 16:112])
    np.testing.assert_array_equal(stages[1].image[..., 0], matte[:, 16:112])
    np.testing.assert_array_equal(stages[-1].image, runner.render(photo, framed, matte=matte))


def test_render_all_finds_the_subject_once_for_framed_and_plain_recipes(photo, matte):
    finder = CountingFinder(matte)
    framed = Recipe("framed", PLAIN.steps, size=None, frame=Framing(anchor="subject"))
    results = runner.render_all(photo, [PLAIN, framed], find_subject=Finder("stand-in", finder))
    assert [r.shape[:2] for r in results] == [(96, 128), (59, 59)]
    assert len(finder.seen) == 1


# --- transparency: the picture is cut out after the last step -------------------------------------------------------


def test_a_circle_cut_gives_four_channels_the_steps_result_and_the_inscribed_circle_as_alpha(photo):
    recipe = Recipe("round", PLAIN.steps, size=None, frame=Framing(transparent="circle"))
    out = runner.render(photo, recipe)
    assert out.shape == (96, 128, 4)
    np.testing.assert_array_equal(out[..., :3], runner.render(photo, PLAIN))
    ys, xs = np.mgrid[0:96, 0:128]
    distance = np.hypot(ys + 0.5 - 48, xs + 0.5 - 64)  # from the middle; the circle's radius is 48
    assert (out[..., 3][distance < 47] == 1).all()
    assert (out[..., 3][distance > 49] == 0).all()


def test_the_alpha_of_a_background_cut_is_the_subject_matte_that_moved_with_the_photo(photo, matte):
    recipe = Recipe("cut", PLAIN.steps, size=None, frame=Framing(transparent="background", flip="x", aspect=1.0))
    out = runner.render(photo, recipe, matte=matte)
    np.testing.assert_array_equal(out[..., 3], matte[:, ::-1][:, 16:112])
    np.testing.assert_array_equal(out[..., :3], runner.render(photo[:, ::-1][:, 16:112], PLAIN))


def test_the_alpha_of_rounded_corners_is_clear_in_the_corners_and_opaque_in_the_middle_of_each_side(photo):
    recipe = Recipe("rounded", PLAIN.steps, size=None, frame=Framing(transparent="rounded", corner_radius=0.25))
    alpha = runner.render(photo, recipe)[..., 3]  # a corner radius of a quarter of the 96 px side: 24 px
    assert [alpha[0, 0], alpha[0, -1], alpha[-1, 0], alpha[-1, -1]] == [0, 0, 0, 0]
    assert [alpha[0, 64], alpha[-1, 64], alpha[48, 0], alpha[48, -1]] == [1, 1, 1, 1]


def test_the_cut_is_made_at_the_recipes_size_after_the_steps(photo, matte):
    recipe = Recipe("cut", PLAIN.steps, size=64, frame=Framing(transparent="background"))
    out = runner.render(photo, recipe, matte=matte)
    assert out.shape == (48, 64, 4)
    np.testing.assert_array_equal(out[..., 3], resize(matte, 64))


def test_a_cut_around_the_subject_needs_the_subject_even_when_no_step_does(photo):
    recipe = Recipe("cut", PLAIN.steps, size=None, frame=Framing(transparent="background"))
    with pytest.raises(SubjectNotFoundError, match="needs the subject"):
        runner.render(photo, recipe)


def test_trace_adds_the_cut_as_its_last_stage_and_it_is_what_render_returns(photo, matte):
    recipe = Recipe("cut", MASKED.steps, size=None, frame=Framing(transparent="circle"))
    stages = runner.trace(photo, recipe, matte=matte)
    assert [stage.image.shape[2] for stage in stages] == [3, 3, 3, 4]  # input, matte, the step, the cut
    assert stages[-1].label == "2. transparent('circle')"
    np.testing.assert_array_equal(stages[-1].image, runner.render(photo, recipe, matte=matte))
    np.testing.assert_array_equal(stages[-1].image[..., :3], stages[-2].image)


def test_render_all_cuts_only_the_recipes_that_ask_for_it(photo):
    round_ = Recipe("round", PLAIN.steps, size=None, frame=Framing(transparent="circle"))
    results = runner.render_all(photo, [PLAIN, round_])
    assert [r.shape[2] for r in results] == [3, 4]


# --- memory: the runner keeps only the image the next step needs ---------------------------------------------------


class ScratchParams(EffectParams):
    """The memory probes' settings."""

    copies: int = param(4, unit="count", min=1, max=8, doc="Picture-sized scratch buffers the step fills as it works.")


SCRATCH_COPIES = 4  # each scratch step holds this many pictures, plus its output, at its peak


def peak_pictures(photo: np.ndarray, recipe: Recipe, matte: np.ndarray | None = None) -> float:
    """The most memory `render` had allocated at once (beyond the photo and matte, which the caller holds), in pictures
    of the working size."""
    picture = runner.render(photo, recipe, matte=matte).nbytes  # also warms up: one-time allocations stay out
    tracemalloc.start()
    try:
        runner.render(photo, recipe, matte=matte)
        return tracemalloc.get_traced_memory()[1] / picture
    finally:
        tracemalloc.stop()


@pytest.fixture
def memory_probes(throwaway_effects):
    """`scratch_probe` works in `copies` picture-sized buffers and returns a new picture; `hoarding_probe` returns a new
    picture but keeps every picture it is given alive, the way a leaky runner would."""
    hoard: list[np.ndarray] = []

    @effect(category="utilities", params=ScratchParams)
    def scratch_probe(rgb, params):
        """Fills scratch pictures, then returns a copy of the input."""
        scratch = np.repeat(rgb[None], params.copies, axis=0)
        return scratch[-1].copy()

    @effect(category="utilities", params=ScratchParams)
    def hoarding_probe(rgb, params):
        """Keeps its input alive and returns a copy of it."""
        hoard.append(rgb)
        return rgb.copy()

    yield
    hoard.clear()


PHOTO = np.random.default_rng(3).random((210, 330, 3), dtype=np.float32)  # scaled to 320 px: a new picture
SUBJECT = np.zeros((210, 330), np.float32)
SUBJECT[50:150, 80:250] = 1.0


@pytest.mark.parametrize(
    ("mask", "matte", "kept"),
    [
        (None, None, 0.0),
        ("subject", SUBJECT, 1 / 3),  # the scaled matte, one channel: a third of a picture, needed by every step
        ("background", SUBJECT, 2 / 3),  # and the background selection made from it, kept for the steps that mask
    ],
    ids=["plain", "subject", "background"],
)
def test_a_linear_render_holds_no_more_than_the_working_step_its_input_and_output(memory_probes, mask, matte, kept):
    """A step's input is released once the next step has run, and neither the scaled photo nor a masked step's blend
    weights are kept for the rest of the chain: the peak is the busiest step's scratch, output and input (and the
    scaled matte), whatever the number of steps."""
    steps = tuple(Step("scratch_probe", {"copies": SCRATCH_COPIES}, mask) for _ in range(6))
    bound = (
        SCRATCH_COPIES + 1 + 1 + kept + 0.25
    )  # scratch, output, input, the selections, a quarter picture for small things
    assert peak_pictures(PHOTO, Recipe("chain", steps, size=320), matte) <= bound
    # known-bad: the same gauge sees a picture that something keeps alive
    leaky = (*steps[:3], Step("hoarding_probe", {}, mask), *steps[3:])
    assert peak_pictures(PHOTO, Recipe("leaky", leaky, size=320), matte) > bound


# --- regions: `each` gives every piece of a partition its look; `mask` limits a step to a selection -----------------

RED, GREEN, BLUE, WHITE = "#ff0000", "#00ff00", "#0000ff", "#ffffff"
RGB = {RED: (1.0, 0.0, 0.0), GREEN: (0.0, 1.0, 0.0), BLUE: (0.0, 0.0, 1.0), WHITE: (1.0, 1.0, 1.0)}


def fill(color: str, **keys) -> Step:
    return Step("fill", {"color": color}, **keys)


def columns(count: int) -> dict[str, Grid | Pick | Shards]:
    return {"cols": Grid(columns=count, rows=1)}


def run(photo, steps, regions, **keys):
    return runner.render(photo, Recipe("regions", steps, size=None, regions=regions), **keys)


@pytest.mark.parametrize("count", [1, 2, 4, 5])
def test_each_piece_shows_its_look_and_the_looks_take_turns(photo, count):
    looks = [fill(RED), fill(GREEN), fill(BLUE)]
    out = run(photo, [Each("cols", looks, feather=0)], columns(count))
    width = photo.shape[1]
    for x in range(width):
        column = int((x + 0.5) * count / width)  # the piece holding the pixel's centre, worked out by hand
        want = RGB[(RED, GREEN, BLUE)[column % 3]]
        assert (out[:, x] == want).all(), (count, x)


@pytest.mark.parametrize("size", [(1, 1), (1, 7), (7, 1), (13, 29), (61, 37)])
@pytest.mark.parametrize("feather", [0, 0.5 / 128, 3 / 128])  # shares of the 128 px long side
@pytest.mark.parametrize("assign", ["cycle", "random"])
def test_the_looks_weights_add_up_to_one_at_every_pixel_whatever_the_size_and_feather(size, feather, assign):
    photo = np.random.default_rng(2).random((*size, 3), dtype=np.float32)
    pure = [
        fill(RED),
        fill(GREEN),
        fill(BLUE),
    ]  # a pure look is its own weight in its channel, so the channels add up to 1
    out = run(photo, [Each("cols", pure, assign=assign, feather=feather)], columns(5))
    assert out.shape == photo.shape
    assert out.min() >= 0
    assert out.max() <= 1
    np.testing.assert_allclose(out.sum(axis=2), 1.0, atol=1e-5)


def test_a_none_look_leaves_its_pieces_exactly_as_they_were(photo):
    out = run(photo, [Each("cols", [fill(RED), "none"], feather=0)], columns(4))
    np.testing.assert_array_equal(out[:, 32:64], photo[:, 32:64])
    np.testing.assert_array_equal(out[:, 96:], photo[:, 96:])
    assert (out[:, 0:32] == RGB[RED]).all()


def test_a_none_look_is_exact_away_from_the_feathered_borders(photo):
    out = run(photo, [Each("cols", [fill(RED), "none"], feather=2 / 128)], columns(4))
    reach = 4 * 2 + 1  # a feather of 2 px, cut off at 4 sigmas
    np.testing.assert_array_equal(out[:, 32 + reach : 64 - reach], photo[:, 32 + reach : 64 - reach])
    assert (out[:, 0 : 32 - reach] == RGB[RED]).all()
    border = out[:, 31, 1]  # the green of the last column of the red piece: red has none, the photo some
    assert (border > 0).all()
    assert (border < photo[:, 31, 1]).all()  # a mix of the two looks, not either one


@pytest.mark.parametrize("feather", [0, 3 / 128])
@pytest.mark.parametrize("assign", ["cycle", "random"])
def test_looks_that_change_nothing_give_back_the_photo_exactly(photo, feather, assign):
    nothing: list[Step | str] = [fill(RED, opacity=0.0), fill(GREEN, opacity=0.0), "none"]
    np.testing.assert_array_equal(
        run(photo, [Each("cols", nothing, assign=assign, feather=feather)], columns(7)), photo
    )


def test_a_look_with_its_own_opacity_and_blend_mode_is_laid_over_the_photo_like_a_step(photo):
    looks = [fill("#808080", blend_mode="multiply"), fill(WHITE, opacity=0.25)]
    out = run(photo, [Each("cols", looks, feather=0)], columns(2))
    np.testing.assert_allclose(out[:, :64], photo[:, :64] * (128 / 255), atol=1e-6)
    np.testing.assert_allclose(out[:, 64:], photo[:, 64:] * 0.75 + 0.25, atol=1e-6)


def test_a_look_takes_its_effects_default_opacity_unless_it_gives_one(photo, throwaway_effects):
    @effect(category="utilities", params=ScratchParams, opacity=0.4)
    def faint_white(rgb, params):
        """Paints white, shown at 0.4 unless a step says otherwise."""
        return np.ones_like(rgb)

    out = run(photo, [Each("cols", [Step("faint_white"), Step("faint_white", opacity=1.0)], feather=0)], columns(2))
    np.testing.assert_allclose(out[:, :64], photo[:, :64] * 0.6 + 0.4, atol=1e-6)
    assert (out[:, 64:] == 1).all()


def test_the_each_steps_own_opacity_and_blend_mode_apply_to_the_blended_looks(photo):
    looks = [fill(RED), fill(GREEN)]
    half = run(photo, [Each("cols", looks, feather=0, opacity=0.5)], columns(2))
    np.testing.assert_allclose(half[:, :64], photo[:, :64] * 0.5 + 0.5 * np.array(RGB[RED], np.float32), atol=1e-6)
    np.testing.assert_allclose(half[:, 64:], photo[:, 64:] * 0.5 + 0.5 * np.array(RGB[GREEN], np.float32), atol=1e-6)
    multiplied = run(photo, [Each("cols", looks, feather=0, blend_mode="multiply")], columns(2))
    np.testing.assert_allclose(multiplied[:, :64], photo[:, :64] * np.array(RGB[RED], np.float32), atol=1e-6)


HEAD: dict[str, Grid | Pick | Shards] = {
    "panes": Grid(columns=2, rows=2),
    "head": Pick(of="panes", pieces=[1]),
}  # the top right quarter


@pytest.mark.parametrize(
    "step",
    [
        pytest.param(fill(RED, mask="head"), id="a step"),
        pytest.param(Each("panes", [fill(RED)], mask="head"), id="an each step"),
    ],
)
def test_a_step_masked_by_a_region_changes_only_the_pixels_the_region_chooses(photo, step):
    out = run(photo, [step], HEAD)
    assert (out[:48, 64:] == RGB[RED]).all()
    np.testing.assert_array_equal(out[:48, :64], photo[:48, :64])
    np.testing.assert_array_equal(out[48:], photo[48:])


def test_a_mask_can_name_a_region_made_from_the_subject(photo, matte):
    regions = {"around": Shards(count=6, fit="subject"), "piece": Pick(of="around", pieces=[0])}
    out = run(photo, [fill(RED, mask="piece")], regions, matte=matte)
    changed = (out != photo).any(axis=2)
    assert changed.any()
    assert not changed.all()  # one shard of six


class Stamps:
    """The renders so far of the `stamp` effect, which paints its own number into the picture."""

    count = 0


@pytest.fixture
def stamp(throwaway_effects):
    Stamps.count = 0

    @effect(category="utilities", params=ScratchParams)
    def stamp(rgb, params):
        """Paints a flat grey of 0.01 per render so far, counting this one."""
        Stamps.count += 1
        return np.full_like(rgb, Stamps.count / 100)


def test_every_look_is_rendered_once_over_the_whole_picture_however_many_pieces_it_has(photo, stamp):
    twice = Step("stamp", {"copies": 2})
    out = run(photo, [Each("cols", [Step("stamp"), twice, "none"], feather=0), Step("stamp")], columns(30))
    # 30 pieces, 3 looks, one a "none": two renders (0.01 and 0.02) and the step after them is the third
    assert set(np.unique(out)) == {np.float32(0.03)}


def test_a_look_no_piece_has_is_not_rendered(photo, stamp):
    out = run(
        photo, [Each("cols", [Step("stamp"), Step("stamp"), Step("stamp")], feather=0), Step("stamp")], columns(2)
    )
    assert set(np.unique(out)) == {np.float32(0.03)}  # two pieces use two looks; the third would have made it 0.04


def test_the_random_assignment_is_decided_by_the_seed_alone(photo):
    looks = [fill(RED), fill(GREEN), fill(BLUE)]

    def render_with(seed: int) -> np.ndarray:
        return run(photo, [Each("cols", looks, assign="random", seed=seed, feather=0)], columns(40))

    np.testing.assert_array_equal(render_with(3), render_with(3))
    assert not np.array_equal(render_with(3), render_with(4))
    chosen = {tuple(pixel) for pixel in render_with(3).reshape(-1, 3)}
    assert chosen == set(RGB.values()) - {RGB[WHITE]}  # all three looks are used, and nothing else appears
    cycle = run(photo, [Each("cols", looks, feather=0)], columns(40))
    assert not np.array_equal(render_with(3), cycle)


def test_regions_are_made_for_the_working_picture_after_framing_and_scaling(photo):
    recipe = Recipe(
        "framed",
        [Each("halves", [fill(RED), fill(GREEN)], feather=0)],
        size=64,
        frame=Framing(aspect=1.0),
        regions={"halves": Grid(columns=2, rows=1)},
    )
    out = runner.render(photo, recipe)
    assert out.shape == (64, 64, 3)
    assert (out[:, :32] == RGB[RED]).all()
    assert (out[:, 32:] == RGB[GREEN]).all()


# --- regions and the subject: found at most once, and only when a region in use needs it ------------------------------


def test_a_declared_region_nobody_uses_costs_nothing_and_does_not_look_for_the_subject(photo, matte):
    finder = CountingFinder(matte)
    unused = {"glass": Shards(fit="subject"), "head": Pick(of="glass")}
    out = run(
        photo,
        [fill(RED, mask="head")] if False else [Step("grade", {"exposure": 0.5})],
        unused,
        find_subject=Finder("stand-in", finder),
    )
    assert finder.seen == []
    np.testing.assert_array_equal(out, run(photo, [Step("grade", {"exposure": 0.5})], {}))


def test_a_region_in_use_asks_for_the_subject_once_however_many_steps_use_it(photo, matte):
    finder = CountingFinder(matte)
    regions = {"glass": Shards(count=5, fit="subject"), "head": Pick(of="glass", pieces=[0])}
    steps = [fill(RED, mask="head"), Each("glass", [fill(GREEN), "none"]), fill(BLUE, mask="subject")]
    run(photo, steps, regions, find_subject=Finder("stand-in", finder))
    assert finder.seen == [(96, 128, 3)]  # the photo as given


def test_a_region_laid_over_the_subject_follows_the_subject_scaled_to_the_working_picture(photo, matte):
    recipe = Recipe(
        "scaled",
        [Each("sides", [fill(RED), fill(GREEN)], feather=0)],
        size=64,
        regions={"sides": Grid(columns=2, rows=1, fit="subject")},
    )
    out = runner.render(photo, recipe, find_subject=Finder("stand-in", CountingFinder(matte)))
    assert out.shape == (48, 64, 3)
    middle = 32  # the disc is in the middle of the picture, so its box is cut in half there
    assert (out[:, :middle] == RGB[RED]).all()
    assert (out[:, middle:] == RGB[GREEN]).all()


@pytest.mark.parametrize("run_it", RUNS.values(), ids=RUNS.keys())
def test_a_region_that_needs_the_subject_without_a_finder_is_refused_like_a_masked_step(run_it, photo):
    recipe = Recipe("fitted", [Each("glass", ["none"])], size=None, regions={"glass": Shards(fit="subject")})
    with pytest.raises(SubjectNotFoundError, match="needs the subject"):
        run_it(photo, recipe)


# --- the partition an effect draws ---------------------------------------------------------------------------------


def test_an_effect_that_takes_a_partition_gets_the_pieces_numbered_in_reading_order(photo, partition_probe):
    out = run(photo, [Step("partition_probe", partition="panes")], {"panes": Grid(columns=3, rows=2)})
    height, width = photo.shape[:2]
    for y in range(height):
        for x in range(width):
            want = int((y + 0.5) * 2 / height) * 3 + int((x + 0.5) * 3 / width)  # row after row, by hand
            assert out[y, x, 0] * 255 == pytest.approx(want), (x, y)


def test_a_selection_is_given_to_an_effect_as_a_partition_of_two_pieces(photo, matte, partition_probe):
    out = run(photo, [Step("partition_probe", partition="subject")], {}, matte=matte)
    np.testing.assert_allclose(out[..., 0] * 255, matte > 0.5)  # outside the subject first: it holds the first pixel


# --- trace: the regions a recipe uses, with the pieces numbered ----------------------------------------------------

TRACED: dict[str, Grid | Pick | Shards] = {
    "panes": Grid(columns=10, rows=1),
    "head": Pick(of="panes", pieces=[1]),
    "unused": Grid(columns=2, rows=2),
}


def traced(photo, **keys):
    recipe = Recipe("traced", [Step("grade", {"exposure": 0.2}, mask="head")], size=None, regions=TRACED)
    return runner.trace(photo, recipe, **keys)


def test_trace_shows_each_region_the_steps_use_between_the_input_and_the_steps_and_none_they_do_not(photo):
    stages = traced(photo)
    assert [stage.label.split("(")[0].strip() for stage in stages] == [
        "input",
        "region 'panes'",
        "region 'head'",
        "1. grade",
    ]  # in the order declared, with the one `head` is picked from, and not the unused one
    assert all(stage.image.shape == photo.shape for stage in stages)
    np.testing.assert_array_equal(stages[0].image, photo)


def piece_columns(piece: int, margin: int = 0) -> slice:
    """The pixel columns of piece `piece` of 10 equal columns over 128 px (a pixel belongs to the piece holding its
    centre), without `margin` px at each side."""
    first, last = (math.ceil(k * 12.8 - 0.5) for k in (piece, piece + 1))
    return slice(first + margin, last - margin)


def test_the_stage_of_a_selection_is_white_where_it_chooses_and_black_elsewhere(photo):
    head = traced(photo)[2].image
    assert (head[:, piece_columns(1)] == 1).all()
    assert (head[:, : piece_columns(1).start] == 0).all()
    assert (head[:, piece_columns(1).stop :] == 0).all()


def test_the_stage_of_a_partition_draws_the_borders_and_a_number_in_every_piece(photo):
    panes = traced(photo)[1].image
    white = panes.min(axis=2) > 0.5  # the photo is dimmed, so only lines and numbers are this bright
    for piece in range(10):
        assert white[:, piece_columns(piece, 2)].any(), piece  # a number, away from the borders
    assert white[:, 12:14].any()  # a border between two pieces
    assert not np.array_equal(panes, photo)


def test_the_number_drawn_in_a_piece_is_its_own_a_one_has_less_ink_than_an_eight(photo):
    panes = traced(photo)[1].image

    def ink(piece: int) -> int:
        return int((panes[:, piece_columns(piece, 2)].min(axis=2) > 0.5).sum())

    assert ink(1) < ink(8)


def test_trace_still_ends_in_what_render_returns_when_regions_are_drawn(photo):
    recipe = Recipe("traced", [Step("grade", {"exposure": 0.2}, mask="head")], size=None, regions=TRACED)
    np.testing.assert_array_equal(runner.trace(photo, recipe)[-1].image, runner.render(photo, recipe))


def test_trace_labels_an_each_step_with_its_partition_and_looks(photo):
    each = Each("panes", [fill(RED), "none"], mask="head")
    labels = [stage.label for stage in runner.trace(photo, Recipe("t", [each], size=None, regions=TRACED))]
    assert labels[-1].startswith("1. each panes [fill(color='#ff0000'), none]")


def test_a_feather_is_a_share_of_the_long_side_so_the_blend_is_the_same_at_two_resolutions():
    grey = np.full((96, 128, 3), 0.5, np.float32)
    recipe = [Each("cols", [fill(RED), "none"], feather=0.02)]

    def rendered(size: int) -> np.ndarray:
        return runner.render(grey, Recipe("scaled", recipe, size=size, regions=columns(2)))

    small, large = rendered(128), rendered(256)
    halved = large.reshape(96, 2, 128, 2, 3).mean(axis=(1, 3))  # the large one at the small one's resolution
    np.testing.assert_allclose(halved, small, atol=0.03)
    assert 0.1 < small[0, 63, 1] < 0.4  # and it is a real blend at the border, not a hard edge


# --- the subject found piece by piece (grid first) ------------------------------------------------------------------


class PieceFinder:
    """A stand-in subject finder for pieces: records the crops it is asked about and finds a 4 x 4 subject at the top
    left of each crop."""

    def __init__(self) -> None:
        self.crops: list[np.ndarray] = []

    def __call__(self, rgb: np.ndarray) -> np.ndarray:
        self.crops.append(rgb.copy())
        matte = np.zeros(rgb.shape[:2], np.float32)
        matte[2:6, 2:6] = 1
        return matte


GRID_FIRST = Recipe(
    "grid-first",
    [fill(RED, mask="found")],
    size=64,
    regions={"cells": Grid(columns=2, rows=2), "found": Subject(per="cells", context=0)},
)


FINAL_PICTURE = {
    "render": lambda photo, recipe, **k: runner.render(photo, recipe, **k),
    "trace": lambda photo, recipe, **k: runner.trace(photo, recipe, **k)[-1].image,
    "render_all": lambda photo, recipe, **k: runner.render_all(photo, [PLAIN, recipe], **k)[1],
}


@pytest.mark.parametrize("run_it", FINAL_PICTURE.values(), ids=FINAL_PICTURE.keys())
def test_the_finder_is_asked_about_the_crop_of_each_piece_of_the_working_picture_and_the_finds_are_laid_where_found(
    run_it, photo
):
    finder = PieceFinder()
    out = run_it(photo, GRID_FIRST, find_subject=Finder("stand-in", finder))
    working = resize(photo, 64)  # 48 x 64: four pieces of 24 x 32
    assert sorted(crop.tobytes() for crop in finder.crops) == sorted(
        working[y : y + 24, x : x + 32].tobytes() for y in (0, 24) for x in (0, 32)
    )
    expected = working.copy()
    for y in (0, 24):
        for x in (0, 32):
            expected[y + 2 : y + 6, x + 2 : x + 6] = RGB[RED]
    np.testing.assert_array_equal(out, expected)


def test_a_matte_of_the_whole_photo_does_not_stand_in_for_the_finder_of_pieces(photo, matte):
    with pytest.raises(SubjectNotFoundError, match=r"'found'.*cells"):
        runner.render(photo, GRID_FIRST, matte=matte)
    finder = PieceFinder()
    runner.render(photo, GRID_FIRST, matte=matte, find_subject=Finder("stand-in", finder))
    assert [crop.shape for crop in finder.crops] == [(24, 32, 3)] * 4  # the pieces, never the whole photo


@pytest.mark.parametrize("run_it", RUNS.values(), ids=RUNS.keys())
def test_a_subject_found_piece_by_piece_is_refused_without_a_finder(run_it, photo):
    with pytest.raises(SubjectNotFoundError, match=r"'found'.*finder"):
        run_it(photo, GRID_FIRST)


def test_a_subject_found_piece_by_piece_that_no_step_uses_asks_nobody():
    finder = PieceFinder()
    photo = np.zeros((20, 30, 3), np.float32)
    recipe = Recipe("unused", [Step("grade", {"exposure": 0.5})], size=None, regions=GRID_FIRST.regions)
    runner.render(photo, recipe, find_subject=Finder("stand-in", finder))
    assert finder.crops == []
