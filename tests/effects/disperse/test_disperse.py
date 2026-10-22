"""Behaviour of the `disperse` effect."""

import numpy as np
import pytest

from photo_fx import effects
from photo_fx.core import registry
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
    ("sweep", "first_half", "last_half"), [(1.0, slice(0, 80), slice(80, 160)), (-1.0, slice(80, 160), slice(0, 80))]
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
    centre = lambda img: float((img.max(axis=(0, 2)) * np.arange(200)).sum() / img.max(axis=(0, 2)).sum())  # noqa: E731  (a one-line helper)
    kwargs = {"progress": 0.25, "top": 0, "edge": 0, "sweep": 0, "angle": 0, "seed": 2}
    assert (
        centre(effects.apply("disperse", blob, wind=0.3, **kwargs))
        > 100
        > centre(effects.apply("disperse", blob, wind=-0.3, **kwargs))
    )


def test_disperse_neighbouring_frames_look_alike_so_the_motion_is_continuous():
    frame = lambda p, seed: effects.apply("disperse", _lit(), progress=p, seed=seed, wind=0.2)  # noqa: E731  (a one-line helper)
    continuous = np.abs(frame(0.50, 3) - frame(0.51, 3)).mean()  # one frame later, same pieces on the same paths
    re_rolled = np.abs(frame(0.50, 3) - frame(0.51, 4)).mean()  # what picking new random paths per frame would give
    assert continuous < re_rolled / 2


@pytest.mark.parametrize(("wind", "sign"), [(0.3, 1), (-0.3, -1)])
def test_disperse_a_piece_pulls_back_against_the_wind_before_it_goes_with_it(wind, sign):
    cell_w, cell_h = cell_size(5)
    single = np.zeros((120, 400, 3), np.float32)
    single[5 * cell_h : 6 * cell_h, 40 * cell_w : 41 * cell_w] = 1.0  # exactly one piece, in the middle
    xs = []
    for progress in np.arange(0.0, 0.2, 0.004):  # with no jitter it winds up from 0 and leaves at 0.06 (`anticipation`)
        frame = effects.apply("disperse", single, progress=progress, wind=wind, angle=0, jitter=0.0, seed=3)
        columns = np.nonzero(frame.max(axis=(0, 2)) > 0.05)[0]
        if len(columns):
            xs.append(columns.mean())
    start = xs[0]
    first_move = next(x - start for x in xs if abs(x - start) > 1)
    assert first_move * sign < 0  # its first movement is against the wind (the wind-up)
    assert max((x - start) * sign for x in xs) > 20  # and then it is carried with the wind


def _one_piece(height=240, width=400):
    """Exactly one lit piece, near the middle, and its centre (y, x) in px."""
    cell_w, cell_h = cell_size(5)
    row, col = height // cell_h // 2, width // cell_w // 2
    img = np.zeros((height, width, 3), np.float32)
    img[row * cell_h : (row + 1) * cell_h, col * cell_w : (col + 1) * cell_w] = 1.0
    return img, ((row + 0.5) * cell_h, (col + 0.5) * cell_w)


def _centre(frame: np.ndarray) -> tuple[float, float]:
    """(x, y) of the centre of the visible pixels, in pixel-centre coordinates (+0.5)."""
    ys, xs = np.nonzero(frame.max(axis=2) > 0.05)
    return xs.mean() + 0.5, ys.mean() + 0.5


def _track(img, until=0.7, step=0.004, since=0.0, **params):
    """Where the lit pixels' centre is at each moment of the dispersal: [(progress, x, y), ...]."""
    out = []
    for progress in np.arange(since, until, step):
        shown = effects.apply("disperse", img, progress=progress, **params).max(axis=2) > 0.05
        if shown.any():
            ys, xs = np.nonzero(shown)
            out.append((progress, xs.mean(), ys.mean()))
    return out


