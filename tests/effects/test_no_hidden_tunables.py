"""No hidden tunables (CLAUDE.md rule 5): every number that shapes an effect's result is a parameter.

An architecture test: its subject is the source of the effect modules, so it reads them with `ast`. A numeric literal
in an effect module fails unless it is a parameter's declaration (inside `param(...)`: default, min, max), a trivial
value (0, 1, 2, 0.5, and 255 for 8-bit scaling; a minus sign does not matter), or carries a `# constant: <citation or
reason>` comment on its own line or the line before (a standard, such as Rec. 709 weights, or a pure guard, such as an
epsilon). Only effect packages (`effects/<name>/effect.py`) are scanned: TRANSITIONAL, the legacy loose modules join
when they are migrated.
"""

import ast
import io
import textwrap
import tokenize
from pathlib import Path

import pytest

import photo_fx

EFFECTS = Path(photo_fx.__file__).parent / "effects"
EFFECT_MODULES = sorted(EFFECTS.glob("*/effect.py"))
TRIVIAL = {0, 1, 2, 0.5, 255}
MARKER = "# constant:"


def hidden_tunables(path: Path) -> list[str]:
    """Each numeric literal in the module at `path` that is neither a parameter, trivial, nor marked a constant, as
    'file:line: literal'."""
    source = path.read_text(encoding="utf-8")
    marked = {
        token.start[0]
        for token in tokenize.generate_tokens(io.StringIO(source).readline)
        if token.type == tokenize.COMMENT and token.string.startswith(MARKER)
    }
    tree = ast.parse(source)
    declared = {
        id(inner)
        for node in ast.walk(tree)
        if isinstance(node, ast.Call) and _called_name(node.func) == "param"
        for inner in ast.walk(node)
    }
    return [
        f"{path}:{node.lineno}: {node.value!r}"
        for node in ast.walk(tree)
        if isinstance(node, ast.Constant)
        and isinstance(node.value, int | float)
        and not isinstance(node.value, bool)
        and node.value not in TRIVIAL
        and id(node) not in declared
        and not {node.lineno, node.lineno - 1} & marked
    ]


def _called_name(func: ast.expr) -> str | None:
    if isinstance(func, ast.Name):
        return func.id
    return func.attr if isinstance(func, ast.Attribute) else None


def test_there_are_effect_packages_to_scan():
    assert len(EFFECT_MODULES) >= 6  # the colour effects at least: a wrong glob must not pass vacuously


@pytest.mark.parametrize("path", EFFECT_MODULES, ids=[path.parent.name for path in EFFECT_MODULES])
def test_every_number_in_an_effect_is_a_parameter_or_a_cited_constant(path):
    assert hidden_tunables(path) == []


# --- the scanner itself: known-good and known-bad modules ---------------------------------------------------------

GOOD = '''
    """A probe effect module that keeps the rule."""
    import numpy as np

    _SRGB_BREAK = 0.04045  # constant: IEC 61966-2-1, where the sRGB curve's linear segment ends

    class ProbeParams(EffectParams):
        radius: float = param(3.5, unit="px", min=0.25, max=40, doc="Blur radius.")

    def probe(rgb, params):
        # constant: 1e-9 only keeps a flat picture from dividing by zero
        safe = max(float(rgb.max()), 1e-9)
        return np.clip(rgb * 0.5 / safe * 2 - 1, 0, 255) * params.radius
'''

BAD = {
    "a factor in an expression": ("    return rgb * radius * 0.35\n", ["0.35"]),
    "the values in a tuple": ("    weights = (0.2, 0.7)\n", ["0.2", "0.7"]),
    "a private helper's default": ("    def _soften(x, amount=0.8):\n        return x\n", ["0.8"]),
    "an unmarked module constant": ("    _EDGE = 3.5  # the edge width\n", ["3.5"]),
    "an epsilon without the marker": ("    safe = max(total, 1e-9)\n", ["1e-09"]),
}


def _module(tmp_path: Path, body: str) -> Path:
    path = tmp_path / "effect.py"
    path.write_text(textwrap.dedent(body), encoding="utf-8")
    return path


def test_the_scanner_passes_parameters_trivial_values_and_marked_constants(tmp_path):
    assert hidden_tunables(_module(tmp_path, GOOD)) == []


@pytest.mark.parametrize(("line", "literals"), BAD.values(), ids=BAD.keys())
def test_the_scanner_names_the_file_line_and_literal_of_each_hidden_tunable(tmp_path, line, literals):
    path = _module(tmp_path, "def probe(rgb, radius):\n" + line)
    assert sorted(hidden_tunables(path)) == [f"{path}:2: {literal}" for literal in literals]


def test_the_marker_on_the_same_line_or_the_line_before_makes_a_literal_a_constant(tmp_path):
    same_line = "def probe(rgb, radius):\n    return rgb * radius * 0.35  # constant: the golden-angle share\n"
    line_before = "def probe(rgb, radius):\n    # constant: the golden-angle share\n    return rgb * radius * 0.35\n"
    two_lines_before = "# constant: too far away\n\ndef probe(rgb, radius):\n    return rgb * radius * 0.35\n"
    assert hidden_tunables(_module(tmp_path, same_line)) == []
    assert hidden_tunables(_module(tmp_path, line_before)) == []
    path = _module(tmp_path, two_lines_before)
    assert hidden_tunables(path) == [f"{path}:4: 0.35"]
