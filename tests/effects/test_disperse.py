"""Tests of `effects/disperse.py`."""

import numpy as np
import pytest

from photo_fx import effects
from photo_fx.ops.glyphs import cell_size


def _lit(height=120, width=160):
    return np.ones((height, width, 3), np.float32)


def test_disperse_is_the_untouched_picture_at_progress_zero_and_nothing_at_one(photo):
    np.testing.assert_array_equal(effects.apply("disperse", photo, progress=0.0), photo)
    assert effects.apply("disperse", _lit(), progress=1.0, seed=1).max() == 0.0


def test_disperse_fades_steadily():
    means = [effects.apply("disperse", _lit(), progress=p, seed=1).mean() for p in (0.0, 0.25, 0.5, 0.75, 1.0)]
    assert means == sorted(means, reverse=True)
    assert means[0] > 0.99
    assert means[-1] == 0.0


@pytest.mark.parametrize(
    "sweep, first_half, last_half", [(1.0, slice(0, 80), slice(80, 160)), (-1.0, slice(80, 160), slice(0, 80))]
)
def test_disperse_sweep_chooses_which_side_goes_first(sweep, first_half, last_half):
    out = effects.apply("disperse", _lit(), progress=0.5, top=0, edge=0, sweep=sweep, wind=0, angle=0, seed=1)
    assert out[:, first_half].mean() < out[:, last_half].mean() / 3


def test_disperse_reach_leaves_the_rest_standing():
    whole = effects.apply("disperse", _lit(), progress=1.0, reach=1.0, edge=0, seed=1)
    part = effects.apply("disperse", _lit(), progress=1.0, reach=0.4, edge=0, seed=1)
    assert whole.mean() == 0.0
    assert 0.4 < part.mean() < 0.8


def test_disperse_wind_carries_pieces_in_its_direction():
    blob = np.zeros((120, 200, 3), np.float32)
    blob[40:80, 80:120] = 1.0
    centre = lambda img: float((img.max(axis=(0, 2)) * np.arange(200)).sum() / img.max(axis=(0, 2)).sum())  # noqa: E731
    kwargs = dict(progress=0.25, top=0, edge=0, sweep=0, angle=0, seed=2)
    assert (
        centre(effects.apply("disperse", blob, wind=0.3, **kwargs))
        > 100
        > centre(effects.apply("disperse", blob, wind=-0.3, **kwargs))
    )


def test_disperse_neighbouring_frames_look_alike_so_the_motion_is_continuous():
    frame = lambda p, seed: effects.apply("disperse", _lit(), progress=p, seed=seed, wind=0.2)  # noqa: E731
    continuous = np.abs(frame(0.50, 3) - frame(0.51, 3)).mean()  # one frame later, same pieces on the same paths
    re_rolled = np.abs(frame(0.50, 3) - frame(0.51, 4)).mean()  # what picking new random paths per frame would give
    assert continuous < re_rolled / 2


@pytest.mark.parametrize("wind, sign", [(0.3, 1), (-0.3, -1)])
def test_disperse_a_piece_pulls_back_against_the_wind_before_it_goes_with_it(wind, sign):
    cell_w, cell_h = cell_size(5)
    single = np.zeros((120, 400, 3), np.float32)
    single[5 * cell_h : 6 * cell_h, 40 * cell_w : 41 * cell_w] = 1.0  # exactly one piece, in the middle
    xs = []
    for progress in np.arange(0.0, 0.6, 0.004):
        columns = np.nonzero(
            effects.apply("disperse", single, progress=progress, wind=wind, angle=0, seed=3).max(axis=(0, 2)) > 0.05
        )[0]
        if len(columns):
            xs.append(columns.mean())
    start = xs[0]
    first_move = next(x - start for x in xs if abs(x - start) > 1)
    assert first_move * sign < 0  # its first movement is against the wind (the wind-up)
    assert max((x - start) * sign for x in xs) > 20  # and then it is carried with the wind


def _one_piece(height=240, width=400):
    cell_w, cell_h = cell_size(5)
    img = np.zeros((height, width, 3), np.float32)
    img[12 * cell_h : 13 * cell_h, 40 * cell_w : 41 * cell_w] = 1.0  # exactly one piece, in the middle
    return img, (12.5 * cell_h, 40.5 * cell_w)


