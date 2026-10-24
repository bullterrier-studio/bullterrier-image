"""Presets are an effect's named parameter sets, read from TOML: each is checked by the effect's own parameter model,
and one without a description or with a bad parameter is a `RecipeError` naming it. Read from a throwaway folder here;
the shipped presets are checked in `tests/effects/test_contract.py`."""

import pytest

from bullterrier_image.core import registry
from bullterrier_image.core.presets import read
from bullterrier_image.errors import RecipeError

MODEL = registry.parameters("gradient_map")


def _folder(tmp_path, text):
    (tmp_path / "gradient_map.toml").write_text(text, encoding="utf-8")
    return tmp_path


def test_presets_are_read_in_file_order_with_their_description_and_parameters(tmp_path):
    folder = _folder(
        tmp_path,
        '[warm]\ndescription = "Brown."\nstops = ["#000", "#fa0"]\n\n'
        '[cold]\ndescription = "Blue."\nshadow_clip = 0.2\n',
    )
    found = read("gradient_map", MODEL, folder)
    assert list(found) == ["warm", "cold"]
    assert found["warm"].description == "Brown."
    assert found["warm"].params == {"stops": ["#000", "#fa0"]}  # the description is not a parameter
    assert found["cold"].params == {"shadow_clip": 0.2}


def test_an_effect_without_a_file_has_no_presets(tmp_path):
    assert read("gradient_map", MODEL, tmp_path) == {}


BAD = {
    "no description": ("[warm]\nshadow_clip = 0.1\n", "description"),
    "an empty description": ('[warm]\ndescription = "  "\n', "description"),
    "a description that is no text": ("[warm]\ndescription = 3\n", "description"),
    "a misspelt parameter": ('[warm]\ndescription = "x"\nshadow_clp = 0.1\n', "shadow_clp"),
    "a value out of range": ('[warm]\ndescription = "x"\nshadow_clip = 0.9\n', "shadow_clip.*0.9"),
    "the subject matte, which a recipe supplies": ('[warm]\ndescription = "x"\nmatte = 1\n', "matte"),
}


@pytest.mark.parametrize(("text", "complaint"), BAD.values(), ids=BAD.keys())
def test_a_preset_that_is_not_complete_and_valid_is_refused_naming_it_and_its_effect(tmp_path, text, complaint):
    with pytest.raises(RecipeError, match=rf"preset 'warm' of effect 'gradient_map'.*{complaint}"):
        read("gradient_map", MODEL, _folder(tmp_path, text))
