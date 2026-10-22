import numpy as np
import pytest

from portrait_fx import effects
from portrait_fx.effects._common import parse_color
from portrait_fx.effects.ascii import _glyph_atlas, cell_size

ALL = sorted(effects.describe())


@pytest.mark.parametrize("name", ALL)
def test_every_effect_keeps_shape_dtype_and_range(name, photo):
    out = effects.apply(name, photo)
    assert out.shape == photo.shape
    assert out.dtype == np.float32
    assert np.isfinite(out).all()
    assert out.min() >= -1e-6 and out.max() <= 1 + 1e-6


@pytest.mark.parametrize("name", ALL)
def test_every_effect_is_deterministic_and_leaves_its_input_alone(name, photo):
    before = photo.copy()
    first, second = effects.apply(name, photo), effects.apply(name, photo)
    np.testing.assert_array_equal(first, second)
    np.testing.assert_array_equal(photo, before)


@pytest.mark.parametrize("name", ALL)
def test_every_effect_documents_itself(name):
    assert effects.describe()[name]


def test_unknown_effect_lists_choices(photo):
    with pytest.raises(KeyError, match="ascii"):
        effects.apply("nope", photo)


def test_parse_color_forms():
    np.testing.assert_allclose(parse_color("#ff8000"), [1.0, 128 / 255, 0.0], atol=1e-6)
    np.testing.assert_allclose(parse_color("#f00"), [1, 0, 0])
    np.testing.assert_allclose(parse_color([0.1, 0.2, 0.3]), [0.1, 0.2, 0.3])
    with pytest.raises(ValueError):
        parse_color("#12")


def test_fill_ignores_the_picture(photo):
    out = effects.apply("fill", photo, color="#102030")
    assert np.allclose(out, parse_color("#102030"))


def test_gradient_map_end_points_use_first_and_last_stop():
    ramp = np.linspace(0, 1, 256, dtype=np.float32)[None, :, None].repeat(3, axis=2).repeat(4, axis=0)
    out = effects.apply("gradient_map", ramp, stops=["#ff0000", "#00ff00", "#0000ff"])
    np.testing.assert_allclose(out[0, 0], [1, 0, 0], atol=0.02)
    np.testing.assert_allclose(out[0, -1], [0, 0, 1], atol=0.02)
    np.testing.assert_allclose(out[0, 128], [0, 1, 0], atol=0.05)


def test_gradient_map_needs_two_stops(photo):
    with pytest.raises(ValueError):
        effects.apply("gradient_map", photo, stops=["#000000"])


def test_hue_shift_full_turn_is_identity(photo):
    np.testing.assert_allclose(effects.apply("hue_shift", photo, degrees=360), photo, atol=1e-5)


def test_hue_shift_keeps_brightness_and_moves_colour():
    from portrait_fx.effects._common import luminance

    skin_and_wall = np.array([[[0.8, 0.6, 0.5], [0.5, 0.35, 0.3], [0.6, 0.4, 0.4]]], np.float32)  # in-gamut colours
    out = effects.apply("hue_shift", skin_and_wall, degrees=120)
    np.testing.assert_allclose(luminance(out), luminance(skin_and_wall), atol=1e-4)
    assert (out.argmax(axis=-1) != skin_and_wall.argmax(axis=-1)).all()  # the dominant channel moved


def test_split_tone_tints_without_changing_brightness(photo):
    from portrait_fx.effects._common import luminance

    midtones = 0.25 + photo * 0.5  # away from pure black and white, where a tint would clip and change brightness
    out = effects.apply("split_tone", midtones, amount=0.2)
    assert not np.allclose(out, midtones)
    np.testing.assert_allclose(luminance(out), luminance(midtones), atol=1e-4)
    np.testing.assert_array_equal(effects.apply("split_tone", photo, amount=0.0), photo)


def test_split_tone_leaves_pure_black_black():
    assert effects.apply("split_tone", np.zeros((4, 4, 3), np.float32), amount=1.0).max() == 0.0


def test_grade_exposure_one_stop_doubles_brightness():
    dark = np.full((8, 8, 3), 0.2, dtype=np.float32)
    assert np.allclose(effects.apply("grade", dark, exposure=1.0, contrast=1.0), 0.4, atol=1e-5)


def test_clarity_zero_amount_is_identity(photo):
    np.testing.assert_allclose(effects.apply("clarity", photo, amount=0.0), photo, atol=1e-6)


def test_glow_only_ever_brightens(photo):
    assert (effects.apply("glow", photo) >= photo - 1e-6).all()


def test_grain_seed_controls_noise(photo):
    a, b = effects.apply("grain", photo, seed=1), effects.apply("grain", photo, seed=2)
    assert not np.allclose(a, b)
    np.testing.assert_array_equal(a, effects.apply("grain", photo, seed=1))


