"""No hidden tunables (CLAUDE.md rule 5): every number that shapes an effect's result is a parameter.

An architecture test: its subject is the source of the effect packages (`effects/<name>/effect.py`) and of the helpers
they share (`ops/*.py`), so it reads them with `ast`. A numeric literal fails unless it is
- part of a parameter's declaration (inside `param(...)`: default, min, max), or
- a trivial value (0, 1, 2, 0.5, 255; a minus sign does not matter) used as an arithmetic operand (exponents included:
  squares and square roots are mathematics), a comparison operand, an index or slice bound, or a positional bound, fill
  or matrix entry of a `BOUND_CALLS` function (`np.clip(x, 0, 1)`), or
- an array axis (`axis=`, `axes=`, or a positional of `transpose`), or
- the integer 3, the colour channels, as a size: a slice bound (`hsv[..., 2:3]`) or an argument or shape element of
  `SIZE_CALLS` (`zeros((h, w, 3))`, `reshape(h, w, 3)`, `repeat(grey, 3, axis=-1)`); anywhere else a 3 is a number like
  any other (`rgb * 3` fails), or
- marked `# constant: <reason of at least three words>`: a trailing marker covers its own statement, a marker on a
  line of its own covers the next statement (a standard, such as Rec. 709 weights, or a pure guard, such as an epsilon).
Anything else fails, whatever its value: an option of a call, by keyword or position (`sigma=2`, `iterations=1`,
`gaussian_filter(x, 2)`), a default of a function, a value assigned to a name, an element of a list that is not an
index.
There is no list of exemptions: a finding is fixed (a parameter, or a cited constant), never waved through.

A `# constant:` marker excuses a number, never a library's option. A literal option of a call into numpy, scipy or PIL
(`order=1`, `mode="reflect"`, `kind="linear"`, `truncate=4.0`, `size=(3, 3)`) or a library's own named choice
(`resample=Image.BICUBIC`) fails, marker or no marker: such an option shapes the result, so it is a documented
parameter, whatever reasoning a comment gives (batch 2 shipped `order=1  # constant:` in `radial_blur`). A call counts
as a call into a library when it
- is reached through a name imported from one (`np.pad(...)`, `Image.open(f).resize(...)`), or
- is a method of a value that provably comes from one: a parameter annotated with a library type (`img: Image.Image`,
  `rgb: np.ndarray`), or a name assigned from an expression that starts at a library name or at such a value
  (`small = Image.fromarray(x)`, `plane = rgb[..., 0] * 2`, then `small.resize(..., resample=Image.BICUBIC)`).
Not decided statically, so not reported (a blind spot, not a pass): a method of a value that went through our own
function, a builtin or an attribute (`blur(x).astype(..)`, `self.image.resize(..)`), of an element of a container,
or of a parameter without an annotation; a literal passed by position (`np.pad(x, 2, "edge")`: it cannot be told from
a subscript string such as `np.einsum("ij,jk", a, b)`); and a boolean or None option (`replace=False`, `keepdims=True`),
which cannot be told from plumbing. Array axes (`axis=`, `axes=`) and `dtype=` are plumbing, not options. The options
of our own functions are not scanned (`blend(a, b, mode="mean")`): their bodies are.
"""

import ast
import io
import re
import textwrap
import tokenize
from pathlib import Path

import pytest

import bullterrier_image

PACKAGE = Path(bullterrier_image.__file__).parent
SCANNED = sorted([*(PACKAGE / "effects").glob("*/effect.py"), *(PACKAGE / "ops").glob("*.py")])
TRIVIAL = {0, 1, 2, 0.5, 255}
CHANNELS = 3
MARKER = re.compile(r"#\s*constant:(.*)")
MIN_REASON_WORDS = 3
LIBRARIES = {"numpy", "scipy", "PIL"}
# keywords that are plumbing, not a choice: array axes, and the type an array is made in
PLUMBING_KEYWORDS = {"axis", "axes", "dtype"}
# calls whose positional numbers are array axes (exempt, like `axis=`)
AXIS_CALLS = {"transpose", "moveaxis", "swapaxes"}
# calls whose positional 3 is the number of colour channels, a size
SIZE_CALLS = {"zeros", "ones", "empty", "reshape", "repeat"}
# calls whose trivial positional numbers are bounds, fills or matrix entries, never a library's option: in any other
# call (`gaussian_filter(rgb, 2)`) a positional number is an option like a keyword one
BOUND_CALLS = {
    "clip", "minimum", "maximum", "min", "max", "where", "full", "full_like", "pad", "range", "array", "uniform",
}  # fmt: skip


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
    if (
        node.value == CHANNELS
        and isinstance(node.value, int)
        and (isinstance(parent, ast.Slice) or called in SIZE_CALLS)
    ):
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


