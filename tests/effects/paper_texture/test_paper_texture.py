"""Behaviour of the `paper_texture` effect: a fibre, canvas or linen texture laid over the picture's brightness."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "paper_texture"
KINDS = ["paper", "canvas", "linen"]


def grey(value: float = 0.5, size: int = 128) -> np.ndarray:
    return np.full((size, size, 3), value, np.float32)


def texture(settings: dict, size: int = 128) -> np.ndarray:
    """What the effect adds to a mid-grey picture, in the grey's units: (h, w). Small amounts never clip."""
    return (registry.apply(NAME, grey(size=size), **settings) - 0.5)[..., 0].astype(np.float64)


def peak_period(profile: np.ndarray) -> float:
    """The period in px of the strongest wave in a 1-D profile (not counting the mean)."""
    spectrum = np.abs(np.fft.rfft(profile - profile.mean()))
    return len(profile) / int(np.argmax(spectrum[1:]) + 1)


def autocorrelation(field: np.ndarray, lag: int) -> float:
    return float(np.corrcoef(field[:, :-lag].ravel(), field[:, lag:].ravel())[0, 1])


@pytest.mark.parametrize("kind", KINDS)
def test_amount_zero_leaves_the_picture_unchanged(kind: str, photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, kind=kind, amount=0.0)
    np.testing.assert_array_equal(out, photo)
    assert not np.shares_memory(out, photo)


@pytest.mark.parametrize("amount", [0.05, 0.1, 0.2])
@pytest.mark.parametrize("kind", KINDS)
def test_amount_is_the_relative_standard_deviation_and_the_mean_brightness_is_kept(kind: str, amount: float) -> None:
    """On a 0.5 grey the texture multiplies by 1 + amount x t, t of zero mean and unit deviation: std = 0.5 x amount."""
    out = texture({"kind": kind, "amount": amount, "seed": 2})
    assert out.std() == pytest.approx(0.5 * amount, rel=1e-3)
    assert out.mean() == pytest.approx(0.0, abs=1e-4)


def test_the_texture_scales_with_the_picture_and_leaves_black_black() -> None:
    colour = np.empty((64, 64, 3), np.float32)
    colour[...] = (0.1, 0.2, 0.4)  # dark enough that 1 + 0.1 x texture never reaches white
    out = registry.apply(NAME, colour, amount=0.1)
    ratio = out / colour
    np.testing.assert_allclose(ratio[..., 0], ratio[..., 1], atol=1e-5)  # one texture, every channel
    np.testing.assert_allclose(ratio[..., 0], ratio[..., 2], atol=1e-5)
    assert ratio[..., 0].std() > 0.05
    np.testing.assert_array_equal(registry.apply(NAME, np.zeros((16, 16, 3), np.float32), amount=1.0), 0.0)


@pytest.mark.parametrize("kind", KINDS)
def test_the_same_seed_gives_the_same_texture_and_another_seed_another(kind: str) -> None:
    first = texture({"kind": kind, "amount": 0.2, "seed": 1})
    np.testing.assert_array_equal(first, texture({"kind": kind, "amount": 0.2, "seed": 1}))
    assert np.abs(first - texture({"kind": kind, "amount": 0.2, "seed": 2})).mean() > 0.01


def test_a_larger_scale_correlates_paper_over_a_longer_distance() -> None:
    fine, medium, coarse = (
        autocorrelation(texture({"amount": 0.1, "scale": scale, "octaves": 1}, size=192), lag=4)
        for scale in (2.0, 8.0, 32.0)
    )
    assert fine < 0.5 < medium < coarse
    assert coarse > 0.9


@pytest.mark.parametrize("spacing", [8.0, 12.0, 16.0])
@pytest.mark.parametrize("kind", ["canvas", "linen"])
def test_a_woven_texture_repeats_every_thread_pitch(kind: str, spacing: float) -> None:
    """The strongest wave along the rows and along the columns has the period `spacing` (FFT peak at 1/spacing)."""
    out = texture({"kind": kind, "amount": 0.2, "spacing": spacing, "seed": 3}, size=192)
    assert peak_period(out.mean(axis=0)) == pytest.approx(spacing)  # threads running down: a profile across columns
    assert peak_period(out.mean(axis=1)) == pytest.approx(spacing)