def test_glitch_only_moves_pixels_around(photo):
    out = effects.apply("glitch", photo, seed=3)
    assert not np.array_equal(out, photo)
    for c in range(3):  # rolling permutes values within each channel
        np.testing.assert_array_equal(np.sort(out[..., c], axis=None), np.sort(photo[..., c], axis=None))


def test_glitch_text_strips_change_only_some_rows_and_leave_the_sliding_alone(photo):
    plain = effects.apply("glitch", photo, seed=3, ascii_share=0.0)
    texty = effects.apply("glitch", photo, seed=3, ascii_share=0.5, ascii_cell=6)
    changed_rows = (plain != texty).any(axis=(1, 2))
    assert changed_rows.any(), "no text was drawn"
    assert not changed_rows.all(), "text covered everything, so the sliding strips are not visible any more"


def test_glitch_with_text_is_deterministic_and_seed_dependent(photo):
    a = effects.apply("glitch", photo, seed=5, ascii_share=0.5)
    np.testing.assert_array_equal(a, effects.apply("glitch", photo, seed=5, ascii_share=0.5))
    assert not np.array_equal(a, effects.apply("glitch", photo, seed=6, ascii_share=0.5))


def test_glitch_text_share_is_a_fraction_of_the_strips(photo):
    some = (effects.apply("glitch", photo, seed=3, ascii_share=0.3) != effects.apply("glitch", photo, seed=3)).any(
        axis=(1, 2)
    )
    most = (effects.apply("glitch", photo, seed=3, ascii_share=1.0) != effects.apply("glitch", photo, seed=3)).any(
        axis=(1, 2)
    )
    assert most.sum() >= some.sum() > 0


# --- ascii ------------------------------------------------------------------------------------------------------


def test_ascii_atlas_is_sorted_from_least_to_most_ink():
    atlas = _glyph_atlas("@.:", 10, None)  # given deliberately out of order
    ink = atlas.reshape(len(atlas), -1).mean(axis=1)
    assert list(ink) == sorted(ink) and ink[0] < ink[-1]


def test_ascii_bright_areas_get_dense_glyphs_on_dark_background():
    atlas = _glyph_atlas(" .:-=+*#%@", 10, None)
    height, width = atlas.shape[1:]
    white, black = np.ones((height, width, 3), np.float32), np.zeros((height, width, 3), np.float32)
    on_dark = effects.apply("ascii", white, cell=10, fg="#ffffff", bg="#000000")
    assert on_dark.mean() == pytest.approx(atlas[-1].mean(), abs=1e-3)
    assert effects.apply("ascii", black, cell=10, fg="#ffffff", bg="#000000").max() == 0.0


def test_ascii_flips_the_ramp_on_a_light_background():
    black = np.zeros((20, 10, 3), np.float32)
    on_light = effects.apply("ascii", black, cell=10, fg="#000000", bg="#ffffff")
    assert on_light.mean() < 0.9  # dark picture on paper = dense dark glyphs, not blank paper


def test_ascii_needs_two_distinct_characters(photo):
    with pytest.raises(ValueError, match="at least two"):
        effects.apply("ascii", photo, chars="aaaa")


def test_ascii_handles_sizes_that_are_not_a_multiple_of_the_cell():
    odd = np.random.default_rng(0).random((37, 53, 3), dtype=np.float32)
    assert effects.apply("ascii", odd, cell=8).shape == odd.shape


def test_ascii_photo_keeps_the_picture_visible_behind_the_text(photo):
    flat = effects.apply("ascii", photo, cell=8, photo=0.0)
    over = effects.apply("ascii", photo, cell=8, photo=1.0)
    assert np.abs(over - photo).mean() < np.abs(flat - photo).mean() / 2  # much closer to the original
    assert not np.allclose(over, photo)  # but the text is still drawn


def test_glitch_ascii_photo_keeps_strip_detail(photo):
    flat = effects.apply("glitch", photo, seed=3, ascii_share=1.0, ascii_cell=6, ascii_photo=0.0)
    over = effects.apply("glitch", photo, seed=3, ascii_share=1.0, ascii_cell=6, ascii_photo=1.0)
    plain = effects.apply("glitch", photo, seed=3)
    assert np.abs(over - plain).mean() < np.abs(flat - plain).mean()


def test_cell_size_matches_the_grid_ascii_draws_on():
    cell_w, cell_h = cell_size(8)
    assert _glyph_atlas(" .:", 8, None).shape[1:] == (cell_h, cell_w)


# --- evaporate ---------------------------------------------------------------------------------------------------


def test_evaporate_leaves_everything_below_start_untouched():
    lit = np.ones((96, 128, 3), np.float32)
    out = effects.apply("evaporate", lit, cell=5, start=0.5, end=0.2)
    assert not np.array_equal(out, lit)
    first_safe_row = 48 + cell_size(5)[1]  # pieces are whole character rows, so the row straddling `start` may change
    np.testing.assert_array_equal(out[first_safe_row:], lit[first_safe_row:])


