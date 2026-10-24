"""Behaviour of the `reflection` effect."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "reflection"
HEIGHT, WIDTH = 30, 24
TOP, FLOOR = 2, 10  # the subject fills rows 2..9, so its lowest row is 9 and the floor starts at row 10
FLAT = {"fade": 0.0, "opacity": 1.0, "blur": 0.0}


def subject(top: int = TOP, floor: int = FLOOR) -> np.ndarray:
    matte = np.zeros((HEIGHT, WIDTH), np.float32)
    matte[top:floor, 4:20] = 1.0
    return matte


def rows_picture() -> np.ndarray:
    """Every row a different grey, so a row seen somewhere else says where it came from."""
    return np.repeat((np.arange(HEIGHT, dtype=np.float32) / HEIGHT)[:, None, None], WIDTH, axis=1).repeat(3, axis=2)


def white_over_black() -> np.ndarray:
    picture = np.zeros((HEIGHT, WIDTH, 3), np.float32)
    picture[:FLOOR] = 1.0
    return picture


@pytest.mark.parametrize(
    ("height", "mirrored"),
    [
        pytest.param(1.0, 8, id="height 1 mirrors as many rows as the subject has: 8"),
        pytest.param(0.5, 4, id="height 0.5 mirrors half of them"),
        pytest.param(0.0, 0, id="height 0 mirrors nothing"),
    ],
)
def test_the_picture_is_mirrored_about_the_lowest_row_of_the_subject_for_height_times_its_height(
    height: float, mirrored: int
) -> None:
    picture = rows_picture()
    out = registry.apply(NAME, picture, matte=subject(), height=height, **FLAT)
    expected = picture.copy()
    for i in range(mirrored):
        expected[FLOOR + i] = picture[FLOOR - 1 - i]  # the row just below the floor line shows the row just above it
    np.testing.assert_array_equal(out, expected)


@pytest.mark.parametrize(
    ("fade", "opacity", "weights"),
    [
        pytest.param(0.0, 1.0, [1.0] * 8, id="fade 0 keeps the reflection as strong all the way"),
        pytest.param(
            1.0,
            1.0,
            [1.0, 0.875, 0.75, 0.625, 0.5, 0.375, 0.25, 0.125],
            id="fade 1 falls in a straight line to nothing",
        ),
        pytest.param(0.5, 1.0, [1.0, 0.9375, 0.875, 0.8125, 0.75, 0.6875, 0.625, 0.5625], id="fade 0.5 falls to half"),
        pytest.param(1.0, 0.5, [0.5, 0.4375, 0.375, 0.3125, 0.25, 0.1875, 0.125, 0.0625], id="opacity scales it all"),
    ],
)
def test_the_reflection_fades_with_distance_from_the_floor_line(
    fade: float, opacity: float, weights: list[float]
) -> None:
    """White above the floor over black below it: a mirrored row shows white by exactly its weight."""
    out = registry.apply(NAME, white_over_black(), matte=subject(), height=1.0, fade=fade, opacity=opacity, blur=0.0)
    np.testing.assert_allclose(out[FLOOR : FLOOR + 8, 5, 0], weights, atol=1e-6)
    np.testing.assert_array_equal(out[:FLOOR], 1.0)  # the picture above the floor is never changed
    np.testing.assert_array_equal(out[FLOOR + 8 :], 0.0)  # and beyond the reflection's height


def test_where_the_picture_has_no_room_the_reflection_is_cut_at_the_edge() -> None:
    """The subject's lowest row is 3 rows from the bottom, so only 3 of its 8 mirrored rows fit."""
    picture = rows_picture()
    matte = subject(top=HEIGHT - 11, floor=HEIGHT - 3)
    out = registry.apply(NAME, picture, matte=matte, height=1.0, **FLAT)
    expected = picture.copy()
    for i in range(3):
        expected[HEIGHT - 3 + i] = picture[HEIGHT - 4 - i]
    np.testing.assert_array_equal(out, expected)


