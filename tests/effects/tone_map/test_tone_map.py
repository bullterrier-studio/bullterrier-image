"""Behaviour of the `tone_map` effect."""

from itertools import pairwise

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "tone_map"


def flat(colour: tuple[float, float, float] | float, shape: tuple[int, int] = (24, 24)) -> np.ndarray:
    return np.broadcast_to(np.array(colour, np.float32), (*shape, 3)).copy()


def two_blocks(dark: float = 0.1, bright: float = 0.8, shape: tuple[int, int] = (16, 64)) -> np.ndarray:
    """Left half `dark`, right half `bright`: a sharp edge between them, three stops apart for the defaults."""
    picture = flat(bright, shape)
    picture[:, : shape[1] // 2] = dark
    return picture


def checkerboard(mean: float = 0.25, swing: float = 0.1, shape: tuple[int, int] = (40, 40)) -> np.ndarray:
    """Fine detail on an even background: every other pixel `swing` brighter or darker than `mean`, a ratio of
    (1 + swing) / (1 - swing) between neighbours, about a third of a stop."""
    parity = (np.add.outer(np.arange(shape[0]), np.arange(shape[1])) % 2 == 0)[..., None]
    return flat(mean, shape) * np.where(parity, 1 + swing, 1 - swing).astype(np.float32)


def test_contrast_one_and_amount_zero_leave_the_picture_unchanged(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo, contrast=1.0, amount=0.0), photo, atol=1e-5)


# On an even picture base and detail are the colour's own log luminance and zero, so every channel is multiplied by
# Y^(contrast - 1), Y the Rec. 709 luma: a grey v becomes v^contrast. Worked out by hand (calculator) per row.
FLAT_ROWS = {
    "grey 0.25, contrast 0.5: the square root": (0.25, 0.5, 0.0, (0.5, 0.5, 0.5)),
    "grey 0.25, contrast 0: everything goes to white": (0.25, 0.0, 0.0, (1.0, 1.0, 1.0)),
    "grey 0.25, contrast 1: untouched": (0.25, 1.0, 0.0, (0.25, 0.25, 0.25)),
    "grey 0.25, contrast 0.25: the fourth root": (0.25, 0.25, 0.0, (0.25**0.25,) * 3),
    "detail has none to boost on an even picture": (0.25, 0.5, 4.0, (0.5, 0.5, 0.5)),
    "a colour keeps its channel ratios: luma 0.33582, gain 0.33582^-0.5": (
        (0.2, 0.4, 0.1),
        0.5,
        0.0,
        (0.3451253, 0.6902505, 0.1725626),
    ),
    "a very dark grey 0.001, contrast 0.5: 0.001^0.5": (0.001, 0.5, 0.0, (0.0316228,) * 3),
    "black stays black": (0.0, 0.0, 0.0, (0.0, 0.0, 0.0)),
    "white stays white": (1.0, 0.3, 0.0, (1.0, 1.0, 1.0)),
}


@pytest.mark.parametrize(("colour", "contrast", "amount", "expected"), FLAT_ROWS.values(), ids=list(FLAT_ROWS))
def test_an_even_picture_is_brought_up_by_the_hand_worked_gain(colour, contrast, amount, expected) -> None:
    out = registry.apply(NAME, flat(colour), contrast=contrast, amount=amount)
    np.testing.assert_allclose(out, flat(expected), atol=1e-5)


def test_lowering_contrast_never_darkens_and_lightens_what_is_dark_most(photo: np.ndarray) -> None:
    outs = [registry.apply(NAME, photo, contrast=c, amount=0.0) for c in (1.0, 0.75, 0.5, 0.0)]
    for before, after in pairwise(outs):
        assert (after >= before - 1e-6).all()
    gain = outs[-1].mean(axis=-1) - photo.mean(axis=-1)
    brightness = photo.mean(axis=-1)
    assert gain[brightness < 0.3].mean() > gain[brightness > 0.7].mean()


