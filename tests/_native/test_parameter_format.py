"""Conformance to the ecosystem's parameter format, version 2 (`docs/parameter-format/parameters.md`).

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
SCHEMA = Draft202012Validator(json.loads((FORMAT / "schemas" / "parameter-2.schema.json").read_text(encoding="utf-8")))


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


def test_the_schema_accepts_every_parameter_every_effect_the_core_declares():
    assert {"grade", "pattern", "pixel_grid"} <= set(DECLARED)  # the walk reaches the effects declared in Rust
    refused = {
        f"{effect}.{field['name']}": [error.message for error in SCHEMA.iter_errors(field)]
        for effect, fields in DECLARED.items()
        for field in fields
        if not SCHEMA.is_valid(field)
    }
    assert refused == {}


@pytest.mark.parametrize(
    "extra",
    [
        pytest.param({"min_length": 2}, id="a floor only: no ceiling is written"),
        pytest.param({"min_length": 2, "max_length": 4}, id="both ends"),
    ],
)
def test_the_schema_accepts_a_list_of_colours_as_the_core_writes_it(extra):
    """The lengths as `Field` serialises them (crates/core/src/params.rs; the same JSON is asserted in
    crates/core/src/params/derived.rs): a floor alone, as `gradient_map` writes it, and a floor with a ceiling, which no
    effect declares."""
    field = {
        "name": "stops",
        "kind": "colors",
        "meaning": "The colours of the ramp in order.",
        "default": ["#12002b", "#ff2bd6"],
        "unit": None,
        "min": None,
        "max": None,
        "cyclic": False,
        "choices": [],
        "drivable": False,
        "advanced": False,
    } | extra
    assert [error.message for error in SCHEMA.iter_errors(field)] == []


@pytest.mark.parametrize(
    "extra",
    [
        pytest.param({"kind": "field", "name": "field", "default": "depth"}, id="a field: a name, never null"),
        pytest.param({"kind": "color", "name": "color", "default": None, "nullable": True}, id="an optional colour"),
    ],
)
def test_the_schema_accepts_a_field_and_an_optional_colour_as_the_core_writes_them(extra):
    """The fields as `Field` serialises them (crates/core/src/params.rs; the same JSON is asserted in
    crates/core/src/params/derived.rs): `nullable` is written only where it is true."""
    field = {
        "meaning": "A selection or a colour.",
        "unit": None,
        "min": None,
        "max": None,
        "cyclic": False,
        "choices": [],
        "drivable": False,
        "advanced": False,
    } | extra
    assert [error.message for error in SCHEMA.iter_errors(field)] == []


def test_the_schema_accepts_a_number_limited_by_another_as_the_core_writes_it():
    """`at_most` is a key image adds (crates/core/src/params.rs); the format says a reader ignores a key it does not
    know, so the schema takes the parameter as it takes any number."""
    field = {
        "name": "band_min_height",
        "kind": "float",
        "meaning": "The lowest a band can be.",
        "default": 10,
        "unit": "px",
        "min": 0,
        "max": 2000,
        "cyclic": False,
        "choices": [],
        "at_most": "band_max_height",
        "drivable": False,
        "advanced": False,
    }
    assert [error.message for error in SCHEMA.iter_errors(field)] == []
