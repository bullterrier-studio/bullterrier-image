import numpy as np
import pytest

from photo_fx import effects
from photo_fx.effects import _registry
from photo_fx.effects._registry import unregister


@pytest.fixture
def fresh_registry(monkeypatch):
    """An empty effect registry for one test, so it can build exactly the effects and categories it needs, whatever
    real effects exist. The real registry comes back afterwards, untouched."""
    monkeypatch.setattr(_registry, "_EFFECTS", {})
    monkeypatch.setattr(_registry, "_CATEGORY", {})


@pytest.fixture
def register_probe():
    """`register_probe(name, category)` registers a do-nothing effect with that name. Use it inside `fresh_registry`
    or `throwaway_effects`, which remove it again."""

    def register(name: str, category: str = "utilities") -> None:
        def probe(rgb, amount: float = 0.5):
            return rgb

        probe.__name__ = probe.__qualname__ = name
        probe.__doc__ = f"The {name} probe."
        _registry.effect(category=category)(probe)

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
