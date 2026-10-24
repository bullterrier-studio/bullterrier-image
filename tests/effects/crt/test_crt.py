"""Behaviour of the `crt` effect: a curved glass screen with a shadow mask, scanlines, bloom and a vignette."""

from pathlib import Path

import numpy as np
import pytest

from bullterrier_image.core import registry
from bullterrier_image.recipe import load_recipe

NAME = "crt"
# every part of the screen off
OFF = {"curve": 0.0, "shadow_mask": 0.0, "scanlines": 0.0, "bloom": 0.0, "vignette": 0.0}


def crt(rgb: np.ndarray, **settings: object) -> np.ndarray:
    """The picture through a screen that has only the parts `settings` turns on."""
    return registry.apply(NAME, rgb, **{**OFF, **settings})


def flat(value: float, height: int = 8, width: int = 12) -> np.ndarray:
    return np.full((height, width, 3), value, np.float32)


def test_a_screen_with_every_part_off_shows_the_picture_as_it_is(photo: np.ndarray) -> None:
    out = crt(photo, border="#ff0000")
    np.testing.assert_allclose(out, photo, atol=1e-6)
    assert not np.shares_memory(out, photo)  # a new picture, not the input handed back


def test_a_scanline_darkens_each_row_by_its_distance_from_the_nearest_beam() -> None:
    """With spacing 4 the beams are centred on rows 0, 4, 8... and the dark between them is at 2, 6...: row r is
    darkened by `scanlines` x sin^2(pi r / 4) = 0, 0.5, 1, 0.5 of it, so at 0.5 the gains are 1, 0.75, 0.5, 0.75."""
    out = crt(flat(0.8), scanlines=0.5, scanline_spacing=4)
    gains = np.array([1, 0.75, 0.5, 0.75, 1, 0.75, 0.5, 0.75])
    np.testing.assert_allclose(out[:, :, 0], 0.8 * gains[:, None] * np.ones((1, 12)), atol=1e-6)
    assert (out == out[..., :1]).all()  # the same in every channel


@pytest.mark.parametrize(
    ("pitch", "lit_channel_of_each_column"),
    [(3, [0, 1, 2, 0, 1, 2]), (6, [0, 0, 1, 1, 2, 2])],
    ids=["pitch 3: one stripe a column", "pitch 6: two columns a stripe"],
)
def test_the_shadow_mask_lights_red_green_and_blue_stripes_and_keeps_the_average_brightness(
    pitch: int, lit_channel_of_each_column: list[int]
) -> None:
    """At mask 0.5 an unlit stripe passes half the light and a lit one all of it; a third of the columns are lit for
    each channel, so the mean gain is (1 + 0.5 + 0.5) / 3 = 2/3 and the gains are divided by it: 1.5 where lit, 0.75
    where not. A grey of 0.4 shows 0.6 on the lit stripe and 0.3 on the others."""
    out = crt(flat(0.4, 4, 6), shadow_mask=0.5, shadow_mask_pitch=pitch)
    expected = np.full((4, 6, 3), 0.3, np.float32)
    for column, channel in enumerate(lit_channel_of_each_column):
        expected[:, column, channel] = 0.6
    np.testing.assert_allclose(out, expected, atol=1e-6)


def test_bloom_screens_the_blurred_highlights_over_a_picture() -> None:
    """A flat grey of 0.8 with threshold 0.5: the highlights are (0.8 - 0.5) / (1 - 0.5) = 0.6 everywhere, a blur of a
    flat picture is flat, and screening 0.5 x 0.6 over 0.8 gives 0.8 + 0.3 x (1 - 0.8) = 0.86. A grey below the
    threshold has no highlights and is unchanged."""
    np.testing.assert_allclose(crt(flat(0.8), bloom=0.5, bloom_threshold=0.5), 0.86, atol=1e-6)
    np.testing.assert_allclose(crt(flat(0.4), bloom=0.5, bloom_threshold=0.5), 0.4, atol=1e-6)


