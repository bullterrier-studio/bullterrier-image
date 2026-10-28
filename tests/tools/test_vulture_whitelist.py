"""tools/vulture_whitelist.py: the fields of the parameter models of the effects the Rust core implements, and only
those, as a vulture whitelist."""

import importlib.util
from pathlib import Path

from bullterrier_image._native import _core  # noqa: PLC2701  architecture: the effects the Rust core implements
from bullterrier_image.core import registry
from bullterrier_image.core.params import EffectParams, param

ROOT = Path(__file__).resolve().parents[2]


def _load_tool():
    spec = importlib.util.spec_from_file_location("vulture_whitelist", ROOT / "tools" / "vulture_whitelist.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


vulture_whitelist = _load_tool()


class ProbeParams(EffectParams):
    """Two fields, as an effect's model declares them."""

    zz_spread: float = param(1.0, unit="ratio", min=0, max=2, doc="A spread.")
    zz_lift: float = param(0.0, unit="fraction", min=0, max=1, doc="A lift.")


def test_the_whitelist_names_every_field_of_each_model_given():
    source = vulture_whitelist.whitelist([ProbeParams])
    compile(source, "whitelist.py", "exec")  # vulture reads it as Python
    assert {line.split()[0] for line in source.splitlines()} == {"_.zz_spread", "_.zz_lift"}


def test_the_written_whitelist_names_the_fields_of_the_rust_effects_and_of_no_other(tmp_path):
    target = tmp_path / "whitelist.py"
    assert vulture_whitelist.main([str(target)]) == 0
    written = {line.split()[0].removeprefix("_.") for line in target.read_text(encoding="utf-8").splitlines()}
    assert written == {field for name in _core.effects() for field in registry.parameters(name).model_fields}
