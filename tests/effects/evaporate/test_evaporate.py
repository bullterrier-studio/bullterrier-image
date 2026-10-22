"""Behaviour of the `evaporate` effect."""

import numpy as np
import pytest

from photo_fx import effects
from photo_fx.ops.glyphs import cell_size


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
    assert out[64:].mean() > 0.95
    assert out[:20].mean() < 0.5


def test_evaporate_only_moves_pieces_up_and_never_adds_light():
    upper_half = np.zeros((96, 128, 3), np.float32)
    upper_half[:48] = 0.8
    out = effects.apply("evaporate", upper_half, cell=5, start=0.9, end=0.4, rise=0.3)
    assert out[48 + cell_size(5)[1] :].max() == 0.0  # nothing sank into the dark lower half
    assert out.max() <= upper_half.max() + 1e-6


def test_evaporate_vanish_controls_how_many_pieces_survive_the_trip():
    lit = np.ones((120, 128, 3), np.float32)
    kwargs = {"cell": 5, "start": 0.9, "end": 0.5, "rise": 0.4, "seed": 2}
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
    # pieces flew out of both sides
    assert out[:, :55].max() > 0
    assert out[:, 145:].max() > 0


def test_evaporate_outward_sends_pieces_away_from_the_edge_they_came_from():
    sides = lambda img: img[:, :55].sum() + img[:, 145:].sum()  # noqa: E731  (a one-line helper)
    kwargs = {"cell": 5, "start": 0, "edge": 0.1, "seed": 1}
    assert sides(effects.apply("evaporate", _block(), outward=0.8, **kwargs)) > sides(
        effects.apply("evaporate", _block(), outward=0.0, **kwargs)
    )


def test_evaporate_wind_makes_the_outward_drift_lopsided():
    left_and_right = lambda img: (img[:, :55].sum(), img[:, 145:].sum())  # noqa: E731  (a one-line helper)
    left, right = left_and_right(effects.apply("evaporate", _block(), cell=5, start=0, edge=0.1, outward=0.4, wind=3.0))
    assert right > left


def test_evaporate_treats_the_left_right_and_top_borders_as_edges_but_not_the_bottom():
    lit = np.ones((100, 120, 3), np.float32)
    out = effects.apply("evaporate", lit, cell=5, start=0, edge=0.2, seed=2)
    assert out[:5, 50:70].mean() < 1.0
    assert out[40:80, :5].mean() < 1.0
    assert out[40:80, -5:].mean() < 1.0
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
    kwargs = {"cell": 5, "start": 0, "edge": 0.1, "outward": 0.5, "seed": 1}
    for given in (matte, None):  # with the real silhouette, and with the fallback that fills enclosed gaps
        out = effects.apply("evaporate", img, matte=given, **kwargs)
        assert out[100:140, 80:120][img[100:140, 80:120] > 0].min() == 1.0  # lit pieces around the gap survive


def test_evaporate_takes_its_edge_from_the_matte_when_given():
    img = np.ones((200, 200, 3), np.float32)  # everything is lit...
    matte = np.zeros((200, 200), np.float32)
    matte[:, 80:120] = 1.0  # ...but the figure is only a thin column; the rest counts as background
    kwargs = {"cell": 5, "start": 0, "edge": 0.05, "seed": 1}
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
    kwargs = {"cell": 5, "start": 1.0, "end": 0.5, "rise": 0.5, "seed": 3}
    right = effects.apply("evaporate", lit, wind=0.8, **kwargs)
    left = effects.apply("evaporate", lit, wind=-0.8, **kwargs)
    columns = np.arange(200)
    centre = lambda img: float((img.max(axis=(0, 2)) * columns).sum() / img.max(axis=(0, 2)).sum())  # noqa: E731  (a one-line helper)
    assert centre(right) > centre(left)


def test_evaporate_bottom_border_is_an_edge_only_when_open_bottom_is_off():
    lit = np.ones((100, 120, 3), np.float32)
    kwargs = {"cell": 5, "start": 0, "edge": 0.2, "seed": 2}
    assert effects.apply("evaporate", lit, **kwargs)[-10:, 50:70].min() == 1.0
    assert effects.apply("evaporate", lit, open_bottom=False, **kwargs)[-10:, 50:70].mean() < 1.0


def test_evaporate_without_falloff_keeps_risen_pieces_at_full_brightness():
    lit = np.ones((120, 128, 3), np.float32)
    kwargs = {"cell": 5, "start": 0.9, "end": 0.5, "rise": 0.4, "seed": 2}
    dimmed = effects.apply("evaporate", lit, **kwargs)
    undimmed = effects.apply("evaporate", lit, falloff=0.0, **kwargs)
    assert set(np.unique(undimmed)) == {0.0, 1.0}  # every piece is either gone or as bright as it was
    assert ((dimmed > 0) & (dimmed < 1)).any()  # by default they fade with height


