"""Behaviour of the `layered_cutouts` effect."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "layered_cutouts"
GREY = 0.8
RED, BLUE = "#ff0000", "#0000ff"


def grey(height: int = 12, width: int = 40) -> np.ndarray:
    return np.full((height, width, 3), GREY, np.float32)


def block(cols: slice, height: int = 12, width: int = 40) -> np.ndarray:
    """A subject 4 rows tall in the middle and the given columns wide."""
    matte = np.zeros((height, width), np.float32)
    matte[4:8, cols] = 1.0
    return matte


def row(out: np.ndarray, cols: slice) -> np.ndarray:
    return out[5, cols]


def test_each_layer_is_the_silhouette_moved_one_more_offset_and_coloured_from_color_toward_end_color() -> None:
    """A 2 px subject, three layers 4 px apart, to the right: layer k covers the 2 px that start 4k px on. The
    colours step evenly from red (the layer by the subject) to blue (the farthest): red, purple, blue."""
    out = registry.apply(
        NAME, grey(), matte=block(slice(5, 7)), layers=3, offset=4.0, angle=0, color=RED, end_color=BLUE, opacity=1.0
    )
    np.testing.assert_allclose(row(out, slice(0, 5)), GREY, atol=1e-6)  # left of the subject
    np.testing.assert_allclose(row(out, slice(5, 7)), GREY, atol=1e-6)  # the subject itself
    np.testing.assert_allclose(row(out, slice(7, 9)), GREY, atol=1e-6)  # between the subject and the first layer
    np.testing.assert_allclose(row(out, slice(9, 11)), [[1.0, 0.0, 0.0]] * 2, atol=1e-6)
    np.testing.assert_allclose(row(out, slice(11, 13)), GREY, atol=1e-6)
    np.testing.assert_allclose(row(out, slice(13, 15)), [[0.5, 0.0, 0.5]] * 2, atol=1e-6)
    np.testing.assert_allclose(row(out, slice(15, 17)), GREY, atol=1e-6)
    np.testing.assert_allclose(row(out, slice(17, 19)), [[0.0, 0.0, 1.0]] * 2, atol=1e-6)
    np.testing.assert_allclose(row(out, slice(19, 40)), GREY, atol=1e-6)


def test_a_layer_nearer_the_subject_is_in_front_of_the_farther_ones() -> None:
    """Layers 2 px apart on a 6 px subject overlap; where they do the nearest shows. The subject is cols 10..15, so
    layer 1 is cols 12..17, layer 2 cols 14..19, layer 3 cols 16..21: 16..17 belong to layer 1, 18..19 to layer 2 and
    20..21 to layer 3."""
    out = registry.apply(
        NAME, grey(), matte=block(slice(10, 16)), layers=3, offset=2.0, angle=0, color=RED, end_color=BLUE, opacity=1.0
    )
    np.testing.assert_allclose(row(out, slice(16, 18)), [[1.0, 0.0, 0.0]] * 2, atol=1e-6)
    np.testing.assert_allclose(row(out, slice(18, 20)), [[0.5, 0.0, 0.5]] * 2, atol=1e-6)
    np.testing.assert_allclose(row(out, slice(20, 22)), [[0.0, 0.0, 1.0]] * 2, atol=1e-6)


def test_layers_are_laid_over_each_other_by_opacity() -> None:
    """Two layers of opacity 0.5 over grey: the farther one first, g + 0.5 (blue - g) = (0.4, 0.4, 0.9) where only it
    shows, and the nearer one over that, + 0.5 (red - that) = (0.7, 0.2, 0.45), where both do."""
    out = registry.apply(
        NAME, grey(), matte=block(slice(10, 16)), layers=2, offset=2.0, angle=0, color=RED, end_color=BLUE, opacity=0.5
    )
    np.testing.assert_allclose(row(out, slice(16, 18)), [[0.7, 0.2, 0.45]] * 2, atol=1e-6)
    np.testing.assert_allclose(row(out, slice(18, 20)), [[0.4, 0.4, 0.9]] * 2, atol=1e-6)


@pytest.mark.parametrize(
    ("angle", "step", "offset"),
    [
        pytest.param(0, (0, 1), 3.0, id="0 degrees is to the right"),
        pytest.param(90, (1, 0), 3.0, id="90 degrees is down"),
        pytest.param(180, (0, -1), 3.0, id="180 degrees is to the left"),
        pytest.param(-90, (-1, 0), 3.0, id="-90 degrees is up"),
        pytest.param(45, (1, 1), 3 * 2**0.5, id="45 degrees is the lower right; the offset is along the ray"),
    ],
)
def test_the_layers_go_in_the_direction_of_the_angle(angle: float, step: tuple[int, int], offset: float) -> None:
    matte = np.zeros((21, 21), np.float32)
    matte[10, 10] = 1.0
    out = registry.apply(
        NAME, grey(21, 21), matte=matte, layers=2, offset=offset, angle=angle, color=RED, end_color=BLUE, opacity=1.0
    )
    nearest = (10 + 3 * step[0], 10 + 3 * step[1])
    farthest = (10 + 6 * step[0], 10 + 6 * step[1])
    expected = grey(21, 21)
    expected[nearest], expected[farthest] = [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]
    np.testing.assert_allclose(out, expected, atol=1e-6)


def test_a_soft_edge_keeps_its_share_of_the_subject_however_many_layers_lie_behind() -> None:
    """Column 16 is half subject; layers 1 px apart cover it all with the red of the layer by the subject, so it is
    half picture, half red, whether there is one layer or four."""
    matte = block(slice(10, 16))
    matte[4:8, 16] = 0.5
    picture = grey()
    for layers in (1, 4):
        out = registry.apply(NAME, picture, matte=matte, layers=layers, offset=1.0, angle=0, color=RED, opacity=1.0)
        np.testing.assert_allclose(out[5, 16], [0.9, 0.4, 0.4], atol=1e-6)


def test_a_subject_cut_by_the_border_has_layers_that_go_on_past_it() -> None:
    """The subject fills rows 2 to 4 of the six left columns, so it goes on beyond the border. The picture must show
    what a picture 10 px wider on the left, with the subject carried on into it, shows."""
    matte = np.zeros((20, 12), np.float32)
    matte[2:5, :6] = 1.0
    wide = np.zeros((20, 22), np.float32)
    wide[2:5, :16] = 1.0
    kwargs = {"layers": 3, "offset": 4 * 2**0.5, "angle": 45, "color": RED, "end_color": BLUE, "opacity": 1.0}
    out = registry.apply(NAME, grey(20, 12), matte=matte, **kwargs)
    out_wide = registry.apply(NAME, grey(20, 22), matte=wide, **kwargs)
    np.testing.assert_allclose(out, out_wide[:, 10:], atol=1e-6)
    np.testing.assert_allclose(out[8, 0], [1.0, 0.0, 0.0], atol=1e-6)  # in the layer of the part out of the picture


def test_the_subject_is_never_changed_and_opacity_zero_changes_nothing(photo: np.ndarray, matte: np.ndarray) -> None:
    out = registry.apply(NAME, photo, matte=matte, layers=5, offset=6.0, angle=30, opacity=1.0)
    np.testing.assert_array_equal(out[matte == 1], photo[matte == 1])
    assert not np.array_equal(out, photo)
    np.testing.assert_array_equal(registry.apply(NAME, photo, matte=matte, opacity=0.0), photo)


def test_without_a_subject_there_are_no_layers(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo), photo)
