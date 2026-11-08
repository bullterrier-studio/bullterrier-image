"""The `slice_shift` presets, resolved by `fx`. The kernel is tested in `crates/core/tests/it/slice_shift.rs`."""

from bullterrier_image import fx
from bullterrier_image.recipe import Recipe, render


def test_the_glitch_preset_is_a_few_bands_not_a_tear(photo) -> None:
    out = render(photo, Recipe([fx.slice_shift(preset="glitch")], name="t", size=None))
    changed_rows = (out != photo).any(axis=(1, 2)).sum()
    assert 0 < changed_rows <= 0.5 * photo.shape[0]
