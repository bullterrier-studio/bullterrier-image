"""No hidden tunables (CLAUDE.md rule 5): every number that shapes an effect's result is a parameter.

An architecture test: its subject is the source of the effect packages (`effects/<name>/effect.py`) and of the helpers
they share (`ops/*.py`), so it reads them with `ast`. A numeric literal fails unless it is
- part of a parameter's declaration (inside `param(...)`: default, min, max), or
- a trivial value (0, 1, 2, 0.5, 255; a minus sign does not matter) used as an arithmetic operand (exponents included:
  squares and square roots are mathematics), a comparison operand, an index or slice bound, or a positional bound, fill
  or matrix entry of a `BOUND_CALLS` function (`np.clip(x, 0, 1)`), or
- an array axis (`axis=`, `axes=`, or a positional of `transpose`), or
- marked `# constant: <reason of at least three words>`: a trailing marker covers its own statement, a marker on a
  line of its own covers the next statement (a standard, such as Rec. 709 weights, or a pure guard, such as an epsilon).
Anything else fails, whatever its value: an option of a call, by keyword or position (`sigma=2`, `iterations=1`,
`gaussian_filter(x, 2)`), a default of a function, a value assigned to a name, an element of a list that is not an
index.
TRANSITIONAL: the legacy loose effect modules join when migrated; `EXEMPT` lists the findings their owners remove.
"""

import ast
import io
import re
import textwrap
import tokenize
from pathlib import Path

import pytest

import photo_fx

PACKAGE = Path(photo_fx.__file__).parent
SCANNED = sorted([*(PACKAGE / "effects").glob("*/effect.py"), *(PACKAGE / "ops").glob("*.py")])
TRIVIAL = {0, 1, 2, 0.5, 255}
MARKER = re.compile(r"#\s*constant:(.*)")
MIN_REASON_WORDS = 3
# calls whose positional numbers are array axes (exempt, like `axis=`)
AXIS_CALLS = {"transpose", "moveaxis", "swapaxes"}
# calls whose trivial positional numbers are bounds, fills or matrix entries, never a library's option: in any other
# call (`gaussian_filter(rgb, 2)`) a positional number is an option like a keyword one
BOUND_CALLS = {
    "clip",
    "minimum",
    "maximum",
    "min",
    "max",
    "where",
    "full",
    "full_like",
    "pad",
    "range",
    "array",
    "uniform",
}

# TRANSITIONAL: findings left in place because the code is being rewritten elsewhere or exposing the value needs effect
# changes. Key: (module relative to photo_fx, literal); value: who removes it and why. One entry per line, so removals
# merge cleanly; an entry that no longer matches anything fails, so it is removed with its finding.
EXEMPT: dict[tuple[str, float], str] = {
    ("ops/color.py", 0.35): "ops audit: vivid's floor on the brightest channel, shaped by ascii/halftone/glitch",
    ("ops/cells.py", 0.02): "evaporate/disperse migration: `lit`'s brightness threshold for a visible piece",
    ("ops/textart.py", 3): "ascii migration (textart is being edited): the channel count needs a `# constant:`",
    ("effects/glitch/effect.py", 1): "glitch (in flight): the second random stream's id needs a `# constant:`",
}


def hidden_tunables(source: str) -> list[tuple[int, float]]:
    """(line, literal) of each numeric literal in the module `source` that the rule above does not allow."""
    tree = ast.parse(source)
    parents = {child: node for node in ast.walk(tree) for child in ast.iter_child_nodes(node)}
    declared = {
        id(inner)
        for node in ast.walk(tree)
        if isinstance(node, ast.Call) and _called_name(node.func) == "param"
        for inner in ast.walk(node)
    }
    covered = _covered_lines(source, tree)
    return sorted(
        (node.lineno, node.value)
        for node in ast.walk(tree)
        if isinstance(node, ast.Constant)
        and isinstance(node.value, int | float)
        and not isinstance(node.value, bool)
        and id(node) not in declared
        and node.lineno not in covered
        and _hidden(node, parents)
    )