# A known schedule for a lone piece: with no jitter it leaves at `anticipation` (0.1) and, with no variation, vanishes
# `flight` (0.2) later, so at progress 0.1 + 0.2 t it is a share t through its flight; with no fade it stays visible.
_TIMED = {"anticipation": 0.1, "flight": 0.2, "flight_variation": 0.0, "jitter": 0.0, "falloff": 0.0}


def _through_flight(share: float) -> float:
    return 0.1 + 0.2 * share


@pytest.mark.parametrize(("wind", "angle"), [(0.3, 30), (0.3, 0), (-0.3, 30), (0.25, 60)])
def test_disperse_wind_blows_at_the_given_angle_above_the_horizontal(wind, angle):
    img, (y0, x0) = _one_piece()
    late = effects.apply(
        "disperse", img, progress=_through_flight(0.95), wind=wind, angle=angle, sway=0.0, seed=3, **_TIMED
    )
    x, y = _centre(late)
    dx, rise = x - x0, y0 - y  # rise is positive for upward
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
            farthest = max(farthest, int(columns.max()))
    assert farthest <= 20 + 400 / 3


def test_disperse_hover_floats_a_piece_slowly_with_the_wind_long_before_it_goes():
    img, (_, x0) = _one_piece()
    kwargs = {"wind": 0.3, "angle": 30, "seed": 3}
    drift = lambda hover: next(x for p, x, _ in _track(img, until=0.06, hover=hover, **kwargs) if p >= 0.05) - x0  # noqa: E731  (a one-line helper)
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
    shimmer = lambda clock, wind: effects.apply("disperse", grey, progress=0.0, ambient=0.08, clock=clock, wind=wind)  # noqa: E731  (a one-line helper)
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


@pytest.mark.parametrize(("reach", "goes"), [(0.5, 0.5), (1.0, 1.0)])
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


@pytest.mark.parametrize("progress", [0.1, 0.3, 0.5, 0.7, 0.9])
def test_disperse_glowing_and_shimmering_pieces_never_go_above_white(progress):
    out = effects.apply("disperse", _lit(), progress=progress, ambient=0.1, clock=0.7, seed=1)
    assert out.max() <= 1.0
    assert out.min() >= 0.0


def _random_settings(rng: np.random.Generator) -> dict[str, object]:
    """A value for every number of the effect, drawn from its documented range (ints whole), and a random switch."""
    model = registry.parameters("disperse")
    assert model is not None
    settings: dict[str, object] = {"open_bottom": bool(rng.integers(2))}
    for doc in model.fields_doc():
        if doc.minimum is None or doc.maximum is None or doc.name in ("cell", "seed"):
            continue
        settings[doc.name] = float(rng.uniform(doc.minimum, doc.maximum))
    settings["cell"] = int(rng.integers(1, 12))
    settings["seed"] = int(rng.integers(0, 1000))
    return settings


@pytest.mark.parametrize("draw", range(40))
def test_disperse_stays_within_0_to_1_whatever_the_settings(draw):
    """Any mix of in-range values, at a random moment: finite, same shape, 0..1 (the glow and the shimmer brighten)."""
    rng = np.random.default_rng(draw)
    picture = np.clip(rng.random((30, 44, 3), dtype=np.float32) * 1.6, 0, 1).astype(np.float32)  # mostly white
    matte = np.zeros((30, 44), np.float32)
    matte[8:, 10:34] = 1.0
    out = effects.apply("disperse", picture, matte=matte, **_random_settings(rng))
    assert out.shape == picture.shape
    assert np.isfinite(out).all()
    assert out.min() >= 0.0
    assert out.max() <= 1.0


@pytest.mark.parametrize("progress", [0.0, 0.25, 0.5, 0.75, 1.0])
def test_disperse_every_frame_of_an_animated_progress_is_a_valid_value(progress):
    """`progress` and `clock` are what recipes animate; every frame of that range is accepted and well-formed."""
    out = effects.apply("disperse", _lit(), progress=progress, clock=3600.0 * progress, ambient=0.1)
    assert np.isfinite(out).all()
    assert out.min() >= 0.0
    assert out.max() <= 1.0


