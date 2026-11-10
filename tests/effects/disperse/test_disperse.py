"""The looks (presets) of `disperse`: their data, and that each does what its description says. The effect itself is
tested in Rust (`crates/core/tests/it/disperse.rs`)."""

import numpy as np
import pytest

from bullterrier_image.core import registry


def _lit(height=120, width=160):
    return np.ones((height, width, 3), np.float32)


def _disperse(img: np.ndarray, **settings: object) -> np.ndarray:
    """`disperse` on `img`; unless `field` is given, the figure is the picture's brightness: what is lit on black."""
    return registry.apply("disperse", img, **{"field": img.mean(axis=2), **settings})


_LOOKS = ["evaporate", "halo", "windblown", "disintegrate"]


def _look(name: str, img: np.ndarray) -> np.ndarray:
    return _disperse(img, **registry.presets("disperse")[name].params)


def _brightness_centre_x(img: np.ndarray) -> float:
    columns = img.sum(axis=(0, 2))
    return float((columns * np.arange(img.shape[1])).sum() / columns.sum())


@pytest.mark.parametrize("name", _LOOKS)
def test_disperse_has_a_preset_for_each_look_it_folded_in_and_describes_it(name):
    assert registry.presets("disperse")[name].description
    assert "evaporate" in registry.summary("disperse")  # the plain-words summary names the looks
    assert name.replace("windblown", "wind-blown") in registry.summary("disperse")


@pytest.mark.parametrize("name", _LOOKS)
def test_a_look_sheds_the_top_before_the_bottom_and_leaves_something_standing(name):
    out = _look(name, _lit(height=240, width=240))
    assert out[:80].mean() < out[160:].mean()
    assert 0.1 < out.mean() < 0.9


def test_windblown_carries_the_figure_to_the_right_and_the_halo_does_not():
    block = np.zeros((240, 240, 3), np.float32)
    block[80:240, 70:170] = 1.0
    start = _brightness_centre_x(block)
    assert _brightness_centre_x(_look("windblown", block)) > start + 5
    assert abs(_brightness_centre_x(_look("halo", block)) - start) < 5


def test_the_halo_sheds_the_sides_of_the_figure_and_the_default_look_keeps_them():
    block = np.zeros((240, 240, 3), np.float32)
    block[80:240, 70:170] = 1.0
    edge, middle = (slice(120, 240), slice(70, 90)), (slice(120, 240), slice(110, 130))  # below the head
    halo, kept = _look("halo", block), _look("evaporate", block)
    assert halo[edge].mean() < 0.7 * halo[middle].mean()
    assert kept[edge].mean() > 0.85 * kept[middle].mean()