def _hidden(node: ast.Constant, parents: dict[ast.AST, ast.AST]) -> bool:
    child: ast.AST = node
    parent = parents[node]
    # a sign, a choice between two values, or a tuple or list: what counts is where the whole is used
    while isinstance(parent, ast.UnaryOp | ast.IfExp | ast.Tuple | ast.List) and not (
        isinstance(parent, ast.IfExp) and child is parent.test
    ):
        child, parent = parent, parents[parent]
    called = _called_name(parent.func) if isinstance(parent, ast.Call) and child in parent.args else None
    if isinstance(parent, ast.keyword):
        return not (parent.arg in ("axis", "axes") and isinstance(node.value, int))
    if called in AXIS_CALLS and isinstance(node.value, int):
        return False
    if isinstance(parent, ast.arguments) or node.value not in TRIVIAL:
        return True
    if isinstance(parent, ast.BinOp | ast.Compare | ast.AugAssign | ast.Slice):
        return False
    if isinstance(parent, ast.Subscript):
        return child is not parent.slice
    return called not in BOUND_CALLS


def _covered_lines(source: str, tree: ast.Module) -> set[int]:
    """The lines whose literals a `# constant:` marker with a reason covers."""
    lines = source.splitlines()
    # a statement's own lines: all of a simple one, only the header (decorators included) of one with a body
    spans = [
        (
            min([stmt.lineno, *(decorator.lineno for decorator in getattr(stmt, "decorator_list", []))]),
            body[0].lineno - 1 if (body := getattr(stmt, "body", None)) else stmt.end_lineno or stmt.lineno,
        )
        for stmt in ast.walk(tree)
        if isinstance(stmt, ast.stmt)
    ]
    covered: set[int] = set()
    for token in tokenize.generate_tokens(io.StringIO(source).readline):
        match = MARKER.match(token.string) if token.type == tokenize.COMMENT else None
        if match is None or len(match.group(1).split()) < MIN_REASON_WORDS:
            continue
        line = token.start[0]
        if lines[line - 1].lstrip().startswith("#"):  # on a line of its own: the next statement
            following = [span for span in spans if span[0] > line]
            chosen = [span for span in following if span[0] == min(start for start, _ in following)]
        else:  # trailing: the innermost statement holding the line
            holding = [span for span in spans if span[0] <= line <= span[1]]
            chosen = [max(holding)] if holding else []
        for start, end in chosen:
            covered.update(range(start, end + 1))
    return covered


def _called_name(func: ast.expr) -> str | None:
    if isinstance(func, ast.Name):
        return func.id
    return func.attr if isinstance(func, ast.Attribute) else None


def _module_name(path: Path) -> str:
    return path.relative_to(PACKAGE).as_posix()


def _unexempted(path: Path) -> list[str]:
    found = hidden_tunables(path.read_text(encoding="utf-8"))
    return [f"{path}:{line}: {value!r}" for line, value in found if (_module_name(path), value) not in EXEMPT]


def test_the_scan_covers_the_effect_packages_and_the_ops_modules():
    names = {_module_name(path) for path in SCANNED}
    assert {"effects/grade/effect.py", "effects/clarity/effect.py", "ops/color.py", "ops/cells.py"} <= names


@pytest.mark.parametrize("path", SCANNED, ids=[_module_name(path) for path in SCANNED])
def test_every_number_is_a_parameter_or_a_cited_constant(path):
    assert _unexempted(path) == []


@pytest.mark.parametrize(("module", "literal"), EXEMPT.keys(), ids=[f"{m}:{v}" for m, v in EXEMPT])
def test_every_transitional_exemption_still_matches_a_finding(module, literal):
    found = hidden_tunables((PACKAGE / module).read_text(encoding="utf-8"))
    assert literal in {value for _, value in found}, f"{module}: {literal} is fixed, delete its EXEMPT entry"


# --- the scanner itself: known-good and known-bad modules ---------------------------------------------------------


def scan(body: str) -> list[float]:
    return [value for _, value in hidden_tunables(textwrap.dedent(body))]


