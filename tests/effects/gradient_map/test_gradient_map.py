"""Behaviour of the `gradient_map` effect."""

import numpy as np
import pytest

from photo_fx.core import registry


def test_gradient_map_end_points_use_first_and_last_stop() -> None:
    ramp = np.linspace(0, 1, 256, dtype=np.float32)[None, :, None].repeat(3, axis=2).repeat(4, axis=0)
    out = registry.apply("gradient_map", ramp, stops=["#ff0000", "#00ff00", "#0000ff"])
    np.testing.assert_allclose(out[0, 0], [1, 0, 0], atol=0.02)
    np.testing.assert_allclose(out[0, -1], [0, 0, 1], atol=0.02)
    np.testing.assert_allclose(out[0, 128], [0, 1, 0], atol=0.05)


def test_gradient_map_needs_two_stops(photo: np.ndarray) -> None:
    with pytest.raises(ValueError, match="stops"):
        registry.apply("gradient_map", photo, stops=["#000000"])


def test_gradient_map_clip_fractions_set_how_much_is_pushed_to_the_end_stops() -> None:
    ramp = np.linspace(0, 1, 1000, dtype=np.float32)[None, :, None].repeat(3, axis=2)  # every brightness once
    black_white = ["#000000", "#ffffff"]
    unclipped = registry.apply("gradient_map", ramp, stops=black_white, shadow_clip=0.0, highlight_clip=0.0)
    assert (unclipped[0, :, 0] == 0.0).sum() == 1  # only the darkest pixel is the first stop
    clipped = registry.apply("gradient_map", ramp, stops=black_white, shadow_clip=0.1, highlight_clip=0.2)
    assert abs((clipped[0, :, 0] == 0.0).sum() - 100) <= 1  # the darkest 10% are the first stop
    assert abs((clipped[0, :, 0] == 1.0).sum() - 200) <= 1  # the brightest 20% are the last stop


@pytest.mark.parametrize(("method", "expected"), [("linear", 0.5), ("lower", 1.0), ("higher", 0.0)])
def test_gradient_map_quantile_method_decides_where_the_stretch_bounds_fall(method: str, expected: float) -> None:
    """Five greys 0, .25, .5, .75, 1, clipping 30% at each end: the 30% and 70% quantiles are 0.3 and 0.7 by linear
    interpolation, 0.25 and 0.5 rounding down to a sample ('lower'), 0.5 and 0.75 rounding up ('higher'). The middle
    grey 0.5 then sits at (0.5 - lo) / (hi - lo) of a black-to-white gradient."""
    greys = np.repeat(np.array([0, 0.25, 0.5, 0.75, 1], np.float32)[None, :, None], 3, axis=2)
    out = registry.apply(
        "gradient_map",
        greys,
        stops=["#000000", "#ffffff"],
        shadow_clip=0.3,
        highlight_clip=0.3,
        quantile_method=method,
    )
    assert out[0, 2, 0] == pytest.approx(expected, abs=1e-6)
