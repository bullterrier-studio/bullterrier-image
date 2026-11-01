"""Tests of `threads=` on `render`, `trace` and `render_all`: the same bytes at any number, a bad number refused, and
renders from several Python threads."""

from concurrent.futures import ThreadPoolExecutor

import numpy as np
import pytest

from bullterrier_image import RecipeError
from bullterrier_image import recipe as facade
from bullterrier_image.core.params import Picture
from bullterrier_image.core.steps import Recipe, Step
from bullterrier_image.recipe import Framing

PLAIN = Recipe((Step("grade", params={"exposure": 0.5}),), name="plain", size=None)


def threaded_look() -> Recipe:
    """A look on a 300 x 400 photo that the core shares out between threads (grade, a mask and blend mode, a turn and a
    resize) and runs through a numpy effect and a picture parameter, which stay on the calling thread."""
    logo = Picture(np.random.default_rng(5).random((8, 8, 4), dtype=np.float32), "logo")
    return Recipe(
        [
            Step("grade", params={"exposure": 0.4, "saturation": 1.2}),
            Step("vignette", params={"amount": 0.4}),
            Step("blend", params={"image": logo}, opacity=0.5),
            Step("grade", params={"exposure": -0.3}, blend_mode="overlay", opacity=0.7, mask="light"),
        ],
        name="threaded",
        size=250,
        frame=Framing(angle=7, fit="expand", resample="bilinear"),
    )


@pytest.fixture(scope="module")
def big_photo() -> np.ndarray:
    return np.random.default_rng(2).random((300, 400, 3), dtype=np.float32)


@pytest.mark.parametrize("threads", [2, 4, 0])
@pytest.mark.parametrize(
    "function",
    [facade.render, lambda photo, recipe, **keys: facade.render_all(photo, [recipe], **keys)[0]],
    ids=["render", "render_all"],
)
def test_a_render_on_more_threads_gives_the_same_bytes_as_on_one(big_photo, function, threads):
    alone = function(big_photo, threaded_look())
    assert function(big_photo, threaded_look(), threads=threads).tobytes() == alone.tobytes()


@pytest.mark.parametrize("threads", [2, 0])
def test_a_trace_on_more_threads_shows_the_same_stages_as_on_one(big_photo, threads):
    alone = facade.trace(big_photo, threaded_look())
    more = facade.trace(big_photo, threaded_look(), threads=threads)
    assert [stage.label for stage in more] == [stage.label for stage in alone]
    assert all(m.image.tobytes() == a.image.tobytes() for m, a in zip(more, alone, strict=True))


@pytest.mark.parametrize(
    ("threads", "error"),
    [
        pytest.param(-1, "got -1", id="negative"),
        pytest.param(1.5, "got 1.5", id="fraction"),
        pytest.param("2", "got '2'", id="text"),
        pytest.param(True, "got True", id="bool"),
        pytest.param(None, "got None", id="none"),
        pytest.param(257, "257", id="more than the core allows"),
    ],
)
@pytest.mark.parametrize(
    "call",
    [
        pytest.param(lambda photo, **keys: facade.render(photo, PLAIN, **keys), id="render"),
        pytest.param(lambda photo, **keys: facade.trace(photo, PLAIN, **keys), id="trace"),
        pytest.param(lambda photo, **keys: facade.render_all(photo, [PLAIN], **keys), id="render_all"),
    ],
)
def test_a_threads_that_is_no_number_of_threads_is_refused_naming_it(photo, call, threads, error):
    with pytest.raises(RecipeError, match=error):
        call(photo, threads=threads)


def test_renders_from_several_python_threads_with_threads_each_give_each_photos_picture(big_photo):
    photos = [np.ascontiguousarray(np.roll(big_photo, shift, axis=1)) for shift in range(8)]
    alone = [facade.render(photo, threaded_look()).tobytes() for photo in photos]
    with ThreadPoolExecutor(4) as pool:
        together = list(pool.map(lambda photo: facade.render(photo, threaded_look(), threads=2).tobytes(), photos))
    assert together == alone
    assert len(set(alone)) == len(alone), "the photos give different pictures"