def test_disperse_a_shorter_flight_keeps_at_most_a_third_of_the_pieces_in_flight():
    """The motion-design 1/3 rule (no more than a third of the elements moving at once) is reachable by shortening the
    flight; the default flight is not promised to meet it."""
    img = _isolated_pieces()
    kwargs = {"top": 0, "edge": 0, "sweep": 1.0, "wind": 0.0, "seed": 1}
    frames = [effects.apply("disperse", img, progress=p, flight=0.1, **kwargs) for p in np.linspace(0, 1, 51)]
    assert max(_share_in_flight(frame) for frame in frames) <= 1 / 3


def test_disperse_without_falloff_or_flare_pieces_keep_their_brightness_until_they_vanish():
    img = _isolated_pieces()
    kwargs = {"progress": 0.5, "top": 0, "edge": 0, "sweep": 1.0, "wind": 0.2, "seed": 1}
    plain = effects.apply("disperse", img, falloff=0.0, flare=0.0, **kwargs)
    assert set(np.unique(plain)) == {0.0, 1.0}
    shaded = effects.apply("disperse", img, **kwargs)
    assert ((shaded > 0) & (shaded < 1)).any()  # by default they fade on the way


def test_disperse_without_flare_nothing_gets_brighter():
    grey = np.full((120, 160, 3), 0.5, np.float32)
    for progress in np.arange(0.1, 0.9, 0.05):
        assert effects.apply("disperse", grey, progress=progress, sweep=0.5, seed=1, wind=0.1, flare=0.0).max() <= 0.5


@pytest.mark.parametrize(("wind", "sign"), [(0.3, 1), (-0.3, -1)])
def test_disperse_without_pullback_a_piece_moves_with_the_wind_from_the_start(wind, sign):
    cell_w, cell_h = cell_size(5)
    single = np.zeros((120, 400, 3), np.float32)
    single[5 * cell_h : 6 * cell_h, 40 * cell_w : 41 * cell_w] = 1.0
    xs = []
    for progress in np.arange(0.0, 0.6, 0.004):
        frame = effects.apply("disperse", single, progress=progress, wind=wind, angle=0, pullback=0.0, seed=3)
        columns = np.nonzero(frame.max(axis=(0, 2)) > 0.05)[0]
        if len(columns):
            xs.append(columns.mean())
    first_move = next(x - xs[0] for x in xs if abs(x - xs[0]) > 1)
    assert first_move * sign > 0


def test_disperse_finish_is_when_the_last_piece_has_vanished():
    assert effects.apply("disperse", _lit(), progress=0.6, finish=0.6, seed=1).max() == 0.0
    assert effects.apply("disperse", _lit(), progress=0.6, seed=1).max() > 0.0  # by default (0.98) some remain


def test_disperse_shimmer_repeats_after_one_period_and_one_wavelength():
    grey = np.full((60, 200, 3), 0.5, np.float32)
    kwargs = {"progress": 0.0, "ambient": 0.08, "wind": 0.2, "ambient_period": 2.0, "ambient_wavelength": 0.25}
    at = lambda clock: effects.apply("disperse", grey, clock=clock, **kwargs)  # noqa: E731  (a one-line helper)
    np.testing.assert_allclose(at(0.3), at(2.3), atol=1e-5)  # one period later
    assert np.abs(at(0.3) - at(1.3)).max() > 0.01  # half a period later it differs
    row = at(0.3)[30, :, 0]
    np.testing.assert_allclose(row[:150], row[50:], atol=1e-5)  # a wavelength is 0.25 x 200 = 50 px