def test_evaporate_thins_out_the_top_more_than_the_bottom():
    lit = np.ones((96, 128, 3), np.float32)
    out = effects.apply("evaporate", lit, cell=5, start=0.8, end=0.3)
    assert out[:32].mean() < out[64:].mean()
    assert out[64:].mean() > 0.95 and out[:20].mean() < 0.5


def test_evaporate_only_moves_pieces_up_and_never_adds_light():
    upper_half = np.zeros((96, 128, 3), np.float32)
    upper_half[:48] = 0.8
    out = effects.apply("evaporate", upper_half, cell=5, start=0.9, end=0.4, rise=0.3)
    assert out[48 + cell_size(5)[1] :].max() == 0.0  # nothing sank into the dark lower half
    assert out.max() <= upper_half.max() + 1e-6


def test_evaporate_vanish_controls_how_many_pieces_survive_the_trip():
    lit = np.ones((120, 128, 3), np.float32)
    kwargs = dict(cell=5, start=0.9, end=0.5, rise=0.4, seed=2)
    keep_all = effects.apply("evaporate", lit, vanish=0.0, **kwargs)
    some = effects.apply("evaporate", lit, vanish=0.5, **kwargs)
    none = effects.apply("evaporate", lit, vanish=1.0, **kwargs)
    assert keep_all.sum() > some.sum() > none.sum()


def _block():
    """A lit 80x140 rectangle on black, standing on the bottom edge of a 200x200 picture."""
    img = np.zeros((200, 200, 3), np.float32)
    img[60:, 60:140] = 1.0
    return img


def test_evaporate_edge_sheds_the_outline_and_keeps_the_inside():
    out = effects.apply("evaporate", _block(), cell=5, start=0, edge=0.1, outward=0.5, seed=1)
    assert out[150:190, 90:110].min() == 1.0  # deep inside, more than `edge` from any edge, and the bottom is no edge
    assert not np.array_equal(out, _block())
    assert out[:, :55].max() > 0 and out[:, 145:].max() > 0  # pieces flew out of both sides


def test_evaporate_outward_sends_pieces_away_from_the_edge_they_came_from():
    sides = lambda img: img[:, :55].sum() + img[:, 145:].sum()  # noqa: E731
    kwargs = dict(cell=5, start=0, edge=0.1, seed=1)
    assert sides(effects.apply("evaporate", _block(), outward=0.8, **kwargs)) > sides(
        effects.apply("evaporate", _block(), outward=0.0, **kwargs)
    )


def test_evaporate_wind_makes_the_outward_drift_lopsided():
    left_and_right = lambda img: (img[:, :55].sum(), img[:, 145:].sum())  # noqa: E731
    left, right = left_and_right(effects.apply("evaporate", _block(), cell=5, start=0, edge=0.1, outward=0.4, wind=3.0))
    assert right > left


def test_evaporate_treats_the_left_right_and_top_borders_as_edges_but_not_the_bottom():
    lit = np.ones((100, 120, 3), np.float32)
    out = effects.apply("evaporate", lit, cell=5, start=0, edge=0.2, seed=2)
    assert out[:5, 50:70].mean() < 1.0 and out[40:80, :5].mean() < 1.0 and out[40:80, -5:].mean() < 1.0
    assert out[-10:, 50:70].min() == 1.0  # bottom centre: far from the other borders, and the bottom is no edge


def test_evaporate_without_edge_or_outward_ignores_the_outline():
    lit = np.zeros((100, 120, 3), np.float32)
    lit[50:, 30:90] = 1.0  # lit only below `start`
    out = effects.apply("evaporate", lit, cell=5, start=0.4, end=0.1)
    first_safe_row = 40 + cell_size(5)[1]
    np.testing.assert_array_equal(out[first_safe_row:], lit[first_safe_row:])


def _figure_with_a_dark_gap():
    """A lit rectangle with a black 'eye' inside it, and the matte of the whole rectangle."""
    img = _block()
    img[110:130, 90:110] = 0.0
    matte = np.zeros((200, 200), np.float32)
    matte[60:, 60:140] = 1.0
    return img, matte


def test_evaporate_does_not_treat_dark_parts_of_the_figure_as_its_edge():
    img, matte = _figure_with_a_dark_gap()
    kwargs = dict(cell=5, start=0, edge=0.1, outward=0.5, seed=1)
    for given in (matte, None):  # with the real silhouette, and with the fallback that fills enclosed gaps
        out = effects.apply("evaporate", img, matte=given, **kwargs)
        assert out[100:140, 80:120][img[100:140, 80:120] > 0].min() == 1.0  # lit pieces around the gap survive