def test_a_canvas_without_fibre_repeats_exactly() -> None:
    """With `fibre` 0 only the weave is left: canvas threads are alike, so shifting by one pitch changes nothing."""
    canvas = texture({"kind": "canvas", "amount": 0.2, "spacing": 8.0, "fibre": 0.0}, size=64)
    np.testing.assert_allclose(canvas[:, 8:], canvas[:, :-8], atol=1e-5)


@pytest.mark.parametrize("spacing", [8.0, 12.0])
def test_every_linen_thread_has_a_strength_of_its_own_across_its_whole_pitch(spacing: float) -> None:
    """Linen without fibre: a thread's wave cos(2 pi x / spacing) is scaled as a whole, so columns 1 and spacing - 1 of
    one pitch (equal cosines) agree, while column spacing - 1 and the next pitch's column 1 (equal cosines) differ."""
    pitch = int(spacing)
    out = texture({"kind": "linen", "amount": 0.2, "spacing": spacing, "fibre": 0.0, "seed": 1}, size=pitch * 6)
    for start in range(0, pitch * 5, pitch):
        np.testing.assert_allclose(out[:, start + 1], out[:, start + pitch - 1], atol=1e-5)
    assert np.abs(out[:, pitch - 1 : pitch * 5 : pitch] - out[:, pitch + 1 :: pitch]).min() > 1e-3


def test_linen_thread_strengths_follow_the_seed() -> None:
    first = texture({"kind": "linen", "amount": 0.2, "fibre": 0.0, "seed": 1})
    np.testing.assert_array_equal(first, texture({"kind": "linen", "amount": 0.2, "fibre": 0.0, "seed": 1}))
    assert np.abs(first - texture({"kind": "linen", "amount": 0.2, "fibre": 0.0, "seed": 2})).mean() > 0.01


@pytest.mark.parametrize("kind", ["canvas", "linen"])
@pytest.mark.parametrize("fibre", [0.25, 0.75])
def test_fibre_is_the_share_of_the_texture_variance_that_is_not_weave(kind: str, fibre: float) -> None:
    """The weave and the fibres are independent, so the texture's correlation with the pure weave is sqrt(1 - fibre)."""
    pure = texture({"kind": kind, "amount": 0.2, "spacing": 8.0, "fibre": 0.0, "seed": 4}, size=192)
    mixed = texture({"kind": kind, "amount": 0.2, "spacing": 8.0, "fibre": fibre, "seed": 4}, size=192)
    assert np.corrcoef(pure.ravel(), mixed.ravel())[0, 1] == pytest.approx(np.sqrt(1 - fibre), abs=0.05)


@pytest.mark.parametrize(
    ("smoother", "rougher"),
    [
        pytest.param({"octaves": 1}, {"octaves": 4, "persistence": 0.8}, id="more octaves add fine detail"),
        pytest.param({"persistence": 0.1}, {"persistence": 0.9}, id="more persistence keeps the fine layers strong"),
        pytest.param({"lacunarity": 1.2}, {"lacunarity": 3.0}, id="more lacunarity makes the layers finer"),
    ],
)
def test_the_fibre_layers_are_set_by_octaves_persistence_and_lacunarity(smoother: dict, rougher: dict) -> None:
    base = {"amount": 0.1, "scale": 24.0, "octaves": 3, "persistence": 0.8, "lacunarity": 2.0, "seed": 1}
    smooth = texture({**base, **smoother}, size=192)
    rough = texture({**base, **rougher}, size=192)
    assert autocorrelation(rough, lag=1) < autocorrelation(smooth, lag=1) - 0.01


def test_with_all_fibre_a_weave_is_the_paper_texture_and_paper_ignores_the_thread_pitch() -> None:
    paper = texture({"kind": "paper", "amount": 0.2, "seed": 6})
    np.testing.assert_allclose(texture({"kind": "canvas", "amount": 0.2, "fibre": 1.0, "seed": 6}), paper, atol=1e-5)
    np.testing.assert_array_equal(texture({"kind": "paper", "amount": 0.2, "spacing": 20.0, "seed": 6}), paper)
    assert np.abs(texture({"kind": "canvas", "amount": 0.2, "seed": 6}) - paper).mean() > 0.01
