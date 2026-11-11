"""An effect registered with a parameter table (the parameter format as data) is checked, scaled and described by the
core as an effect declared in Rust is: the function receives the complete, checked values, the pixel ones scaled to the
picture. The table's own rules are `crates/core/tests/it/table_json.rs`; this is the boundary that carries it."""

from contextlib import contextmanager

import numpy as np
import pytest

from bullterrier_image._native import _core  # noqa: PLC2701  architecture: the boundary a plugin table crosses

PHOTO = np.full((48, 96, 3), 0.5, np.float32)  # 96 px wide: 96 / 480 of the design size, a ratio of 0.2


def number(name, default, unit, low, high, *, cyclic=False):
    return {
        "name": name, "kind": "float", "meaning": "A number.", "default": default,
        "unit": unit, "min": low, "max": high, "cyclic": cyclic,
    }  # fmt: skip


TABLE = {
    "doc": "A probe.",
    "fields": [
        number("radius", 10.0, "px", 0, 100),
        number("angle", 0.0, "degrees", 0, 360, cyclic=True),
        {"name": "mode", "kind": "choice", "meaning": "How.", "default": "soft", "unit": None, "min": None, "max": None,
         "cyclic": False, "choices": ["soft", "hard"]},
    ],
}  # fmt: skip


@contextmanager
def registered(name, function, table):
    _core.register(name, "utilities", 1.0, function, table=table)
    try:
        yield
    finally:
        _core.unregister(name)


def recipe(name, **params):
    return {"steps": [{"effect": name, "params": params}], "size": None}


def test_the_core_describes_and_schemas_a_registered_effect_by_its_table():
    with registered("probe_table_a", lambda rgb, params: rgb, TABLE):
        described = _core.params("probe_table_a")
        schema = _core.schema("probe_table_a")
    assert described["doc"] == "A probe."
    assert [(field["name"], field["kind"], field["default"]) for field in described["fields"]] == [
        ("radius", "float", 10.0),
        ("angle", "float", 0.0),
        ("mode", "choice", "soft"),
    ]
    assert schema["properties"]["radius"]["maximum"] == 100
    assert schema["properties"]["mode"]["enum"] == ["soft", "hard"]


def test_the_function_receives_complete_checked_values_with_the_pixel_ones_scaled_to_the_picture():
    seen = []

    def probe(rgb, params):
        seen.append(params)
        return rgb

    with registered("probe_table_b", probe, TABLE):
        _core.render(PHOTO, recipe("probe_table_b", radius=20, angle=450))
    assert len(seen) == 1
    assert seen[0]["radius"] == pytest.approx(20 * 0.2)  # design px scaled to the 96 px picture
    assert seen[0]["angle"] == pytest.approx(90.0)  # a turn wraps
    assert seen[0]["mode"] == "soft"  # the default, completed


@pytest.mark.parametrize(
    ("params", "said"),
    [({"radius": 101}, "radius"), ({"mode": "rough"}, "mode"), ({"colour": 1}, "colour")],
    ids=["above its maximum", "not a choice", "unknown name"],
)
def test_a_step_that_breaks_the_table_is_refused_by_the_core_before_the_function_runs(params, said):
    called = []

    def probe(rgb, given):
        called.append(given)
        return rgb

    with registered("probe_table_c", probe, TABLE), pytest.raises(ValueError, match=said):
        _core.check_recipe(recipe("probe_table_c", **params))
    assert called == []


def test_a_table_that_breaks_the_format_registers_nothing_and_names_the_parameter():
    bad = {"doc": "", "fields": [{**TABLE["fields"][0], "min": 9, "max": 1}]}
    with pytest.raises(ValueError, match="radius"):
        _core.register("probe_table_d", "utilities", 1.0, lambda rgb, params: rgb, table=bad)
    assert "probe_table_d" not in _core.effects()
