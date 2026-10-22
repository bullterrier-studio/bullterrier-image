"""Tests of `core/runner.py`: the runner is given the subject (a matte, or a function that finds it); it never looks
for the subject itself."""

import numpy as np
import pytest

from photo_fx.core import runner
from photo_fx.core.steps import Recipe, Step
from photo_fx.core.timing import Animated, Animation

MASKED = Recipe("masked", (Step("fill", {"color": "#000000"}, "background"),), size=None)
PLAIN = Recipe("plain", (Step("grade", {"exposure": 0.5}),), size=None)
CLIP = Recipe(
    "clip",
    (Step("fill", {"color": "#000000"}, "background"), Step("grade", {"exposure": Animated(0.0, 1.0)})),
    size=None,
    animation=Animation(seconds=3, fps=1),  # three frames
)


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
    "render_frames": lambda photo, recipe, **k: list(runner.render_frames(photo, recipe, **k)),
    "render_all": lambda photo, recipe, **k: runner.render_all(photo, [PLAIN, recipe], **k),
}


@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_a_masked_recipe_without_a_matte_or_a_finder_is_refused_naming_both_ways(run, photo):
    recipe = CLIP if run is RUNS["render_frames"] else MASKED
    with pytest.raises(ValueError, match="needs the subject") as raised:
        run(photo, recipe)
    message = str(raised.value)  # names the recipe and both ways to give the subject
    assert f"'{recipe.name}'" in message
    assert "`matte`" in message
    assert "`find_subject`" in message


@pytest.mark.parametrize("run", RUNS.values(), ids=RUNS.keys())
def test_the_finder_is_called_exactly_once_per_render(run, photo, matte):
    finder = CountingFinder(matte)
    run(photo, CLIP if run is RUNS["render_frames"] else MASKED, find_subject=finder)
    assert len(finder.seen) == 1  # once for a still, a trace, a whole clip, or all the recipes of render_all


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


def test_an_animation_whose_parameters_never_change_is_the_still_held_for_the_whole_clip(photo):
    held = Recipe("held", PLAIN.steps, size=None, animation=Animation(seconds=3, fps=1))  # three frames, none moving
    frames = list(runner.render_frames(photo, held))
    assert len(frames) == 3
    still = runner.render(photo, PLAIN)
    for frame in frames:
        np.testing.assert_array_equal(frame, still)