def test_disperse_without_jitter_a_side_to_side_front_is_a_straight_line():
    def front_columns(frame: np.ndarray) -> set[int]:
        """Per row of pixels, the column where the pieces still at home begin."""
        at_home = (frame == 1.0).all(axis=2)
        return {int(np.argmax(row)) for row in at_home}

    # no glow, so a piece that has left is dimmer than one at home
    kwargs = {"progress": 0.4, "top": 0, "edge": 0, "sweep": 1.0, "wind": 0.0, "sway": 0.0, "flare": 0.0, "seed": 1}
    assert len(front_columns(effects.apply("disperse", _lit(), jitter=0.0, **kwargs))) == 1  # the same in every row
    assert len(front_columns(effects.apply("disperse", _lit(), **kwargs))) > 3  # by default the front is ragged


def test_disperse_without_wind_or_sway_pieces_fade_where_they_are():
    img = _isolated_pieces()
    kwargs = {"progress": 0.5, "top": 0, "edge": 0, "sweep": 1.0, "wind": 0.0, "seed": 1}
    still = effects.apply("disperse", img, sway=0.0, **kwargs)
    swaying = effects.apply("disperse", img, **kwargs)
    dark = img[..., 0] == 0
    assert still[dark].max() == 0.0  # nothing appears outside where the pieces were
    assert swaying[dark].max() > 0.0  # by default they sway a few px as they fade


def test_disperse_acceleration_holds_a_piece_back_early_in_its_flight():
    img, (_, x0) = _one_piece()
    level = {"wind": 0.3, "angle": 0, "pullback": 0.0, "angle_spread": 0.0, "sway": 0.0, "seed": 3}

    def halfway(acceleration: float) -> float:
        """How far the piece has moved half-way through its flight, in px."""
        frame = effects.apply(
            "disperse", img, progress=_through_flight(0.5), acceleration=acceleration, **level, **_TIMED
        )
        return _centre(frame)[0] - x0

    # half-way through its flight a steady piece has covered half its journey; an accelerating one less
    assert halfway(5.0) < halfway(0.0) - 10
    assert halfway(0.0) > 0


def test_disperse_hover_without_spread_floats_the_whole_figure_at_once():
    block = np.ones((200, 400, 3), np.float32)
    kwargs = {"progress": 0.3, "hover": 0.3, "wind": 0.3, "angle": 30, "sweep": -1.0, "top": 0, "edge": 0, "seed": 1}
    inside = (slice(100, 140), slice(120, 160))
    assert effects.apply("disperse", block, hover_spread=0.0, **kwargs)[inside].min() < 1.0
    assert effects.apply("disperse", block, **kwargs)[inside].min() == 1.0  # by default the inside waits its turn


def test_disperse_bob_is_the_only_vertical_motion_of_a_level_hover():
    img, (y0, _) = _one_piece()
    kwargs = {"wind": 0.3, "angle": 0, "angle_spread": 0.0, "hover": 0.3, "seed": 3}
    hovering = lambda bob: [y for _, _, y in _track(img, until=0.05, bob=bob, **kwargs)]  # noqa: E731  (long before it lets go)
    assert all(y == y0 - 0.5 for y in hovering(0.0))  # pixel centres: 0.5 above y0
    assert any(y != y0 - 0.5 for y in hovering(0.05))


def test_disperse_outward_needs_a_distance_to_throw_pieces():
    kwargs = {"progress": 0.4, "seed": 2}
    np.testing.assert_array_equal(
        effects.apply("disperse", _lit(), outward=1.0, outward_distance=0.0, **kwargs),
        effects.apply("disperse", _lit(), outward=0.0, **kwargs),
    )
    assert not np.array_equal(
        effects.apply("disperse", _lit(), outward=1.0, **kwargs), effects.apply("disperse", _lit(), **kwargs)
    )


def test_disperse_bottom_goes_with_the_other_edges_only_when_open_bottom_is_off():
    kwargs = {"progress": 0.3, "top": 0, "edge": 1.0, "sweep": 0, "wind": 0.0, "seed": 1}
    bottom_centre = (slice(-10, None), slice(70, 90))
    assert effects.apply("disperse", _lit(), **kwargs)[bottom_centre].min() == 1.0
    assert effects.apply("disperse", _lit(), open_bottom=False, **kwargs)[bottom_centre].min() < 1.0