def library_options(source: str) -> list[tuple[int, str]]:
    """(line, keyword) of each literal option of a numpy, scipy or PIL call in `source`, marker or no marker."""
    tree = ast.parse(source)
    libraries = _library_names(tree)
    values = _library_values(tree, libraries)
    return sorted(
        (keyword.value.lineno, name)
        for call in ast.walk(tree)
        if isinstance(call, ast.Call) and _origins(call.func) & (libraries | values)
        for keyword in call.keywords
        if (name := keyword.arg) and name not in PLUMBING_KEYWORDS and _is_a_choice(keyword.value, libraries)
    )


def _library_names(tree: ast.Module) -> set[str]:
    """The names the module binds to numpy, scipy or PIL, or to anything inside them."""
    names: set[str] = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            names.update((alias.asname or alias.name).split(".")[0] for alias in node.names if _is_library(alias.name))
        elif isinstance(node, ast.ImportFrom) and node.module and _is_library(node.module):
            names.update(alias.asname or alias.name for alias in node.names)
    return names


def _library_values(tree: ast.Module, libraries: set[str]) -> set[str]:
    """The names that provably hold a numpy, scipy or PIL value, so that a method call on one is a library call.

    A parameter or variable annotated with a library type (`rgb: np.ndarray`, `img: Image.Image`), and a name assigned
    from an expression that starts at a library name or at such a value (`small = Image.fromarray(x).convert("L")`,
    `edge = rgb[..., 0] - plane`), found by repeating until no name is added. Not decided, so not reported: a value that
    passes through our own function or a builtin (`blur(x)`, `float(x)`), an attribute (`self.image`), an element of a
    container, and a parameter with no annotation. The scan misses those rather than guess.
    """
    values: set[str] = set()
    annotated = [
        node.arg
        for node in ast.walk(tree)
        if isinstance(node, ast.arg)
        and node.annotation is not None
        and any(isinstance(inner, ast.Name) and inner.id in libraries for inner in ast.walk(node.annotation))
    ]
    values.update(annotated)
    assignments = [
        (target, node.value)
        for node in ast.walk(tree)
        if isinstance(node, ast.Assign | ast.AnnAssign) and node.value is not None
        for target in (node.targets if isinstance(node, ast.Assign) else [node.target])
    ]
    grown = True
    while grown:
        grown = False
        for target, value in assignments:
            if _origins(value) & (libraries | values):
                for name in ast.walk(target):
                    if isinstance(name, ast.Name) and name.id not in values:
                        values.add(name.id)
                        grown = True
    return values


def _is_library(module: str) -> bool:
    return module.split(".")[0] in LIBRARIES


def _origins(expr: ast.expr) -> set[str]:
    """The names an expression is built from along its value chain: `ndimage` for `ndimage.gaussian_filter` and for
    `Image.open(f).resize`, `rgb` for `rgb[..., 0].astype` and for `rgb * 2`. A call of a plain name (`blur(x)`) starts
    nowhere known, so its value is not traced."""
    if isinstance(expr, ast.Name):
        return {expr.id}
    if isinstance(expr, ast.Attribute | ast.Subscript | ast.Starred):
        return _origins(expr.value)
    if isinstance(expr, ast.Call):
        return _origins(expr.func.value) if isinstance(expr.func, ast.Attribute) else _origins(expr.func)
    if isinstance(expr, ast.BinOp):
        return _origins(expr.left) | _origins(expr.right)
    if isinstance(expr, ast.UnaryOp):
        return _origins(expr.operand)
    return set()


def _is_a_choice(value: ast.expr, libraries: set[str]) -> bool:
    """A number or string literal (or a tuple of them), or a library's own named value (`Image.BICUBIC`).

    A boolean or None is no choice here: `replace=False`, `keepdims=True` are on/off switches that cannot be told from
    plumbing without knowing the function.
    """
    try:
        literal = ast.literal_eval(value)
    except ValueError:
        return isinstance(value, ast.Attribute) and bool(_origins(value) & libraries)
    return literal is not None and not isinstance(literal, bool)


def _called_name(func: ast.expr) -> str | None:
    if isinstance(func, ast.Name):
        return func.id
    return func.attr if isinstance(func, ast.Attribute) else None


def _module_name(path: Path) -> str:
    return path.relative_to(PACKAGE).as_posix()


def _findings(path: Path) -> list[str]:
    return [f"{path}:{line}: {value!r}" for line, value in hidden_tunables(path.read_text(encoding="utf-8"))]