GOOD = '''
    """A probe module that keeps the rule."""
    import numpy as np

    _SRGB_BREAK = 0.04045  # constant: IEC 61966-2-1, where the sRGB curve's linear segment ends

    class ProbeParams(EffectParams):
        radius: float = param(3.5, unit="px", min=0.25, max=40, doc="Blur radius.", min_length=2)

    def probe(rgb, params):
        # constant: 1e-9 only keeps a flat picture from dividing by zero
        safe = max(float(rgb.max()), 1e-9)
        mixed = np.clip(rgb * 0.5 / safe * 2 - 1, 0, 255) * params.radius
        if mixed.shape[0] > 1 and rgb[..., 0].mean() < 0.5:
            mixed[1:-1] += mixed.sum(axis=(0, 1)) ** 2 + rgb[:, :, 2] ** 0.5
        pieces = np.pad(mixed, ((1, 0 if params.radius else 1), (0, 0))).transpose(0, 2, 1, 3, 4)
        return np.clip(np.where(pieces > 0, pieces, 0.0), 0.0, 1.0)
'''


def test_the_scanner_passes_parameters_trivial_operands_axes_and_marked_constants():
    assert scan(GOOD) == []


BAD = {
    "a factor in an expression": ("return rgb * radius * 0.35", [0.35]),
    "the values in a tuple": ("weights = (0.2, 0.7)", [0.2, 0.7]),
    "an unmarked module constant": ("_EDGE = 3.5  # the edge width", [3.5]),
    "an epsilon without the marker": ("safe = max(total, 1e-9)", [1e-9]),
    "a trivial keyword option of a library call": ("return gaussian_filter(rgb, sigma=2)", [2]),
    "a trivial iteration count": ("return binary_dilation(mask, iterations=1)", [1]),
    "a trivial positional option of a library call": ("return gaussian_filter(rgb, 2)", [2]),
    "a non-trivial bound": ("return np.clip(rgb, 0.0, 0.9)", [0.9]),
    "a trivial default of a helper": ("def _mix(a, b, weight=0.5):\n        return a", [0.5]),
    "a trivial value assigned to a name": ("weight = 0.5", [0.5]),
    "a trivial weight in a list argument of a keyword": ("return np.average(rgb, weights=[0.5, 0.5])", [0.5, 0.5]),
}


@pytest.mark.parametrize(("line", "literals"), BAD.values(), ids=BAD.keys())
def test_the_scanner_finds_each_hidden_tunable_with_its_line(line, literals):
    source = textwrap.dedent(f"def probe(rgb, radius, total, mask):\n    {line}\n")
    assert sorted(hidden_tunables(source)) == sorted((2, literal) for literal in literals)


def test_a_marker_needs_a_reason_of_at_least_three_words():
    assert scan("x = 0.35  # constant:\n") == [0.35]
    assert scan("x = 0.35  # constant: golden ratio\n") == [0.35]
    assert scan("x = 0.35  # constant: the golden-angle share\n") == []


def test_a_trailing_marker_covers_its_own_statement_only():
    assert scan("a = 0.3  # constant: the first standard value\nb = 0.35\n") == [0.35]
    multi_line = "weights = (\n    0.2,  # constant: Rec. 709 weights of the channels\n    0.7,\n)\nother = 0.9\n"
    assert scan(multi_line) == [0.9]  # the whole tuple statement is covered, the next statement is not


def test_a_marker_on_its_own_line_covers_the_next_statement_only():
    assert scan("# constant: the golden-angle share\nx = 0.35\ny = 0.36\n") == [0.36]
    assert scan("# constant: too far from its value\n\n\ndef probe():\n    return 0.35\n") == [
        0.35
    ]  # only the def line


def test_a_marker_on_a_decorator_covers_the_decorator_but_not_the_body():
    source = "@lru_cache(maxsize=16)  # constant: a cache size only\ndef probe():\n    return 0.35\n"
    assert scan(source) == [0.35]
    assert scan("@lru_cache(maxsize=16)\ndef probe():\n    return 0.35\n") == [16, 0.35]


def test_a_numeric_literal_in_a_param_declaration_is_never_a_finding():
    assert scan('radius: float = param(0.35, unit="px", min=0.1, max=7, doc="Radius.")\n') == []
