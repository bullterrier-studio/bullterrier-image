"""Behaviour of the `film_grain` effect: grain whose strength follows the brightness, independent in every channel."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "film_grain"
# A weak grain, so that no value of a mid or dark patch is clipped (std 0.02 at black, 0.006 at white)
WEAK = {"shadows": 0.02, "highlights": 0.006}


def patch(colour: tuple[float, float, float], size: int = 128) -> np.ndarray:
    return np.broadcast_to(np.array(colour, np.float32), (size, size, 3)).copy()


def added(picture: np.ndarray, **settings) -> np.ndarray:
    """What the grain adds to `picture`: (h, w, 3), float64."""
    return (registry.apply(NAME, picture, **settings) - picture).astype(np.float64)


@pytest.mark.parametrize(
    ("brightness", "expected_std"),
    [
        pytest.param(0.1, 0.02 + (0.006 - 0.02) * 0.1, id="a tenth of the way from the shadows to the highlights"),
        pytest.param(0.5, 0.013, id="halfway: the mean of the two"),
        pytest.param(0.9, 0.02 + (0.006 - 0.02) * 0.9, id="nine tenths of the way"),
    ],
)
def test_the_grain_deviation_runs_linearly_from_shadows_at_black_to_highlights_at_white(
    brightness: float, expected_std: float
) -> None:
    """On a flat grey patch (the weak grain is never clipped at these levels)."""
    assert added(patch((brightness,) * 3), **WEAK, seed=1).std() == pytest.approx(expected_std, rel=0.03)


def test_brightness_is_the_luminance_of_the_pixel_for_all_three_channels() -> None:
    """Rec. 709 luminance of (0.2, 0.4, 0.8) is 0.2126 x 0.2 + 0.7152 x 0.4 + 0.0722 x 0.8 = 0.38626 (the mean of the
    channels would be 0.467): every channel's grain has the deviation at that brightness."""
    grain = added(patch((0.2, 0.4, 0.8)), **WEAK, seed=1)
    expected = 0.02 + (0.006 - 0.02) * 0.38626
    for channel in range(3):
        assert grain[..., channel].std() == pytest.approx(expected, rel=0.03)


def test_equal_shadows_and_highlights_give_the_same_grain_at_every_brightness() -> None:
    for brightness in (0.2, 0.5, 0.8):
        grain = added(patch((brightness,) * 3), shadows=0.01, highlights=0.01, seed=1)
        assert grain.std() == pytest.approx(0.01, rel=0.03)
        assert grain.mean() == pytest.approx(0.0, abs=0.0005)


def test_no_grain_at_zero_leaves_the_picture_unchanged(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, shadows=0.0, highlights=0.0)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)


def test_every_channel_has_grain_of_its_own() -> None:
    grain = added(patch((0.5, 0.5, 0.5)), **WEAK, seed=1)
    correlations = np.corrcoef(grain.reshape(-1, 3).T)
    assert np.abs(correlations[np.triu_indices(3, k=1)]).max() < 0.03


def test_size_clumps_the_grain_and_one_pixel_does_not() -> None:
    def neighbour_correlation(size: float) -> float:
        grain = added(patch((0.5,) * 3), **WEAK, size=size, seed=1)[..., 0]
        return float(np.corrcoef(grain[:, :-1].ravel(), grain[:, 1:].ravel())[0, 1])

    assert abs(neighbour_correlation(1.0)) < 0.03
    assert neighbour_correlation(3.0) > 0.5
    assert neighbour_correlation(6.0) > neighbour_correlation(3.0)


def test_edge_and_truncate_decide_what_the_clumping_blur_sees_beyond_the_border() -> None:
    """The blur (size 8: sigma 4) reaches truncate x 4 px in from the border: 4 px at 1, 16 px at 4."""
    base = {**WEAK, "size": 8.0, "seed": 2}
    flat = patch((0.5,) * 3, size=96)
    reflect_near, wrap_near = (added(flat, **base, truncate=1.0, edge=edge)[..., 0] for edge in ("reflect", "wrap"))
    reflect_far, wrap_far = (added(flat, **base, truncate=4.0, edge=edge)[..., 0] for edge in ("reflect", "wrap"))
    inner = (slice(8, -8), slice(8, -8))
    assert np.corrcoef(reflect_near[inner].ravel(), wrap_near[inner].ravel())[0, 1] > 1 - 1e-6  # out of reach
    assert np.corrcoef(reflect_far[inner].ravel(), wrap_far[inner].ravel())[0, 1] < 1 - 1e-6  # reached
    assert np.corrcoef(reflect_near[:, :2].ravel(), wrap_near[:, :2].ravel())[0, 1] < 0.99  # at the border


def test_the_same_seed_gives_the_same_grain_and_another_seed_other_grain(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, seed=1), registry.apply(NAME, photo, seed=1))
    assert np.abs(registry.apply(NAME, photo, seed=1) - registry.apply(NAME, photo, seed=2)).mean() > 0.001


def test_the_grain_of_a_picture_follows_its_own_brightness() -> None:
    """A dark left half (0.15) and a bright right half (0.85) with shadow grain only (0.03 at black, 0 at white): the
    deviation is 0.03 x (1 - brightness) in each half."""
    half = np.concatenate([patch((0.15,) * 3, 96)[:, :48], patch((0.85,) * 3, 96)[:, :48]], axis=1)
    grain = added(half, shadows=0.03, highlights=0.0, seed=3)
    assert grain[:, :48].std() == pytest.approx(0.03 * 0.85, rel=0.05)
    assert grain[:, 48:].std() == pytest.approx(0.03 * 0.15, rel=0.05)
