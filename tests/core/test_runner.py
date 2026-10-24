"""Tests of `core/runner.py`: the runner is given the subject (a matte, or a function that finds it); it never looks
for the subject itself."""

import tracemalloc

import numpy as np
import pytest

from bullterrier_image import SubjectNotFoundError
from bullterrier_image.core import runner
from bullterrier_image.core.framing import Framing
from bullterrier_image.core.params import EffectParams, param
from bullterrier_image.core.registry import effect
from bullterrier_image.core.steps import Recipe, Step
from bullterrier_image.ops.resample import resize

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
    run(photo, MASKED, find_subject=finder)
    assert len(finder.seen) == 1  # once for a render, a trace, or all the recipes of render_all


def test_render_all_asks_once_however_many_recipes_need_the_subject(photo, matte):
    finder = CountingFinder(matte)
    assert len(runner.render_all(photo, [MASKED, PLAIN, MASKED], find_subject=finder)) == 3
    assert len(finder.seen) == 1


def test_the_finder_is_not_called_when_nothing_needs_the_subject_or_the_matte_is_given(photo, matte):
    finder = CountingFinder(matte)
    runner.render(photo, PLAIN, find_subject=finder)
    runner.render_all(photo, [PLAIN, PLAIN], find_subject=finder)
    runner.render(photo, MASKED, matte, find_subject=finder)
    assert finder.seen == []


def test_the_finders_matte_is_the_one_the_recipe_uses(photo, matte):
    found = runner.render(photo, MASKED, find_subject=CountingFinder(matte))
    np.testing.assert_array_equal(found, runner.render(photo, MASKED, matte))
    assert not np.array_equal(found, runner.render(photo, MASKED, 1.0 - matte))


def test_the_finder_sees_the_photo_as_given_before_it_is_framed_or_scaled(photo, matte):
    finder = CountingFinder(matte)
    small = Recipe("small", MASKED.steps, size=64, frame=Framing(aspect=1.0))
    assert runner.render(photo, small, find_subject=finder).shape == (64, 64, 3)
    assert finder.seen == [(96, 128, 3)]  # not the 64 px square it is rendered as


def test_a_finders_matte_of_another_size_than_the_photo_is_refused(photo, matte):
    with pytest.raises(SubjectNotFoundError, match="matte is"):
        runner.render(photo, MASKED, find_subject=CountingFinder(matte[::2, ::2]))


@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_a_finder_that_finds_nothing_is_an_error_naming_it(run, photo):
    """A finder returning None (instead of raising) would otherwise make `trace` drop the subject stage silently."""
    with pytest.raises(SubjectNotFoundError, match="`find_subject` returned None"):
        run(photo, MASKED, find_subject=lambda rgb: None)


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
    expected = runner.render(by_hand(photo), MASKED, by_hand(matte))  # background black, by hand
    np.testing.assert_array_equal(runner.render(photo, framed, matte), expected)
    assert (expected == 0).any()
    assert np.array_equal(runner.render(photo, framed, matte)[by_hand(matte) == 1], by_hand(photo)[by_hand(matte) == 1])


def test_framing_comes_before_the_size(photo):
    framed = Recipe("framed", PLAIN.steps, size=48, frame=Framing(aspect=1.0))
    by_hand = runner.render(resize(photo[:, 16:112], 48), PLAIN)  # crop 96 px square, then scale it to 48
    np.testing.assert_array_equal(runner.render(photo, framed), by_hand)


def test_a_crop_around_the_subject_uses_the_matte_that_the_finder_found_on_the_whole_photo(photo, matte):
    framed = Recipe("framed", PLAIN.steps, size=None, frame=Framing(anchor="subject", aspect=1.0, margin=0.1))
    for result in (
        runner.render(photo, framed, matte),
        runner.render(photo, framed, find_subject=CountingFinder(matte)),
    ):
        np.testing.assert_array_equal(result, runner.render(photo[13:84, 29:100], PLAIN))  # the disc and 6 px each way


def test_a_subject_crop_needs_the_subject_even_when_no_step_does(photo):
    framed = Recipe("framed", PLAIN.steps, size=None, frame=Framing(anchor="subject"))
    with pytest.raises(SubjectNotFoundError, match="needs the subject"):
        runner.render(photo, framed)


