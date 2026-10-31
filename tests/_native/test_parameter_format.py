"""Conformance to the ecosystem's parameter format, version 1 (`docs/parameter-format/parameters.md`).

The schema and the fixtures are the in-repo copy of that format. The fixtures prove the schema accepts what the format
allows and refuses what it forbids; the schema then judges every parameter of every effect the Rust core declares. The
effects are found by asking the core, so an effect ported later joins the test without being listed.
"""

import json
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator

from bullterrier_image._native import _core  # noqa: PLC2701  architecture: only the core can say what it declares

FORMAT = Path(__file__).parents[2] / "docs" / "parameter-format"
SCHEMA = Draft202012Validator(json.loads((FORMAT / "schemas" / "parameter-1.schema.json").read_text(encoding="utf-8")))


def fixtures(name: str) -> list[object]:
    """The `{label, parameter}` entries of a fixture file, as test cases named by their label."""
    entries = json.loads((FORMAT / "fixtures" / "parameters" / name).read_text(encoding="utf-8"))
    return [pytest.param(entry["parameter"], id=entry["label"]) for entry in entries]


VALID = fixtures("valid.json")
INVALID = fixtures("invalid.json")


def test_there_are_fixtures_of_both_kinds():
    assert VALID
    assert INVALID


@pytest.mark.parametrize("parameter", VALID)
def test_the_schema_accepts_a_valid_fixture(parameter):
    assert [error.message for error in SCHEMA.iter_errors(parameter)] == []


@pytest.mark.parametrize("parameter", INVALID)
def test_the_schema_refuses_an_invalid_fixture(parameter):
    assert list(SCHEMA.iter_errors(parameter))


def declared_fields() -> dict[str, list[dict[str, object]]]:
    """The fields of every effect whose parameters the Rust core declares, by effect."""
    fields = {}
    for effect in _core.effects():
        try:
            fields[effect] = _core.params(effect)["fields"]
        except ValueError:  # an effect that describes its own parameters (a Python function registered with the core)
            continue
    return fields


DECLARED = declared_fields()
EMITTED = [
    pytest.param(field, id=f"{effect}.{field['name']}") for effect, fields in DECLARED.items() for field in fields
]


def test_the_walk_reaches_the_effects_declared_in_rust():
    assert {"grade", "pattern", "pixel_grid"} <= set(DECLARED)


@pytest.mark.parametrize("parameter", EMITTED)
def test_the_schema_accepts_a_parameter_an_effect_declares(parameter):
    assert [error.message for error in SCHEMA.iter_errors(parameter)] == []
