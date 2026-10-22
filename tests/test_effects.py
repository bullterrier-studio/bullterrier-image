import numpy as np
import pytest

from portrait_fx import effects
from portrait_fx.effects._common import parse_color
from portrait_fx.effects.ascii import _glyph_atlas

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


# --- halftone ---------------------------------------------------------------------------------------------------


def test_halftone_white_is_nearly_all_dots_and_black_is_blank():
    white, black = np.ones((64, 64, 3), np.float32), np.zeros((64, 64, 3), np.float32)
    assert effects.apply("halftone", white, fg="#ffffff").mean() > 0.9
    assert effects.apply("halftone", black, fg="#ffffff").mean() < 0.01