def test_the_scan_covers_the_effect_packages_and_the_ops_modules():
    names = {_module_name(path) for path in SCANNED}
    assert {"effects/grade/effect.py", "effects/clarity/effect.py", "ops/color.py", "ops/cells.py"} <= names


@pytest.mark.parametrize("path", SCANNED, ids=[_module_name(path) for path in SCANNED])
def test_every_number_is_a_parameter_or_a_cited_constant(path):
    assert _findings(path) == []


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
        grey = np.repeat(mixed[..., :1], 3, axis=-1).reshape(rows, 3) + np.zeros((rows, rows, 3))[..., 2:3]
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
    "a three that is a factor, not a size": ("return rgb * 3", [3]),
    "a three that is a library's option": ("return np.pad(rgb, 3)", [3]),
    "a non-trivial bound": ("return np.clip(rgb, 0.0, 0.9)", [0.9]),
    "a trivial default of a helper": ("def _mix(a, b, weight=0.5):\n        return a", [0.5]),
    "a trivial value assigned to a name": ("weight = 0.5", [0.5]),
    "a trivial weight in a list argument of a keyword": ("return np.average(rgb, weights=[0.5, 0.5])", [0.5, 0.5]),
}


@pytest.mark.parametrize(("line", "literals"), BAD.values(), ids=BAD.keys())
def test_the_scanner_finds_each_hidden_tunable_with_its_line(line, literals):
    source = textwrap.dedent(f"def probe(rgb, radius, total, mask):\n    {line}\n")
    assert sorted(hidden_tunables(source)) == sorted((2, literal) for literal in literals)


MARKED = {  # module source -> the literals still found
    "a marker without a reason": ("x = 0.35  # constant:\n", [0.35]),
    "a reason of two words": ("x = 0.35  # constant: golden ratio\n", [0.35]),
    "a reason of three words": ("x = 0.35  # constant: the golden-angle share\n", []),
    "a trailing marker covers its own statement only": (
        "a = 0.3  # constant: the first standard value\nb = 0.35\n",
        [0.35],
    ),
    "a trailing marker covers the whole statement, not the next": (
        "weights = (\n    0.2,  # constant: Rec. 709 weights of the channels\n    0.7,\n)\nother = 0.9\n",
        [0.9],
    ),
    "a marker on its own line covers the next statement only": (
        "# constant: the golden-angle share\nx = 0.35\ny = 0.36\n",
        [0.36],
    ),
    "a marker before a def covers only the def line": (
        "# constant: too far from its value\n\n\ndef probe():\n    return 0.35\n",
        [0.35],
    ),
    "a marker on a decorator covers the decorator, not the body": (
        "@lru_cache(maxsize=16)  # constant: a cache size only\ndef probe():\n    return 0.35\n",
        [0.35],
    ),
    "an unmarked decorator": ("@lru_cache(maxsize=16)\ndef probe():\n    return 0.35\n", [16, 0.35]),
    "a literal in a param declaration is never a finding": (
        'radius: float = param(0.35, unit="px", min=0.1, max=7, doc="Radius.")\n',
        [],
    ),
}


@pytest.mark.parametrize(("source", "found"), MARKED.values(), ids=MARKED.keys())
def test_a_constant_marker_with_a_reason_covers_its_statement_and_a_param_declaration_needs_none(source, found):
    assert scan(source) == found


# --- literal options of library calls ----------------------------------------------------------------------------


@pytest.mark.parametrize("path", SCANNED, ids=[_module_name(path) for path in SCANNED])
def test_no_literal_option_of_a_library_call_in_the_source(path):
    assert library_options(path.read_text(encoding="utf-8")) == []


HEADER = "import numpy as np\nfrom PIL import Image\nfrom scipy import ndimage\n\n\n"

