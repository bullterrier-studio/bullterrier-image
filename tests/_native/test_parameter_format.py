"""Conformance to the ecosystem's parameter format, version 1 (`docs/parameter-format/parameters.md`).

The schema and the fixtures are the in-repo copy of that format. The fixtures prove the schema accepts what the format
allows and refuses what it forbids; the schema then judges what this package emits.
"""

import json
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator

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