def test_trace_shows_the_framed_photo_and_matte_and_ends_in_the_render(photo, matte):
    framed = Recipe("framed", MASKED.steps, size=None, frame=Framing(aspect=1.0))
    stages = runner.trace(photo, framed, matte)
    np.testing.assert_array_equal(stages[0].image, photo[:, 16:112])
    np.testing.assert_array_equal(stages[1].image[..., 0], matte[:, 16:112])
    np.testing.assert_array_equal(stages[-1].image, runner.render(photo, framed, matte))


def test_render_all_finds_the_subject_once_for_framed_and_plain_recipes(photo, matte):
    finder = CountingFinder(matte)
    framed = Recipe("framed", PLAIN.steps, size=None, frame=Framing(anchor="subject"))
    results = runner.render_all(photo, [PLAIN, framed], find_subject=finder)
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
    out = runner.render(photo, recipe, matte)
    np.testing.assert_array_equal(out[..., 3], matte[:, ::-1][:, 16:112])
    np.testing.assert_array_equal(out[..., :3], runner.render(photo[:, ::-1][:, 16:112], PLAIN))


def test_the_alpha_of_rounded_corners_is_clear_in_the_corners_and_opaque_in_the_middle_of_each_side(photo):
    recipe = Recipe("rounded", PLAIN.steps, size=None, frame=Framing(transparent="rounded", corner_radius=0.25))
    alpha = runner.render(photo, recipe)[..., 3]  # a corner radius of a quarter of the 96 px side: 24 px
    assert [alpha[0, 0], alpha[0, -1], alpha[-1, 0], alpha[-1, -1]] == [0, 0, 0, 0]
    assert [alpha[0, 64], alpha[-1, 64], alpha[48, 0], alpha[48, -1]] == [1, 1, 1, 1]


def test_the_cut_is_made_at_the_recipes_size_after_the_steps(photo, matte):
    recipe = Recipe("cut", PLAIN.steps, size=64, frame=Framing(transparent="background"))
    out = runner.render(photo, recipe, matte)
    assert out.shape == (48, 64, 4)
    np.testing.assert_array_equal(out[..., 3], resize(matte, 64))


def test_a_cut_around_the_subject_needs_the_subject_even_when_no_step_does(photo):
    recipe = Recipe("cut", PLAIN.steps, size=None, frame=Framing(transparent="background"))
    with pytest.raises(SubjectNotFoundError, match="needs the subject"):
        runner.render(photo, recipe)


def test_trace_adds_the_cut_as_its_last_stage_and_it_is_what_render_returns(photo, matte):
    recipe = Recipe("cut", MASKED.steps, size=None, frame=Framing(transparent="circle"))
    stages = runner.trace(photo, recipe, matte)
    assert [stage.image.shape[2] for stage in stages] == [3, 3, 3, 4]  # input, matte, the step, the cut
    assert stages[-1].label == "2. transparent('circle')"
    np.testing.assert_array_equal(stages[-1].image, runner.render(photo, recipe, matte))
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
    picture = runner.render(photo, recipe, matte).nbytes  # also warms up: one-time allocations stay out
    tracemalloc.start()
    try:
        runner.render(photo, recipe, matte)
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
        ("background", SUBJECT, 1 / 3),
    ],
    ids=["plain", "subject", "background"],
)
def test_a_linear_render_holds_no_more_than_the_working_step_its_input_and_output(memory_probes, mask, matte, kept):
    """A step's input is released once the next step has run, and neither the scaled photo nor a masked step's blend
    weights are kept for the rest of the chain: the peak is the busiest step's scratch, output and input (and the
    scaled matte), whatever the number of steps."""
    steps = tuple(Step("scratch_probe", {"copies": SCRATCH_COPIES}, mask) for _ in range(6))
    bound = SCRATCH_COPIES + 1 + 1 + kept + 0.25  # scratch, output, input, matte, a quarter picture for small things
    assert peak_pictures(PHOTO, Recipe("chain", steps, size=320), matte) <= bound
    # known-bad: the same gauge sees a picture that something keeps alive
    leaky = (*steps[:3], Step("hoarding_probe", {}, mask), *steps[3:])
    assert peak_pictures(PHOTO, Recipe("leaky", leaky, size=320), matte) > bound
