"""The shipped presets of the `color_balance` effect; its behaviour is tested in Rust."""

from bullterrier_image.core import registry


def test_the_names_of_the_cut_effects_and_the_teal_and_orange_grade_are_presets() -> None:
    assert {"split_tone", "color_balance", "teal_orange"} <= set(registry.presets("color_balance"))
