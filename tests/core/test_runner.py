"""Tests of `core/runner.py`: the runner is given the subject (a matte, or a function that finds it); it never looks
for the subject itself."""

import tracemalloc

import numpy as np
import pytest

from photo_fx import SubjectNotFoundError
from photo_fx.core import runner
from photo_fx.core.params import EffectParams, param
from photo_fx.core.registry import effect
from photo_fx.core.steps import Recipe, Step

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


def test_the_finder_sees_the_photo_already_scaled_to_the_recipes_size(photo, matte):
    finder = CountingFinder(matte[::2, ::2])  # what the 48x64 photo's matte looks like
    small = Recipe("small", MASKED.steps, size=64)
    assert runner.render(photo, small, find_subject=finder).shape == (48, 64, 3)
    assert finder.seen == [(48, 64, 3)]


@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_a_finder_that_finds_nothing_is_an_error_naming_it(run, photo):
    """A finder returning None (instead of raising) would otherwise make `trace` drop the subject stage silently."""
    with pytest.raises(SubjectNotFoundError, match="`find_subject` returned None"):
        run(photo, MASKED, find_subject=lambda rgb: None)


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
