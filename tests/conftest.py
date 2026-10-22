from collections.abc import Iterator
from contextlib import contextmanager

import numpy as np
import pytest

from photo_fx import effects
from photo_fx.core.registry import effect, unregister


@contextmanager
def empty_registry() -> Iterator[None]:
    """Inside the block the effect registry is empty; afterwards, even if the block raised, whatever was registered in
    it is removed and the real effects are back, in their original order. Uses only the registry's public interface:
    `unregister`, and registering the same function again, which the registry allows."""
    real = [(name, fn, effects.category(name)) for name, fn in effects.all_effects().items()]
    for name, _, _ in real:
        unregister(name)
    try:
        yield
    finally:
        for name in effects.all_effects():
            unregister(name)
        for _, fn, category in real:
            effect(category=category)(fn)


@pytest.fixture
def fresh_registry():
    """An empty effect registry for one test, so it can build exactly the effects and categories it needs, whatever
    real effects exist (see `empty_registry`)."""
    with empty_registry():
        yield


@pytest.fixture
def registry_emptied():
    """The `empty_registry` context manager itself, for the tests of that helper."""
    return empty_registry


@pytest.fixture
def register_probe():
    """`register_probe(name, category)` registers a do-nothing effect with that name. Use it inside `fresh_registry`
    or `throwaway_effects`, which remove it again."""

    def register(name: str, category: str = "utilities") -> None:
        def probe(rgb, amount: float = 0.5):
            return rgb

        probe.__name__ = probe.__qualname__ = name
        probe.__doc__ = f"The {name} probe."
        effect(category=category)(probe)

    return register


@pytest.fixture
def throwaway_effects():
    """Lets a test register effects of its own: whatever it registered is removed again afterwards, even if the test
    fails, so no other test ever sees it."""
    before = set(effects.all_effects())
    yield
    for name in set(effects.all_effects()) - before:
        unregister(name)


@pytest.fixture
def photo() -> np.ndarray:
    """A small, deterministic, non-trivial picture: colour gradients plus noise."""
    rng = np.random.default_rng(1)
    y, x = np.mgrid[0:96, 0:128].astype(np.float32)
    img = np.stack([x / 127, y / 95, (x + y) / 222], axis=-1) * 0.8 + rng.random((96, 128, 3), dtype=np.float32) * 0.2
    return img.astype(np.float32)


@pytest.fixture
def matte() -> np.ndarray:
    """A disc in the middle of the 96x128 picture: 1 inside, 0 outside, soft edge."""
    y, x = np.mgrid[0:96, 0:128].astype(np.float32)
    return np.clip(30 - np.hypot(x - 64, y - 48), 0, 1).astype(np.float32)