def _track(img, **params):
    """Where the lit pixels' centre is at each moment of the dispersal: [(progress, x, y), ...]."""
    out = []
    for progress in np.arange(0.0, 0.7, 0.004):
        shown = effects.apply("disperse", img, progress=progress, **params).max(axis=2) > 0.05
        if shown.any():
            ys, xs = np.nonzero(shown)
            out.append((progress, xs.mean(), ys.mean()))
    return out


@pytest.mark.parametrize("wind, angle", [(0.3, 30), (0.3, 0), (-0.3, 30), (0.25, 60)])
def test_disperse_wind_blows_at_the_given_angle_above_the_horizontal(wind, angle):
    img, (y0, x0) = _one_piece()
    track = _track(img, wind=wind, angle=angle, seed=3, sweep=-1.0)
    farthest = max(track, key=lambda step: abs(step[1] - x0) + abs(step[2] - y0))
    dx, rise = farthest[1] - x0, y0 - farthest[2]  # rise is positive for upward
    assert np.sign(dx) == np.sign(wind) or angle == 90  # to the right for positive wind, to the left for negative
    measured = np.degrees(np.arctan2(rise, abs(dx)))
    assert abs(measured - angle) < 9  # the pieces' own angles differ from the wind's by up to 6 degrees


@pytest.mark.parametrize("angle", [0, 30])
def test_disperse_no_piece_travels_more_than_a_third_of_the_picture_at_any_angle(angle):
    block = np.zeros((100, 400, 3), np.float32)
    block[:, :20] = 1.0
    farthest = 0
    for progress in np.arange(0.0, 1.01, 0.02):
        shown = effects.apply("disperse", block, progress=progress, wind=0.3, angle=angle, seed=4).max(axis=(0, 2))
        columns = np.nonzero(shown > 0.01)[0]
        if len(columns):
            farthest = max(farthest, columns.max())
    assert farthest <= 20 + 400 / 3


def test_disperse_hover_floats_a_piece_slowly_with_the_wind_long_before_it_goes():
    img, (y0, x0) = _one_piece()
    kwargs = dict(wind=0.3, angle=30, seed=3)
    drift = lambda hover: next(x for p, x, _ in _track(img, hover=hover, **kwargs) if p >= 0.05) - x0  # noqa: E731
    assert drift(0.3) > 1.0  # floating up and with the wind: nothing has detached yet
    assert drift(0.0) <= 0.5  # without hovering it stays put (or starts to pull back) at this point


def test_disperse_hover_starts_at_the_outline_and_leaves_the_inside_alone():
    block = np.ones((200, 400, 3), np.float32)
    out = effects.apply(
        "disperse", block, progress=0.3, hover=0.3, wind=0.3, angle=30, sweep=-1.0, top=0, edge=0, seed=1
    )
    assert out[100:140, 120:160].min() == 1.0  # well inside, and the front has not got here: not moving
    assert not np.array_equal(out[:, :10], block[:, :10])  # the picture's left border has been floating for a while


def test_disperse_glows_briefly_as_pieces_leave_but_never_flashes():
    grey = np.full((120, 160, 3), 0.5, np.float32)
    peak = max(
        effects.apply("disperse", grey, progress=p, sweep=0.5, seed=1, wind=0.1).max()
        for p in np.arange(0.1, 0.9, 0.05)
    )
    assert 0.5 + 0.05 < peak <= 0.5 * 1.4 + 1e-6  # brighter than before, but within the glow's cap (no white flash)


