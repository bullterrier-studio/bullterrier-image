"""Tests of `effects/glitch.py`."""

import numpy as np

from photo_fx import effects


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


def test_glitch_ascii_photo_keeps_strip_detail(photo):
    flat = effects.apply("glitch", photo, seed=3, ascii_share=1.0, ascii_cell=6, ascii_photo=0.0)
    over = effects.apply("glitch", photo, seed=3, ascii_share=1.0, ascii_cell=6, ascii_photo=1.0)
    plain = effects.apply("glitch", photo, seed=3)
    assert np.abs(over - plain).mean() < np.abs(flat - plain).mean()


def test_the_glitch_effects_are_in_their_categories():
    """glitch: where the effect catalogue lists them."""
    expected = {
        "glitch": "glitch",
    }
    assert {name: effects.category(name) for name in expected} == expected