def test_evaporate_takes_its_edge_from_the_matte_when_given():
    img = np.ones((200, 200, 3), np.float32)  # everything is lit...
    matte = np.zeros((200, 200), np.float32)
    matte[:, 80:120] = 1.0  # ...but the figure is only a thin column; the rest counts as background
    kwargs = dict(cell=5, start=0, edge=0.05, seed=1)
    with_matte = effects.apply("evaporate", img, matte=matte, **kwargs)
    without = effects.apply("evaporate", img, **kwargs)
    assert with_matte[150:, 20:60].mean() < 1.0  # outside the silhouette: sheds
    assert with_matte[150:, 95:105].min() == 1.0  # the middle of the figure is further than `edge` from its outline
    assert without[150:, 20:60].min() == 1.0  # with no matte there is no such outline (only the picture borders)


def test_evaporate_seed_controls_the_dissolve(photo):
    a, b = effects.apply("evaporate", photo, seed=1), effects.apply("evaporate", photo, seed=2)
    assert not np.array_equal(a, b)
    np.testing.assert_array_equal(a, effects.apply("evaporate", photo, seed=1))


def test_evaporate_wind_pushes_the_pieces_sideways():
    lit = np.zeros((200, 200, 3), np.float32)
    lit[120:140, 90:110] = 1.0  # a lit block near the middle
    kwargs = dict(cell=5, start=1.0, end=0.5, rise=0.5, seed=3)
    right = effects.apply("evaporate", lit, wind=0.8, **kwargs)
    left = effects.apply("evaporate", lit, wind=-0.8, **kwargs)
    columns = np.arange(200)
    centre = lambda img: float((img.max(axis=(0, 2)) * columns).sum() / img.max(axis=(0, 2)).sum())  # noqa: E731
    assert centre(right) > centre(left)


# --- disperse ----------------------------------------------------------------------------------------------------


def _lit(height=120, width=160):
    return np.ones((height, width, 3), np.float32)


def test_disperse_is_the_untouched_picture_at_progress_zero_and_nothing_at_one(photo):
    np.testing.assert_array_equal(effects.apply("disperse", photo, progress=0.0), photo)
    assert effects.apply("disperse", _lit(), progress=1.0, seed=1).max() == 0.0


def test_disperse_fades_steadily():
    means = [effects.apply("disperse", _lit(), progress=p, seed=1).mean() for p in (0.0, 0.25, 0.5, 0.75, 1.0)]
    assert means == sorted(means, reverse=True) and means[0] > 0.99 and means[-1] == 0.0


@pytest.mark.parametrize(
    "sweep, first_half, last_half", [(1.0, slice(0, 80), slice(80, 160)), (-1.0, slice(80, 160), slice(0, 80))]
)
def test_disperse_sweep_chooses_which_side_goes_first(sweep, first_half, last_half):
    out = effects.apply("disperse", _lit(), progress=0.5, top=0, edge=0, sweep=sweep, wind=0, angle=0, seed=1)
    assert out[:, first_half].mean() < out[:, last_half].mean() / 3


def test_disperse_reach_leaves_the_rest_standing():
    whole = effects.apply("disperse", _lit(), progress=1.0, reach=1.0, edge=0, seed=1)
    part = effects.apply("disperse", _lit(), progress=1.0, reach=0.4, edge=0, seed=1)
    assert whole.mean() == 0.0 and 0.4 < part.mean() < 0.8


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


@pytest.mark.parametrize("reach", [0.5, 1.0])
def test_disperse_keeps_the_pieces_in_flight_at_once_to_a_third_or_less(reach):
    from portrait_fx.effects.disperse import _schedule

    rng = np.random.default_rng(0)
    order, spread = rng.random(20000), rng.random(20000)
    detach_at, lifetime = _schedule(order, spread, reach=reach)
    goes = order <= reach
    peak = max((goes & (p > detach_at) & (p < detach_at + lifetime)).mean() for p in np.linspace(0, 1, 101))
    assert peak <= 1 / 3  # of ALL pieces, even when every one of them eventually goes


def test_disperse_seed_changes_the_motion_and_repeats_exactly():
    a = effects.apply("disperse", _lit(), progress=0.5, seed=1)
    np.testing.assert_array_equal(a, effects.apply("disperse", _lit(), progress=0.5, seed=1))
    assert not np.array_equal(a, effects.apply("disperse", _lit(), progress=0.5, seed=2))


# --- halftone ---------------------------------------------------------------------------------------------------


def test_halftone_white_is_nearly_all_dots_and_black_is_blank():
    white, black = np.ones((64, 64, 3), np.float32), np.zeros((64, 64, 3), np.float32)
    assert effects.apply("halftone", white, fg="#ffffff").mean() > 0.9
    assert effects.apply("halftone", black, fg="#ffffff").mean() < 0.01
