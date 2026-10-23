"""The numbers of `timing`: easing curves and the frame count, checked against hand-computed or textbook values (the
cubic-bezier definition), never against the module's own formulas."""

import pytest

from photo_fx.core.timing import Animated, Animation

# (name, value of the ease at t = 0.25, 0.5, 0.75), worked out by hand from the definitions
SIMPLE_EASES = [
    ("linear", 0.25, 0.5, 0.75),
    ("in", 0.0625, 0.25, 0.5625),  # t^2
    ("out", 0.4375, 0.75, 0.9375),  # 1 - (1 - t)^2
    ("in-out", 0.15625, 0.5, 0.84375),  # t^2 (3 - 2t), smoothstep
]

# cubic-bezier control points (x1, y1, x2, y2) of the named presets, as written in the motion-design skill's tables
PRESETS = {
    "gentle": (0.4, 0.0, 0.2, 1.0),
    "standard": (0.2, 0.0, 0.0, 1.0),
    "accelerate": (0.3, 0.0, 1.0, 1.0),
    "decelerate": (0.0, 0.0, 0.0, 1.0),
    "emphasized": (0.05, 0.7, 0.1, 1.0),
    "sine": (0.37, 0.0, 0.63, 1.0),
}


@pytest.mark.parametrize(("name", "at_quarter", "at_half", "at_three_quarters"), SIMPLE_EASES)
def test_simple_eases_have_their_textbook_values(name, at_quarter, at_half, at_three_quarters):
    curve = Animated(2.0, 6.0, name)  # a start and an end that are not 0 and 1: a swapped argument would show
    for t, expected in [(0.25, at_quarter), (0.5, at_half), (0.75, at_three_quarters)]:
        assert curve.at(t) == pytest.approx(2.0 + 4.0 * expected, abs=1e-12)
    assert curve.at(0.0) == 2.0
    assert curve.at(1.0) == 6.0


def bezier_point(x1: float, y1: float, x2: float, y2: float, s: float) -> tuple[float, float]:
    """The point of the cubic bezier (0,0), (x1,y1), (x2,y2), (1,1) at parameter s, by Bernstein's formula."""
    b1, b2, b3 = 3 * s * (1 - s) ** 2, 3 * s**2 * (1 - s), s**3
    return b1 * x1 + b2 * x2 + b3, b1 * y1 + b2 * y2 + b3


CUSTOM = (0.17, 0.67, 0.83, 0.67)  # not a preset


@pytest.mark.parametrize(
    ("ease", "control"), [*PRESETS.items(), (CUSTOM, CUSTOM)], ids=[*PRESETS, "four numbers of one's own"]
)
def test_named_and_custom_curves_follow_their_cubic_bezier_to_high_precision(
    ease: str | tuple[float, float, float, float], control: tuple[float, float, float, float]
) -> None:
    """The curve is the set of points (x(s), y(s)), so the ease at time x(s) must be y(s). Walking s forward needs no
    inversion, so this is independent of the module's solver, and a coarse solver or a wrong constant shows."""
    for step in range(1, 20):
        x, y = bezier_point(*control, step / 20)
        assert Animated(0.0, 1.0, ease).at(x) == pytest.approx(y, abs=1e-9)


@pytest.mark.parametrize(
    ("seconds", "fps", "frames"),
    [
        (1.27, 10, 13),  # 12.7 frames: rounded to the nearest, not cut down
        (0.29, 10, 3),  # 2.9
        (0.04, 10, 2),  # a clip always has at least two frames
        (5, 30, 150),
    ],
)
def test_frame_count_is_seconds_times_fps_to_the_nearest_frame(seconds, fps, frames):
    assert len(Animation(seconds, fps).frames()) == frames


def test_pingpong_adds_the_way_back_without_repeating_the_turning_frames():
    once = Animation(1.27, 10).frames()
    assert len(Animation(1.27, 10, loop="pingpong").frames()) == 2 * len(once) - 2  # 24