def test_bloom_spreads_light_from_a_white_dot_to_its_neighbours_and_never_darkens() -> None:
    dot = np.zeros((41, 41, 3), np.float32)
    dot[20, 20] = 1.0
    out = crt(dot, bloom=1.0, bloom_radius=4.0)
    assert out[20, 23, 0] > 0.003  # light has reached a pixel 3 px from the dot
    assert (out >= dot).all()


def test_the_vignette_darkens_by_the_squared_share_of_the_way_to_a_corner() -> None:
    """On 5 x 5 px the farthest corner is 2 sqrt 2 from the centre pixel (2, 2): the centre keeps its grey, the pixel
    (2, 4), 2 px out, is (2 / (2 sqrt 2))^2 = 0.5 of the way, and a corner is all of it: gains 1, 1 - 0.5 v, 1 - v."""
    out = crt(flat(0.8, 5, 5), vignette=0.6)
    assert out[2, 2, 0] == pytest.approx(0.8, abs=1e-6)
    assert out[2, 4, 0] == pytest.approx(0.8 * (1 - 0.3), abs=1e-6)
    assert out[0, 0, 0] == pytest.approx(0.8 * (1 - 0.6), abs=1e-6)


def test_a_curved_pixel_shows_the_picture_a_little_farther_out_the_farther_it_is_from_the_middle() -> None:
    """On 101 x 101 px with curve 0.2 the pixel (50, 90), 40 px right of the centre (50, 50), is (40 / (50 sqrt 2))^2 =
    0.32 of the way to the corner, so it reads the picture at 1 + 0.2 x 0.32 = 1.064 of that distance, 42.56 px out,
    column 92.56: on a ramp of column / 100, 0.9256."""
    ramp = np.repeat(np.tile(np.arange(101) / 100.0, (101, 1)).astype(np.float32)[..., None], 3, axis=2)
    out = crt(ramp, curve=0.2)
    assert out[50, 90, 1] == pytest.approx(0.9256, abs=1e-4)
    assert out[50, 50, 1] == pytest.approx(0.5, abs=1e-6)  # the middle does not move


@pytest.mark.parametrize("border", ["#ff0000", "#1020ff"])
def test_beyond_the_curved_screen_the_border_colour_shows_and_inside_it_does_not(border: str) -> None:
    """The corner of a curved screen reads beyond the picture; the middle never does."""
    picture = flat(0.5, 41, 61)
    out = crt(picture, curve=0.5, border=border)
    wanted = np.array([int(border[i : i + 2], 16) / 255 for i in (1, 3, 5)], np.float32)
    np.testing.assert_allclose(out[0, 0], wanted, atol=1e-6)
    np.testing.assert_allclose(out[-1, -1], wanted, atol=1e-6)
    np.testing.assert_allclose(out[20, 30], 0.5, atol=1e-6)


def test_the_middle_of_every_side_of_a_curved_screen_is_beyond_it_too() -> None:
    """On 41 x 61 px with curve 0.5 the middle of the top edge, (0, 30), is 20 px from the centre and R = hypot(20, 30)
    = 36.06 px, so it reads at 1 + 0.5 x (20 / 36.06)^2 = 1.15 of that, 3 px above the picture; the middle of the left
    edge, (20, 0), reads 1 + 0.5 x (30 / 36.06)^2 = 1.35 of 30 px, 10 px to its left; likewise the other two."""
    out = crt(flat(0.5, 41, 61), curve=0.5, border="#ffffff")
    for pixel in [(0, 30), (40, 30), (20, 0), (20, 60)]:
        np.testing.assert_allclose(out[pixel], 1.0, atol=1e-6)


