"""Tests of `threads=` as the facade passes it on: a bad number refused, and renders from several Python threads."""

from concurrent.futures import ThreadPoolExecutor

import numpy as np
import pytest

from bullterrier_image import RecipeError
from bullterrier_image import recipe as facade
from bullterrier_image.core.steps import Recipe, Step
from bullterrier_image.core.values import Picture
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


def test_a_render_on_more_threads_gives_the_same_bytes_as_on_one(big_photo):
    alone = facade.render(big_photo, threaded_look())
    assert facade.render(big_photo, threaded_look(), threads=2).tobytes() == alone.tobytes()


@pytest.mark.parametrize(
    ("threads", "error"),
    [
        pytest.param(-1, "got -1", id="negative"),
        pytest.param(1.5, "got 1.5", id="fraction"),
        pytest.param("2", "got '2'", id="text"),
        pytest.param(True, "got True", id="bool"),
        pytest.param(None, "got None", id="none"),
        pytest.param(257, "got 257", id="one more than allowed"),
        pytest.param(2**70, str(2**70), id="more than a machine word"),
    ],
)
def test_a_threads_that_is_no_number_of_threads_is_refused_naming_it(photo, threads, error):
    with pytest.raises(RecipeError, match=error):
        facade.render(photo, PLAIN, threads=threads)


def test_renders_from_several_python_threads_with_threads_each_give_each_photos_picture(big_photo):
    photos = [np.ascontiguousarray(np.roll(big_photo, shift, axis=1)) for shift in range(8)]
    alone = [facade.render(photo, threaded_look()).tobytes() for photo in photos]
    with ThreadPoolExecutor(4) as pool:
        together = list(pool.map(lambda photo: facade.render(photo, threaded_look(), threads=2).tobytes(), photos))
    assert together == alone
    assert len(set(alone)) == len(alone), "the photos give different pictures"
