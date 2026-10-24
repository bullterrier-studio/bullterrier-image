"""Behaviour of the `spotlight` effect."""

import math

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "spotlight"
BASE = 0.8
BLACK = "#000000"


def grey(size: int = 40) -> np.ndarray:
    return np.full((size, size, 3), BASE, np.float32)


def block() -> np.ndarray:
    """A matte of a 20 x 20 square, rows and columns 10..29 of a 40 x 40 picture."""
    matte = np.zeros((40, 40), np.float32)
    matte[10:30, 10:30] = 1.0
    return matte


def test_without_a_subject_there_is_no_light_to_shape(photo):
    np.testing.assert_array_equal(registry.apply(NAME, photo), photo)


def test_amount_0_leaves_the_picture_unchanged(photo, matte):
    np.testing.assert_array_equal(registry.apply(NAME, photo, matte=matte, amount=0), photo)


def test_the_subject_is_untouched(photo, matte):
    out = registry.apply(NAME, photo, matte=matte, amount=1.0, feather=6)
    subject = matte >= 0.5
    np.testing.assert_array_equal(out[subject], photo[subject])
    assert (out[~subject] < photo[~subject]).any()


@pytest.mark.parametrize(
    ("falloff", "growth"),
    [(1.0, lambda reach: reach), (2.0, lambda reach: reach**2), (0.5, lambda reach: math.sqrt(reach))],
    ids=["linear", "squared: stays light near the subject", "square root: darkens quickly near it"],
)
def test_darkness_grows_with_the_distance_from_the_silhouette_to_full_at_radius(falloff, growth):
    """Radius 10, black, amount 1 on grey 0.8: a pixel d px from the square (straight, or from its corner) has
    0.8 (1 - growth(d / 10)) left, and is black from 10 px out."""
    out = registry.apply(NAME, grey(), matte=block(), radius=10, falloff=falloff, amount=1.0, feather=0, color=BLACK)
    straight = {1: 1, 4: 4, 8: 8, 10: 10}  # left of the square, row 20: column 10 - d
    for d, px in straight.items():
        assert out[20, 10 - px, 0] == pytest.approx(BASE * (1 - growth(min(d / 10, 1))), abs=1e-5), d
    corner = math.hypot(5, 5)  # row 5, column 5: diagonal from the square's corner pixel (10, 10)
    assert out[5, 5, 0] == pytest.approx(BASE * (1 - growth(corner / 10)), abs=1e-5)
    assert out[20, 0, 0] == 0.0  # 10 px away: full strength


def test_darkness_never_decreases_away_from_the_subject():
    out = registry.apply(NAME, grey(), matte=block(), radius=12, falloff=0.6, amount=0.8, feather=3)
    row = out[20, :10, 0][::-1]  # from the subject's edge outwards
    assert (np.diff(row) <= 1e-7).all()
    assert row[0] > row[-1]


def test_amount_scales_the_darkening_and_color_is_what_the_picture_darkens_toward():
    colour = [0.0, 0.0, 1.0]
    out = registry.apply(NAME, grey(), matte=block(), radius=10, falloff=1.0, amount=0.5, feather=0, color=colour)
    # 4 px out: darkness 0.5 x 0.4 = 0.2 of the way from the grey to blue: 0.8 x 0.8 + 0 x 0.2, and 0.8 x 0.8 + 1 x 0.2
    np.testing.assert_allclose(out[20, 6], [0.64, 0.64, 0.84], atol=1e-5)


def test_feather_lets_the_light_spill_softly_around_the_subject():
    """Blurring the matte by sigma 2 px: 3 px out the light left is that of a step edge blurred, 1 - Phi(2.5 / 2)
    = 0.106 (the erfc below), so the darkness there is that much less; far away it makes no difference."""
    kwargs = {"radius": 20, "falloff": 1.0, "amount": 1.0, "color": BLACK}
    sharp = registry.apply(NAME, grey(60), matte=np.pad(block(), 10), feather=0, **kwargs)
    soft = registry.apply(NAME, grey(60), matte=np.pad(block(), 10), feather=2, **kwargs)
    spill = 0.5 * math.erfc(2.5 / (2 * math.sqrt(2)))  # share of the light still there, 3 px out (edge at 2.5)
    darkness_sharp = 1 - sharp[30, 17, 0] / BASE  # row 30 (middle), column 17: 3 px left of the square
    darkness_soft = 1 - soft[30, 17, 0] / BASE
    assert darkness_soft == pytest.approx(darkness_sharp * (1 - spill), abs=0.003)
    assert soft[30, 0, 0] == sharp[30, 0, 0]  # 20 px away: beyond the blur's reach, the same


@pytest.mark.parametrize(
    ("threshold", "next_to_it", "beyond"),
    [(0.3, BASE, BASE * (1 - 0.1)), (0.5, BASE * (1 - 0.1 * 0.6), BASE * (1 - 0.2))],
    ids=["0.4 is subject at 0.3: untouched", "0.4 is background at 0.5: 1 px out, 40 % lit"],
)
def test_matte_threshold_is_the_matte_value_from_which_a_pixel_belongs_to_the_subject(threshold, next_to_it, beyond):
    """Radius 10, so darkness is d / 10: the pixel the matte covers 40 % is the subject itself, or 1 px from it
    (darkness 0.1 x the 0.6 it is not lit) and the one beside it 1 or 2 px out."""
    matte = block()
    matte[20, 9] = 0.4
    out = registry.apply(
        NAME, grey(), matte=matte, radius=10, falloff=1.0, amount=1.0, feather=0, color=BLACK, matte_threshold=threshold
    )
    assert out[20, 9, 0] == pytest.approx(next_to_it, abs=1e-5)
    assert out[20, 8, 0] == pytest.approx(beyond, abs=1e-5)


@pytest.mark.parametrize(
    ("fill", "darkened"), [(0.0, True), (1.0, False)], ids=["no subject: all is the rest", "all subject: untouched"]
)
def test_a_matte_covering_nothing_or_everything(fill, darkened, photo):
    matte = np.full(photo.shape[:2], fill, np.float32)
    out = registry.apply(NAME, photo, matte=matte, amount=0.5, color=BLACK)
    np.testing.assert_allclose(out, photo * 0.5 if darkened else photo, atol=1e-6)


def test_edge_sets_what_the_feather_sees_beyond_the_picture_border():
    """A subject at the left border: wrapped round, it lies next to the last column, whose light then spills; mirrored,
    that column has only dark surroundings."""
    matte = np.zeros((40, 40), np.float32)
    matte[10:30, :10] = 1.0
    kwargs = {"matte": matte, "radius": 40, "falloff": 1.0, "amount": 1.0, "feather": 3, "color": BLACK}
    mirrored = registry.apply(NAME, grey(), edge="reflect", **kwargs)
    wrapped = registry.apply(NAME, grey(), edge="wrap", **kwargs)
    assert wrapped[20, 39, 0] > mirrored[20, 39, 0] + 0.05