def test_a_pixel_that_reads_less_than_a_pixel_beyond_the_picture_is_blended_with_the_border() -> None:
    """With curve 0.02 on 41 x 61 px the pixel (20, 0) reads 30 x (1 + 0.02 x (30 / 36.0555)^2) = 30.415 px from the
    centre, 0.415 px beyond the first column, and shows the picture by 1 - 0.415 = 0.585: 0.585 x 0.5 + 0.415 x 1 =
    0.7077 against a white border. The pixel (0, 30) reads 20.123 px up, 0.123 px beyond: 0.877 x 0.5 + 0.123 =
    0.5615."""
    out = crt(flat(0.5, 41, 61), curve=0.02, border="#ffffff")
    for pixel, value in [((20, 0), 0.70769), ((20, 60), 0.70769), ((0, 30), 0.56154), ((40, 30), 0.56154)]:
        assert out[pixel][0] == pytest.approx(value, abs=1e-4)


def test_the_border_colour_stays_out_of_a_screen_that_is_not_curved() -> None:
    out = crt(flat(0.5, 41, 61), curve=0.0, border="#ff0000")
    np.testing.assert_allclose(out, 0.5, atol=1e-6)


def test_the_border_replaces_whatever_the_other_parts_do_beyond_the_screen() -> None:
    """The mask, the scanlines, the bloom and the vignette all work on the whole glass, but beyond the curved screen
    there is only the border colour."""
    out = registry.apply(
        NAME, flat(0.9, 41, 61), curve=0.6, shadow_mask=1.0, scanlines=1.0, bloom=1.0, vignette=1.0, border="#00ff00"
    )
    np.testing.assert_allclose(out[0, 0], [0, 1, 0], atol=1e-6)


def test_truncate_cuts_the_glow_off_at_that_many_sigmas() -> None:
    """A white dot with a glow of sigma 4: 7 px away the default cut-off (4 sigmas, 16 px) still gives light, a cut-off
    at 1 sigma (4 px) none."""
    dot = np.zeros((41, 41, 3), np.float32)
    dot[20, 20] = 1.0
    assert crt(dot, bloom=1.0, bloom_radius=4.0)[20, 27, 0] > 0.001
    assert crt(dot, bloom=1.0, bloom_radius=4.0, truncate=1.0)[20, 27, 0] == 0.0


@pytest.mark.parametrize(("edge", "far_corner_lit"), [("wrap", True), ("reflect", False), ("nearest", False)])
def test_edge_sets_what_the_glow_sees_beyond_the_picture(edge: str, far_corner_lit: bool) -> None:
    """A white dot in the first pixel: its glow spreads over the opposite corner only when the picture wraps."""
    dot = np.zeros((41, 41, 3), np.float32)
    dot[0, 0] = 1.0
    out = crt(dot, bloom=1.0, bloom_radius=3.0, edge=edge)
    assert (out[-1, -1, 0] > 0.001) == far_corner_lit


@pytest.mark.parametrize(("edge", "shows_far_side"), [("wrap", True), ("reflect", False), ("mirror", False)])
def test_edge_sets_what_the_bend_reads_beyond_the_picture_where_it_is_blended_with_the_border(
    edge: str, shows_far_side: bool
) -> None:
    """On 41 x 61 px with curve 0.02 the pixel (20, 0) reads 0.415 px beyond the first column, which is black, with a
    white last column: the picture wraps to the last column there and shows white, mirrored it repeats black."""
    picture = np.zeros((41, 61, 3), np.float32)
    picture[:, -1] = 1.0
    out = crt(picture, curve=0.02, border="#000000", edge=edge, interpolation="bilinear")
    assert (out[20, 0, 0] > 0.05) == shows_far_side


def test_a_recipe_file_can_set_every_control_of_the_screen(tmp_path: Path) -> None:
    """In a recipe `mask` and `opacity` belong to the step, so no control of the screen may be called by either."""
    controls = {"curve": 0.2, "shadow_mask": 0.5, "scanlines": 0.4, "bloom": 0.2, "vignette": 0.1}
    recipe_file = tmp_path / "tube.toml"
    recipe_file.write_text(
        'name = "tube"\n[[step]]\neffect = "crt"\n' + "".join(f"{key} = {value}\n" for key, value in controls.items()),
        encoding="utf-8",
    )
    assert load_recipe(recipe_file).steps[0].runs[0].params == controls