def test_a_cut_reflection_fades_as_if_it_had_its_whole_height() -> None:
    """The same 3 mirrored rows, but fading: they keep the first 3 of the 8 weights, not a fade squeezed into 3."""
    matte = subject(top=HEIGHT - 11, floor=HEIGHT - 3)
    picture = np.zeros((HEIGHT, WIDTH, 3), np.float32)
    picture[: HEIGHT - 3] = 1.0
    out = registry.apply(NAME, picture, matte=matte, height=1.0, fade=1.0, opacity=1.0, blur=0.0)
    np.testing.assert_allclose(out[HEIGHT - 3 :, 5, 0], [1.0, 0.875, 0.75], atol=1e-6)


def test_a_subject_that_reaches_the_bottom_has_nothing_to_reflect_on(photo: np.ndarray) -> None:
    matte = np.zeros(photo.shape[:2], np.float32)
    matte[20:, 30:90] = 1.0
    np.testing.assert_array_equal(registry.apply(NAME, photo, matte=matte, height=1.0), photo)


@pytest.mark.parametrize(
    ("edge_value", "threshold", "floor"),
    [
        pytest.param(0.3, 0.5, 10, id="a faint row below the subject is not its lowest row"),
        pytest.param(0.3, 0.2, 11, id="but it is when the threshold lets it count"),
        pytest.param(0.5, 0.5, 11, id="a row exactly at the threshold counts"),
    ],
)
def test_threshold_decides_how_strong_a_row_of_the_matte_must_be_to_count_as_the_subject(
    edge_value: float, threshold: float, floor: int
) -> None:
    matte = subject()
    matte[FLOOR, 4:20] = edge_value
    picture = rows_picture()
    out = registry.apply(NAME, picture, matte=matte, height=0.25, threshold=threshold, **FLAT)  # 8 rows x 0.25 = 2
    assert out[floor, 0, 0] == picture[floor - 1, 0, 0]
    assert out[floor + 1, 0, 0] == picture[floor - 2, 0, 0]
    assert out[floor + 2, 0, 0] == picture[floor + 2, 0, 0]


def test_blur_softens_the_reflection_as_a_gaussian_of_that_sigma() -> None:
    """One white pixel at row 4, far enough from the floor for its reflection (row 15) to be unaffected by the edge:
    blurred, it becomes the 2-D Gaussian, which peaks at 1 / (2 pi sigma^2) and is exp(-d^2 / 2 sigma^2) of that
    d px to the side."""
    sigma = 1.5
    picture = np.zeros((HEIGHT, WIDTH, 3), np.float32)
    picture[4, 12] = 1.0
    out = registry.apply(NAME, picture, matte=subject(), height=1.0, fade=0.0, opacity=1.0, blur=sigma)
    peak = 1 / (2 * np.pi * sigma**2)
    assert out[15, 12, 0] == pytest.approx(peak, abs=1e-4)
    assert out[15, 14, 0] == pytest.approx(peak * np.exp(-4 / (2 * sigma**2)), abs=1e-4)


def test_the_blur_continues_the_mirror_image_across_the_floor_line() -> None:
    """A white pixel on the row just above the floor line touches its own reflection: the blurred mirror image at the
    first row below the line is the Gaussian of both, 1 row apart (and 0 apart): peak x (1 + exp(-1 / 2 sigma^2))."""
    sigma = 1.5
    picture = np.zeros((HEIGHT, WIDTH, 3), np.float32)
    picture[FLOOR - 1, 12] = 1.0
    out = registry.apply(NAME, picture, matte=subject(), height=1.0, fade=0.0, opacity=1.0, blur=sigma)
    peak = 1 / (2 * np.pi * sigma**2)
    assert out[FLOOR, 12, 0] == pytest.approx(peak * (1 + np.exp(-1 / (2 * sigma**2))), abs=1e-4)


def test_without_a_subject_there_is_no_reflection(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo), photo)
    np.testing.assert_array_equal(registry.apply(NAME, photo, matte=np.zeros(photo.shape[:2], np.float32)), photo)