def test_the_base_is_compressed_so_the_two_halves_of_a_picture_come_closer_in_brightness() -> None:
    """A dark half (0.1) and a bright half (0.8), far from their edge, are flat, so each is a flat picture of its own:
    v^contrast, 0.1^0.5 = 0.3162 and 0.8^0.5 = 0.8944. Three stops apart become one and a half."""
    out = registry.apply(NAME, two_blocks(), contrast=0.5, amount=0.0, radius=4, edge_threshold=0.1)
    np.testing.assert_allclose(out[:, :20], 0.1**0.5, atol=1e-5)
    np.testing.assert_allclose(out[:, 44:], 0.8**0.5, atol=1e-5)


def test_the_detail_on_the_base_is_kept_while_the_base_is_compressed() -> None:
    """Fine texture on an even area keeps its neighbour-to-neighbour ratio (1.1 / 0.9) as the base is compressed, where
    a global curve v^0.5 would shrink it to the square root, 1.1055."""
    out = registry.apply(NAME, checkerboard(), contrast=0.5, amount=0.0, radius=6, edge_threshold=2.0)
    ratio = out[20, 20, 0] / out[20, 21, 0]  # (20 + 20) even: the brighter pixel; its neighbour the darker
    assert ratio == pytest.approx(1.1 / 0.9, rel=0.01)


@pytest.mark.parametrize("amount", [0.5, 1.0, 2.0])
def test_amount_raises_the_detail_by_that_much_in_stops(amount: float) -> None:
    """Detail is the log luminance minus the base; the output holds 1 + amount times it, so a neighbour ratio r
    becomes r^(1 + amount): (1.1 / 0.9) ^ 1.5, 2, 3."""
    out = registry.apply(NAME, checkerboard(), contrast=0.5, amount=amount, radius=6, edge_threshold=2.0)
    ratio = out[20, 20, 0] / out[20, 21, 0]
    assert ratio == pytest.approx((1.1 / 0.9) ** (1 + amount), rel=0.02)


def test_edge_threshold_keeps_a_step_above_it_out_of_the_base() -> None:
    """The 1 stop step between 0.2 and 0.4. With a threshold of 0.1 stop the dark pixel next to the edge keeps its own
    side of the base and comes out as the far dark pixels do; with 4 stops the base blends the two and it is lifted
    less."""
    picture = two_blocks(0.2, 0.4)
    kwargs = {"contrast": 0.5, "amount": 0.0, "radius": 4}
    sharp = registry.apply(NAME, picture, edge_threshold=0.1, **kwargs)[8, :, 0]
    soft = registry.apply(NAME, picture, edge_threshold=4.0, **kwargs)[8, :, 0]
    assert sharp[31] == pytest.approx(sharp[0], rel=0.02)
    assert soft[31] < sharp[31] * 0.95


def test_a_larger_radius_mixes_more_of_the_bright_side_into_the_dark_sides_base() -> None:
    """A dark pixel 6 px from the edge, with a threshold above the step so the base averages across it: a radius of 2
    reaches 4 px (the filter's window of the window) and sees nothing of the bright half, 4 and 8 see more and more, so
    the pixel is lifted less and less."""
    picture = two_blocks()
    outs = [
        float(registry.apply(NAME, picture, contrast=0.5, amount=0.0, radius=r, edge_threshold=8.0)[8, 26, 0])
        for r in (2, 4, 8)
    ]
    assert outs[0] == pytest.approx(0.1**0.5, abs=1e-5)
    assert all(later < earlier - 0.01 for earlier, later in pairwise(outs))


def test_the_base_stays_within_the_brightness_the_picture_has() -> None:
    """Per-channel noise of 0.1 and 0.6 has edges everywhere, which a guided filter overshoots. The base may not go
    beyond the darkest and brightest luma Y of the picture, so with `amount` 0 every pixel's gain is between
    Ymax^(contrast - 1) and Ymin^(contrast - 1), the gains of the brightest and the darkest area."""
    rng = np.random.default_rng(3)
    picture = np.where(rng.random((40, 50, 3)) > 0.5, 0.6, 0.1).astype(np.float32)
    luma = picture @ np.array([0.2126, 0.7152, 0.0722], np.float32)
    out = registry.apply(NAME, picture, contrast=0.5, amount=0.0, radius=3, edge_threshold=0.05)
    gain = out[..., 0] / picture[..., 0]
    assert (gain >= luma.max() ** -0.5 - 1e-4).all()
    assert (gain <= luma.min() ** -0.5 + 1e-4).all()