OPTIONS = {  # module body -> (line, keyword) of each literal library option; the header is 5 lines
    "the batch 2 shape: order=1 behind a marker on its own line": (
        "def probe(plane, matrix, offset):\n"
        "    # constant: linear sampling is enough here\n"
        "    return ndimage.affine_transform(plane, matrix, offset=offset, order=1)\n",
        [(8, "order")],
    ),
    "a trailing marker excuses a string option": (
        'def probe(x):\n    return np.pad(x, 2, mode="edge")  # constant: the picture carries on past its edge\n',
        [(7, "mode")],
    ),
    "a string option with no marker at all": (
        'def probe(x):\n    return np.pad(x, 2, mode="edge")\n',
        [(7, "mode")],
    ),
    "a string option of another library function": (
        'from scipy.interpolate import interp1d\n\n\ndef probe(x, y):\n    return interp1d(x, y, kind="linear")\n',
        [(10, "kind")],
    ),
    "each option of a call that runs over several lines": (
        "def probe(distance, sigma):\n"
        "    # constant: scipy's reflect mode and 4-sigma truncate\n"
        "    return ndimage.gaussian_filter(\n"
        "        distance,\n"
        "        sigma,\n"
        '        mode="reflect",\n'
        "        truncate=4.0,\n"
        "    )\n",
        [(11, "mode"), (12, "truncate")],
    ),
    "a tuple option": (
        "def probe(x):\n    return ndimage.uniform_filter(x, size=(3, 3))  # constant: a small box is enough\n",
        [(7, "size")],
    ),
    "a library's named choice in a call chain that starts at the library": (
        "def probe(path):\n"
        "    return Image.open(path).resize((8, 8), resample=Image.BICUBIC)  # constant: bicubic looks best\n",
        [(7, "resample")],
    ),
    "a name imported from the library": (
        "from scipy.ndimage import gaussian_filter\n\n\n"
        "def probe(x):\n    return gaussian_filter(x, 2.0, truncate=4.0)  # constant: a standard cutoff\n",
        [(10, "truncate")],
    ),
    "a method of a picture parameter (the resize shape)": (
        "def probe(img: Image.Image):\n    return img.resize((8, 8), resample=Image.BICUBIC)\n",
        [(7, "resample")],
    ),
    "a method of an array parameter, annotated as optional": (
        "def probe(x: np.ndarray | None):\n    return x.astype(np.uint8, casting='unsafe')\n",
        [(7, "casting")],
    ),
    "a method of a name assigned from a library call": (
        "def probe(x):\n    small = Image.fromarray(x)\n    return small.resize((8, 8), resample=Image.LANCZOS)\n",
        [(8, "resample")],
    ),
    "a method of a name derived from an annotated array by indexing and arithmetic": (
        "def probe(rgb: np.ndarray):\n"
        "    plane = rgb[..., 0] * 2\n"
        "    again = plane\n"
        '    return again.argsort(kind="stable")\n',
        [(9, "kind")],
    ),
    "a method of a name derived from an annotated array as the right operand": (
        'def probe(rgb: np.ndarray):\n    plane = 2 - rgb\n    return plane.argsort(kind="stable")\n',
        [(8, "kind")],
    ),
}


@pytest.mark.parametrize(("body", "found"), OPTIONS.values(), ids=OPTIONS.keys())
def test_a_literal_option_of_a_library_call_is_found_whatever_a_comment_says(body, found):
    assert library_options(HEADER + body) == found


NOT_OPTIONS = {  # module body: nothing to report
    "a mathematical constant passed by position": (
        "def probe(angle):\n    return np.cos(angle * 2.3999632)  # constant: the golden angle in radians\n"
    ),
    "a guard value passed by position": (
        "def probe(x):\n    # constant: an epsilon that only keeps a flat picture from dividing by zero\n"
        "    return np.maximum(x, 1e-9)\n"
    ),
    "array axes and a dtype": (
        "def probe(x):\n    return np.sum(x, axis=(0, 1), dtype=np.float64)  # constant: sum in double\n"
    ),
    "an option that is a parameter": (
        "def probe(plane, matrix, params):\n    return ndimage.affine_transform(plane, matrix, order=params.order)\n"
    ),
    "a documented parameter passed through a method of a picture": (
        "def probe(img: Image.Image, params):\n    return img.resize((8, 8), resample=params.resample)\n"
    ),
    "a documented parameter passed through a method of an array": (
        "def probe(x: np.ndarray, edge):\n    return x.astype(np.float32, casting=edge)\n"
    ),
    "switches and plumbing of a method": (
        "def probe(x: np.ndarray):\n    return x.sum(axis=0, keepdims=True).astype(np.float32, copy=False)\n"
    ),
    "a None option": "def probe(x: np.ndarray):\n    return np.sort(x, order=None)\n",
    "a keyword of the module's own function": (
        "def blend(a, b, weight):\n    return a\n\n\n"
        "def probe(a, b):\n    return blend(a, b, weight=0.5)  # constant: an even mix of the two\n"
    ),
    "a string option of the module's own function": (
        'def blend(a, b, how):\n    return a\n\n\ndef probe(a, b):\n    return blend(a, b, how="mean")\n'
    ),
    "a string method of a plain string": ('def probe(text):\n    return "a,b".split(sep=",")\n'),
}


@pytest.mark.parametrize("body", NOT_OPTIONS.values(), ids=NOT_OPTIONS.keys())
def test_the_option_check_passes_constants_axes_switches_parameters_and_calls_of_our_own(body):
    assert library_options(HEADER + body) == []
