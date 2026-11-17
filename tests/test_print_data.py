"""The data beside the picture through the facade: the calls take what `render` takes and agree with its picture. What
the data is, is tested in Rust (`crates/core/tests/it/print_data.rs`)."""

import numpy as np
import pytest

from bullterrier_image import Characters, Recipe, RecipeError, characters, fx, ink_planes, palette_map, render


def test_the_palette_map_painted_with_its_colours_is_the_rendered_picture(photo):
    look = Recipe([fx.dither(palette="custom", custom=["#101040", "#e63333", "#f2e680"], cell=2)])
    labels, colors = palette_map(photo, look)
    assert labels.dtype == np.int32
    assert labels.shape == photo.shape[:2]
    assert len(colors) == 3
    assert all(len(color) == 7 and color[0] == "#" for color in colors)
    table = np.array([[int(color[i : i + 2], 16) for i in (1, 3, 5)] for color in colors], np.float32) / 255
    # the colours are 8-bit text, so the repainting is the render to within one level of 255
    assert np.abs(table[labels] - render(photo, look)).max() <= 1 / 255 + 1e-6


def test_rgb_ink_planes_are_the_rendered_channels_and_a_step_is_a_recipe(photo):
    step = fx.halftone(separation="rgb", cell=6)
    planes = ink_planes(photo, step)
    picture = render(photo, step)
    assert len(planes) == 3
    assert all(plane.dtype == np.float32 and plane.shape == photo.shape[:2] for plane in planes)
    for channel, plane in enumerate(planes):
        np.testing.assert_array_equal(plane, picture[..., channel])


def test_the_characters_are_text_with_colours_and_planes_follow_size(photo):
    look = Recipe([fx.ascii(cell=8)])
    text = characters(photo, look, size=64)
    assert isinstance(text, Characters)
    lines = text.text.split("\n")
    assert len(lines) == text.rows
    assert all(len(line) == text.columns for line in lines)
    assert len(text.colors) == text.rows
    assert all(len(line) == text.columns for line in text.colors)
    assert text.ansi().startswith("\x1b[")
    assert text.html().startswith("<pre")
    # the look is the same at any size, so the grid is the same; the size shows in the planes of a picture
    assert text.columns == characters(photo, look).columns
    assert ink_planes(photo, fx.halftone(separation="rgb"), size=64)[0].shape == (48, 64)


def test_the_wrong_kind_of_data_is_a_recipe_error_naming_the_effect(photo):
    with pytest.raises(RecipeError, match="ascii"):
        ink_planes(photo, fx.ascii())
    with pytest.raises(RecipeError, match="grade"):
        characters(photo, fx.grade())
    with pytest.raises(RecipeError, match="opacity"):
        characters(photo, Recipe([fx.ascii(opacity=0.5)]))


def test_a_line_break_among_the_chars_is_refused_naming_chars(photo):
    with pytest.raises(RecipeError, match="chars"):
        characters(photo, fx.ascii(chars=" .\n#"))