def _one_column_of_pieces() -> np.ndarray:
    cell_w, cell_h = cell_size(5)
    img = np.zeros((40 * cell_h, 40 * cell_w, 3), np.float32)
    img[20 * cell_h :, 20 * cell_w : 21 * cell_w] = 1.0  # one character wide, from the middle down
    return img


def test_evaporate_without_sway_pieces_rise_straight_up():
    img = _one_column_of_pieces()
    kwargs = {"cell": 5, "start": 1.0, "end": 0.6, "rise": 0.5, "vanish": 0.0, "seed": 1}
    straight = effects.apply("evaporate", img, sway=0.0, sway_minimum=0.0, **kwargs)
    swaying = effects.apply("evaporate", img, **kwargs)
    column = slice(20 * 5, 21 * 5)
    assert straight[: img.shape[0] // 2, column].max() > 0  # pieces did rise...
    assert np.delete(straight, np.s_[column], axis=1).max() == 0.0  # ...and none left their column
    assert np.delete(swaying, np.s_[column], axis=1).max() > 0.0  # by default they sway sideways


def test_evaporate_thinning_and_vanish_decide_how_many_risen_pieces_survive():
    lit = np.ones((120, 128, 3), np.float32)
    kwargs = {"cell": 5, "start": 0.9, "end": 0.5, "rise": 0.4, "seed": 2, "vanish": 0.0}
    upper = slice(0, 40)
    every = effects.apply("evaporate", lit, thinning=0.0, **kwargs)[upper].sum()
    thinned = effects.apply("evaporate", lit, thinning=1.0, **kwargs)[upper].sum()
    assert thinned < every


@pytest.mark.parametrize(("field", "value"), [("vanish", 1.5), ("cell", 0), ("rise_gamma", -1.0), ("wind", 99.0)])
def test_evaporate_refuses_a_value_outside_its_range_naming_the_field(field, value):
    with pytest.raises(ValueError, match=field):
        effects.apply("evaporate", np.ones((20, 20, 3), np.float32), **{field: value})


def _one_row_of_pieces(row: int = 30) -> np.ndarray:
    """40 rows of 64 pieces, black but for piece row `row`."""
    cell_w, cell_h = cell_size(5)
    img = np.zeros((40 * cell_h, 64 * cell_w, 3), np.float32)
    img[row * cell_h : (row + 1) * cell_h] = 1.0
    return img


def _rows_with_content(frame: np.ndarray) -> set[int]:
    cell_h = cell_size(5)[1]
    return {int(r) // cell_h for r in np.nonzero(frame.max(axis=(1, 2)) > 0)[0]}


# Every detached piece is visible where it lands: none vanish, none dim, none sway out of their row.
_LANDING = {"cell": 5, "vanish": 0.0, "thinning": 0.0, "falloff": 0.0, "sway": 0.0, "sway_minimum": 0.0, "seed": 1}


def test_evaporate_rise_minimum_one_makes_every_fully_pulled_piece_climb_the_whole_rise():
    img = _one_row_of_pieces(30)
    pulled = {"start": 1.0, "end": 0.99, "rise": 0.25, **_LANDING}  # row 30 is far above `end`: every piece detaches
    assert _rows_with_content(effects.apply("evaporate", img, rise_minimum=1.0, **pulled)) == {20}  # 0.25 x 40 rows
    assert len(_rows_with_content(effects.apply("evaporate", img, **pulled))) > 3  # by default climbs differ


def test_evaporate_rise_gamma_zero_lets_weakly_pulled_pieces_climb_as_far_as_strongly_pulled_ones():
    img = _one_row_of_pieces(30)
    weak = {"start": 1.0, "end": 0.0, "rise": 0.25, "rise_minimum": 1.0, **_LANDING}  # row 30: pulled at about 0.14
    assert _rows_with_content(effects.apply("evaporate", img, rise_gamma=0.0, **weak)) == {20, 30}
    assert 20 not in _rows_with_content(effects.apply("evaporate", img, **weak))  # default gamma: they climb less


def test_evaporate_rise_skew_above_one_keeps_most_pieces_low():
    img = _one_row_of_pieces(30)
    pulled = {"start": 1.0, "end": 0.99, "rise": 0.5, "rise_minimum": 0.0, **_LANDING}

    def mean_climb(skew: float) -> float:
        """Average height climbed by the lit pixels, in px."""
        profile = effects.apply("evaporate", img, rise_skew=skew, **pulled).max(axis=(1, 2))
        rows = np.arange(len(profile))
        return 30 * cell_size(5)[1] - float((rows * profile).sum() / profile.sum())

    assert mean_climb(4.0) < mean_climb(1.0) < mean_climb(0.25)