def test_disperse_ambient_shimmer_is_small_runs_with_the_clock_and_travels_against_the_wind():
    grey = np.full((60, 200, 3), 0.5, np.float32)
    shimmer = lambda clock, wind: effects.apply("disperse", grey, progress=0.0, ambient=0.08, clock=clock, wind=wind)  # noqa: E731
    assert np.abs(shimmer(0.7, 0.2) / 0.5 - 1).max() <= 0.08 + 1e-5  # never more than the amplitude
    assert not np.array_equal(shimmer(0.0, 0.2), shimmer(1.0, 0.2))  # alive while nothing detaches
    np.testing.assert_array_equal(
        effects.apply("disperse", grey, progress=0.0, ambient=0.0, clock=3.0), grey
    )  # off by default

    def drift(wind):  # how many columns the wave pattern moved to the left between clock 0 and 1 s
        before, after = shimmer(0.0, wind).mean(axis=(0, 2)), shimmer(1.0, wind).mean(axis=(0, 2))
        shifts = range(-19, 20)  # less than half a wavelength (40 columns), so the answer is unambiguous
        return max(shifts, key=lambda s: np.corrcoef(after[40:-40], before[40 + s : len(before) - 40 + s])[0, 1])

    assert drift(0.2) > 0 > drift(-0.2)  # wind to the right: shimmer to the left, and the other way round


GRID_ROWS, GRID_COLS, GRID_STEP = 30, 20, 3  # lit pieces in every other row and every third column


def _isolated_pieces() -> np.ndarray:
    """Lit pieces with dark pieces between them, so each can be watched on its own. With no wind a piece does not
    travel; it only sways a few px sideways as it fades, which never reaches its neighbour."""
    cell_w, cell_h = cell_size(5)
    img = np.zeros((2 * GRID_ROWS * cell_h, GRID_STEP * GRID_COLS * cell_w, 3), np.float32)
    for row in range(GRID_ROWS):
        for col in range(GRID_COLS):
            img[
                2 * row * cell_h : (2 * row + 1) * cell_h, GRID_STEP * col * cell_w : (GRID_STEP * col + 1) * cell_w
            ] = 1
    return img


def _share_in_flight(frame: np.ndarray) -> float:
    """The share of the pieces that have left and not yet vanished: neither at home at full brightness nor gone."""
    cell_w, cell_h = cell_size(5)
    flying = 0
    for row in range(GRID_ROWS):
        for col in range(GRID_COLS):
            rows = slice(2 * row * cell_h, (2 * row + 1) * cell_h)
            around = slice(max(0, (GRID_STEP * col - 1) * cell_w), (GRID_STEP * col + 2) * cell_w)
            brightest = frame[rows, around].max()
            flying += 0.0 < brightest != 1.0  # fading (or glowing) on its way out
    return flying / (GRID_ROWS * GRID_COLS)


@pytest.mark.parametrize("reach, goes", [(0.5, 0.5), (1.0, 1.0)])
def test_disperse_sweeping_across_keeps_fewer_than_half_of_the_pieces_in_flight_and_uses_the_whole_clip(reach, goes):
    """With the order set by a side-to-side sweep alone. The share depends on the order: other mixes of `top`, `edge`
    and `sweep` on a fragmented picture reach more than half, so this is not a promise for every setting."""
    img = _isolated_pieces()
    moments = np.linspace(0.0, 1.0, 51)
    frames = [
        effects.apply("disperse", img, progress=p, top=0, edge=0, sweep=1.0, wind=0.0, reach=reach, seed=1)
        for p in moments
    ]
    in_flight = [_share_in_flight(frame) for frame in frames]
    assert max(in_flight) < 1 / 2  # of ALL pieces, even when every one of them eventually goes
    assert max(p for p, share in zip(moments, in_flight, strict=True) if share > 0) >= 0.8  # the whole clip is used
    gone = (frames[-1] == 0).all(axis=2)
    assert gone[img[..., 0] == 1].mean() == pytest.approx(goes, abs=0.05)  # and `reach` of them have gone at the end


def test_disperse_seed_changes_the_motion_and_repeats_exactly():
    a = effects.apply("disperse", _lit(), progress=0.5, seed=1)
    np.testing.assert_array_equal(a, effects.apply("disperse", _lit(), progress=0.5, seed=1))
    assert not np.array_equal(a, effects.apply("disperse", _lit(), progress=0.5, seed=2))


def test_the_disperse_effects_are_in_their_categories():
    """disperse: where the effect catalogue lists them."""
    expected = {
        "disperse": "animation",
    }
    assert {name: effects.category(name) for name in expected} == expected
