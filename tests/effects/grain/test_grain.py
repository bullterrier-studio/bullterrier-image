"""Behaviour of the `grain` effect."""

import numpy as np
import pytest

from photo_fx import effects

NAME = "grain"


def grey(value: float = 0.5, size: int = 128) -> np.ndarray:
    return np.full((size, size, 3), value, np.float32)


def added_noise(params: dict, size: int = 128) -> np.ndarray:
    """The brightness grain adds to a mid-grey picture, small enough never to be clipped: (h, w)."""
    return (effects.apply(NAME, grey(size=size), **params) - 0.5)[..., 0].astype(np.float64)


def neighbour_correlation(noise: np.ndarray) -> float:
    """Correlation of each pixel's noise with the one to its right (0 = independent pixels, 1 = smooth)."""
    return float(np.corrcoef(noise[:, :-1].ravel(), noise[:, 1:].ravel())[0, 1])


def test_the_same_seed_gives_the_same_grain_and_another_seed_other_grain(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(effects.apply(NAME, photo, seed=1), effects.apply(NAME, photo, seed=1))
    assert np.abs(effects.apply(NAME, photo, seed=1) - effects.apply(NAME, photo, seed=2)).mean() > 0.01


def test_amount_zero_leaves_the_picture_unchanged(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(effects.apply(NAME, photo, amount=0.0), photo)


@pytest.mark.parametrize("size", [1.0, 4.0])
def test_amount_is_the_standard_deviation_of_the_added_brightness(size: float) -> None:
    noise = added_noise({"amount": 0.03, "size": size, "seed": 5})
    assert noise.std() == pytest.approx(0.03, rel=0.02)
    assert abs(noise.mean()) < 0.003  # brightens and darkens alike


def test_grain_is_the_same_in_every_channel() -> None:
    out = effects.apply(NAME, grey(), amount=0.05, seed=3)
    np.testing.assert_array_equal(out[..., 0], out[..., 1])
    np.testing.assert_array_equal(out[..., 0], out[..., 2])


def test_size_above_one_pixel_clumps_the_grain_and_one_pixel_does_not() -> None:
    assert abs(neighbour_correlation(added_noise({"amount": 0.03, "size": 1.0}))) < 0.05
    assert neighbour_correlation(added_noise({"amount": 0.03, "size": 4.0})) > 0.5
    assert neighbour_correlation(added_noise({"amount": 0.03, "size": 8.0})) > neighbour_correlation(
        added_noise({"amount": 0.03, "size": 4.0})
    )


def test_the_result_is_clipped_to_the_picture_range() -> None:
    out = effects.apply(NAME, grey(1.0), amount=0.5)
    assert out.max() == 1.0
    assert out.min() < 0.9  # darkened where the noise is negative, never above white


def test_mode_changes_the_clumped_grain_only_near_the_border() -> None:
    """Edge handling decides what the blur sees beyond the picture, so it reaches truncate x size/2 px in, no further.

    Grain is rescaled to its standard deviation after the blur, so the interior agrees up to a scale: compare by
    correlation."""
    params = {"amount": 0.01, "size": 4.0, "truncate": 2.0, "seed": 9}  # the blur reaches 2 x 2 = 4 px
    reflect, wrap = added_noise({**params, "mode": "reflect"}), added_noise({**params, "mode": "wrap"})
    inner = (slice(5, -5), slice(5, -5))
    assert np.corrcoef(reflect[inner].ravel(), wrap[inner].ravel())[0, 1] > 1 - 1e-9
    assert np.corrcoef(reflect[:, :2].ravel(), wrap[:, :2].ravel())[0, 1] < 0.99


def test_truncate_sets_how_far_the_border_reaches() -> None:
    params = {"amount": 0.01, "size": 4.0, "seed": 9}
    band = (slice(10, -10), slice(5, 8))  # beyond a 2-sigma blur's reach (4 px) but inside a 4-sigma one's (8 px)

    def disagreement(truncate: float) -> float:
        """1 - correlation of the band under two edge modes: ~0 (float rounding) where the border does not reach."""
        reflect = added_noise({**params, "truncate": truncate, "mode": "reflect"})
        wrap = added_noise({**params, "truncate": truncate, "mode": "wrap"})
        return 1.0 - float(np.corrcoef(reflect[band].ravel(), wrap[band].ravel())[0, 1])

    assert disagreement(2.0) < 1e-9
    assert disagreement(4.0) > 1e-7  # the Gaussian's tail beyond 2 sigma is small, so the effect is too