# With no ordering and no jitter every piece detaches at `anticipation`; with no wind, sway, fade or glow it stays where
# it is at full brightness until it vanishes, so a frame shows exactly which pieces have vanished.
_ALL_AT_ONCE = {"top": 0, "edge": 0, "sweep": 0, "jitter": 0.0, "wind": 0.0, "sway": 0.0, "falloff": 0.0, "flare": 0.0}


def test_disperse_without_flight_variation_every_piece_vanishes_exactly_flight_after_it_leaves():
    settings = {"anticipation": 0.1, "flight": 0.3, "flight_variation": 0.0, "seed": 1, **_ALL_AT_ONCE}
    np.testing.assert_array_equal(effects.apply("disperse", _lit(), progress=0.39, **settings), _lit())
    assert effects.apply("disperse", _lit(), progress=0.41, **settings).max() == 0.0
    varied = effects.apply("disperse", _lit(), progress=0.39, **{**settings, "flight_variation": 0.4})
    assert 0.0 < varied.mean() < 1.0  # by default some have vanished and some not


def test_disperse_the_wind_up_eases_back_over_the_whole_anticipation():
    img, (_, x0) = _one_piece()
    settings = {"wind": 0.3, "angle": 0, "angle_spread": 0.0, "jitter": 0.0, "pullback": 0.5, "seed": 3}
    x = {round(p, 3): x_ for p, x_, _ in _track(img, until=0.15, step=0.0375, anticipation=0.15, **settings)}
    halfway, almost = x0 - x[0.075], x0 - x[0.112]  # it leaves at progress 0.15, after winding up from 0
    assert 0 < halfway < almost


def test_disperse_glow_lasts_flare_span_of_the_flight():
    grey = np.zeros((120, 400, 3), np.float32)
    grey[60:70, 200:205] = 0.5  # one piece
    settings = {"flight": 0.2, "flight_variation": 0.0, "falloff": 0.0, "wind": 0.0, "sway": 0.0, "seed": 1}
    mid_flight = {"progress": 0.16, **_ALL_AT_ONCE, **settings, "flare": 0.4}  # it left at 0.06 and flies 0.2
    assert effects.apply("disperse", grey, flare_span=1.0, **mid_flight).max() == pytest.approx(0.7)  # at its peak
    assert effects.apply("disperse", grey, flare_span=0.2, **mid_flight).max() == pytest.approx(0.5)  # long over


def _reversals(values: list[float]) -> int:
    steps = np.sign(np.diff(values))
    steps = steps[steps != 0]
    return int((steps[1:] != steps[:-1]).sum())


def test_disperse_bob_cycles_is_how_often_a_hovering_piece_bobs():
    img, _ = _one_piece(height=120, width=200)
    # it hovers from the start until it leaves at `anticipation` (0.2), level with the wind, so it only bobs up and down
    settings = {"wind": 0.3, "angle": 0, "angle_spread": 0.0, "hover": 0.3, "bob": 0.05, "jitter": 0.0, "seed": 3}
    bobbing = lambda cycles: [  # noqa: E731  (a one-line helper)
        y for _, _, y in _track(img, until=0.2, step=0.005, anticipation=0.2, bob_cycles=cycles, **settings)
    ]
    assert _reversals(bobbing(0.0)) <= 1  # one rise and fall at most
    assert _reversals(bobbing(4.0)) >= 6


def test_disperse_sway_cycles_is_how_often_a_flying_piece_wobbles():
    img, _ = _one_piece(height=120, width=200)
    settings = {"wind": 0.0, "sway": 0.2, "seed": 3, **_TIMED}
    wobbling = lambda cycles: [  # noqa: E731  (a one-line helper)
        x for _, x, _ in _track(img, since=0.1, until=0.3, step=0.005, sway_cycles=cycles, **settings)
    ]
    assert _reversals(wobbling(0.0)) == 0  # it drifts to one side and does not come back
    assert _reversals(wobbling(4.0)) >= 6


