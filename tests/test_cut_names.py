"""The cut-name audit: every name that left the effect list stays findable through the public lookup.

The table is test data of the audit (old name -> the effect that took it over), not a catalogue the library reads:
nothing in `src` knows the old names. Each name is asked for the way a person does, as `fx.<name>`, and the error
must point at the survivor: as its preset, as one of its parameters, or as a word in its summary.
"""

import pytest

from bullterrier_image import fx
from bullterrier_image.loader import load_recipe

TOOK_OVER = {
    "glow": "bloom",
    "soft_focus": "bloom",
    "halation": "bloom",
    "fade": "levels",
    "invert": "levels",
    "clarity": "sharpen",
    "film_grain": "grain",
    "noise": "grain",
    "byte_corrupt": "jpeg_crush",
    "prism": "chromatic_aberration",
    "black_and_white": "channel_mixer",
    "white_balance": "grade",
    "mirror": "kaleidoscope",
    "shear": "perspective",
    "anaglyph": "rgb_split",
    "plasma": "noise_field",
    "fisheye": "lens_distortion",
    "bump_light": "relight",
    "emboss": "relight",
    "layered_cutouts": "drop_shadow",
    "long_shadow": "drop_shadow",
    "sparkle": "starburst",
    "smoke": "fog",
    "scatter": "displace",
    "embers": "particles",
    "rain": "particles",
    "snow": "particles",
    "floating_dust": "particles",
    "starfield": "particles",
    "split_tone": "color_balance",
    "solarize": "curves",
    "hue_shift": "selective_color",
    "posterize": "dither",
    "threshold": "dither",
    "palette_quantize": "dither",
    "fill": "gradient",
    "scan_roll": "tile",
    "signal_noise": "vhs",
    "shadows_highlights": "tone_map",
    "evaporate": "disperse",
    "overlay": "blend",
    "glitch": "slice_shift",
    "vibrance": "grade",
    "temperature": "grade",
    "depth_of_field": "focus_blur",
}


@pytest.mark.parametrize(("old", "survivor"), TOOK_OVER.items(), ids=TOOK_OVER)
def test_a_cut_name_is_no_effect_and_the_error_points_at_the_survivor(old, survivor):
    with pytest.raises(AttributeError) as raised:
        getattr(fx, old)
    pointer = str(raised.value).split("available:")[0]  # before the list of every effect, which names the survivor too
    assert repr(old) in pointer
    assert survivor in pointer


def test_a_name_that_was_never_an_effect_points_nowhere():
    with pytest.raises(AttributeError) as raised:
        fx.nonsense_xyz  # noqa: B018  (the lookup itself is what must raise)
    assert "is a " not in str(raised.value).split("available:")[0]
    assert "see " not in str(raised.value).split("available:")[0]


def test_the_anaglyph_look_is_loadable_by_name_and_is_named_by_the_error():
    recipe = load_recipe("anaglyph")
    assert recipe.name == "anaglyph"
    assert len(recipe.steps) == 2
    with pytest.raises(AttributeError, match=r'load_recipe\("anaglyph"\)'):
        fx.anaglyph  # noqa: B018  (the lookup itself is what must raise)
