"""The cut-name audit: every effect folded into another stays findable as a preset of the effect that took it over.

The table is test data of the audit (old effect name -> the survivor holding it as a preset), not a catalogue the
library reads: nothing in `src` knows the old names. A name that became a plain parameter (vibrance, temperature) or
a multi-step look is not here.
"""

import pytest

from bullterrier_image.core import registry

FOLDED_INTO = {
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
}


@pytest.mark.parametrize(("old", "survivor"), FOLDED_INTO.items(), ids=FOLDED_INTO)
def test_a_cut_effect_is_a_preset_of_its_survivor_and_no_longer_an_effect(old, survivor):
    assert old in registry.presets(survivor)
    assert old not in registry.all_effects()


def test_a_name_that_was_never_an_effect_is_no_preset_of_any_effect():
    assert not [name for name in registry.all_effects() if "no_such_look" in registry.presets(name)]