@pytest.mark.parametrize("seed", range(4))
def test_disperse_without_angle_spread_every_piece_flies_at_the_wind_angle(seed):
    img, (y0, x0) = _one_piece()
    settings = {"wind": 0.3, "angle": 30, "angle_spread": 0.0, "sway": 0.0, "seed": seed, **_TIMED}
    x, y = _centre(effects.apply("disperse", img, progress=_through_flight(0.95), **settings))
    assert np.degrees(np.arctan2(y0 - y, x - x0)) == pytest.approx(30, abs=1.5)


def test_disperse_distance_minimum_one_carries_every_piece_the_whole_wind():
    """Eight pieces, each in its own row of pieces, blown level: each one's own random speed no longer matters."""
    cell_w, cell_h = cell_size(5)
    img = np.zeros((16 * cell_h, 400, 3), np.float32)
    for row in range(0, 16, 2):
        img[row * cell_h : (row + 1) * cell_h, :cell_w] = 1.0
    level = {"wind": 0.3, "angle": 0, "angle_spread": 0.0, "sway": 0.0, "pullback": 0.0, "top": 0, "edge": 0}
    late = effects.apply(
        "disperse", img, progress=_through_flight(0.98), distance_minimum=1.0, seed=1, **level, **_TIMED
    )
    for row in range(0, 16, 2):
        columns = np.nonzero(late[row * cell_h : (row + 1) * cell_h].max(axis=(0, 2)) > 0.05)[0]
        assert columns.min() > 0.9 * 0.3 * 400, row  # nearly the whole journey (it ends at 1 as it vanishes)


def test_disperse_edge_depth_is_how_deep_closeness_to_the_edge_decides_the_order():
    """Deeper than `edge_depth` every piece counts as fully inside, so they all go last, together."""
    settings = {"progress": 0.5, **_ALL_AT_ONCE, "edge": 1.0, "flight_variation": 0.0, "seed": 1}
    shallow = (effects.apply("disperse", _lit(), edge_depth=0.05, **settings) == 1.0).mean()
    deep = (effects.apply("disperse", _lit(), edge_depth=1.0, **settings) == 1.0).mean()
    assert shallow > deep + 0.2


def test_disperse_takes_its_edge_from_the_matte_when_given():
    img = np.ones((200, 200, 3), np.float32)  # everything is lit...
    matte = np.zeros((200, 200), np.float32)
    matte[:, 80:120] = 1.0  # ...but the figure is only a thin column; the rest counts as background
    kwargs = {"progress": 0.5, "top": 0, "edge": 1.0, "sweep": 0, "seed": 1}
    with_matte = effects.apply("disperse", img, matte=matte, **kwargs)
    without = effects.apply("disperse", img, **kwargs)
    outside = (slice(150, None), slice(20, 60))
    assert with_matte[outside].max() == 0.0  # outside the silhouette is all edge: it has gone by half-way
    assert without[outside].mean() > 0.8  # with no matte only the picture's borders are edges, so most of it stays
    assert with_matte[150:, 95:105].min() == 1.0  # the middle of the figure waits its turn


def _ragged_picture() -> np.ndarray:
    """Lit and dark pieces at random, so the outline turns at every piece."""
    cell_w, cell_h = cell_size(5)
    lit = np.random.default_rng(0).random((20, 40)) < 0.5
    return np.kron(lit, np.ones((cell_h, cell_w)))[..., None].repeat(3, axis=2).astype(np.float32)


def test_disperse_normal_smoothing_changes_which_way_pieces_are_thrown():
    kwargs = {"progress": 0.4, "outward": 1.0, "seed": 1}
    sharp = effects.apply("disperse", _ragged_picture(), normal_smoothing=1.0, **kwargs)
    blurred = effects.apply("disperse", _ragged_picture(), normal_smoothing=8.0, **kwargs)
    assert not np.array_equal(sharp, blurred)
    for out in (sharp, blurred):
        assert out.min() >= 0.0
        assert out.max() <= 1.0
