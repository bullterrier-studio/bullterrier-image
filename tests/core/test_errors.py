"""photo-fx raises its own exceptions: one base, `PhotoFxError`, and subclasses that also inherit the builtin a caller
would already catch, so a mistake raises the same type whichever way it was made (a recipe file, `fx`, a direct
call)."""

import numpy as np
import pytest

from photo_fx import (
    BackendUnavailableError,
    ImageReadError,
    PhotoFxError,
    RecipeError,
    SubjectNotFoundError,
    UnknownEffectError,
    fx,
)
from photo_fx.core import registry
from photo_fx.core.loader import load_recipe
from photo_fx.core.steps import Step

GREY = np.full((4, 4, 3), 0.25, dtype=np.float32)


def test_a_misspelt_parameter_is_a_recipe_error_from_fx_a_direct_call_and_a_recipe_file(tmp_path):
    path = tmp_path / "typo.toml"
    path.write_text("[[step]]\neffect = 'grade'\ntypo = 1\n", encoding="utf-8")
    with pytest.raises(RecipeError, match="typo"):
        fx.grade(typo=1)
    with pytest.raises(RecipeError, match="typo"):
        registry.get("grade")(GREY, typo=1)
    with pytest.raises(RecipeError, match="typo"):
        load_recipe(path)


def test_an_unknown_effect_is_an_unknown_effect_error_with_a_clean_message_wherever_it_is_named(tmp_path):
    path = tmp_path / "unknown.toml"
    path.write_text("[[step]]\neffect = 'no_such_effect'\n", encoding="utf-8")
    for attempt in (lambda: registry.get("no_such_effect"), lambda: Step("no_such_effect"), lambda: load_recipe(path)):
        with pytest.raises(UnknownEffectError) as raised:
            attempt()
        message = str(raised.value)
        assert "no_such_effect" in message
        assert "available" in message
        assert "grade" in message  # the known effects are listed
        assert message[0] not in "'\""  # not a KeyError's quoted repr
    assert str(raised.value).startswith("unknown.toml")


def test_recipe_errors_are_still_caught_by_the_builtin_handlers_callers_already_have():
    with pytest.raises(ValueError, match="opacity"):
        Step("grade", opacity=1.5)
    with pytest.raises(PhotoFxError, match="opacity"):
        Step("grade", opacity=1.5)
    with pytest.raises(KeyError):
        Step("no_such_effect")


@pytest.mark.parametrize(
    ("error", "builtin"),
    [
        (RecipeError, ValueError),
        (UnknownEffectError, KeyError),
        (UnknownEffectError, RecipeError),
        (SubjectNotFoundError, RuntimeError),
        (BackendUnavailableError, NotImplementedError),
        (ImageReadError, OSError),
    ],
)
def test_every_public_error_is_caught_as_a_photo_fx_error_and_as_its_builtin(error, builtin):
    with pytest.raises(PhotoFxError, match="what went wrong"):
        raise error("what went wrong")
    with pytest.raises(builtin, match="what went wrong"):
        raise error("what went wrong")
