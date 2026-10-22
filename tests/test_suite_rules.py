"""The test suite keeps to its own rule: tests describe behaviour, not implementation (CLAUDE.md, "Testing").

An architecture test: its subject is the structure of the test files, so it reads their source with `ast`. It catches
the couplings a parser can see without false positives: reaching for a private name of photo_fx (importing it, reading
`module._name`, or `getattr(module, "_name")`), and patching photo_fx (`monkeypatch.setattr` / `delattr` / `setitem` /
`delitem`, positional or keyword, or `mock.patch`, `patch.object`, `patch.dict` aimed at a photo_fx module, object or
dotted path). Left to review, because a parser cannot tell them from legitimate code: setting attributes on an
instance of a photo_fx class, `importlib.import_module` of a private module, expectations computed with the production
formula, and assertions that mirror the algorithm.
"""

import ast
from pathlib import Path

import pytest

TESTS = Path(__file__).parent

# (file relative to tests/, the offending name) -> why it is allowed. Every entry needs a reason a reviewer accepts.
ALLOWED: dict[tuple[str, str], str] = {}

_PATCHERS = ("monkeypatch.setattr", "monkeypatch.delattr", "monkeypatch.setitem", "monkeypatch.delitem")


def _private(name: str) -> bool:
    return name.startswith("_") and not (name.startswith("__") and name.endswith("__"))


def _from_photo_fx(module: str | None) -> bool:
    return module is not None and (module == "photo_fx" or module.startswith("photo_fx."))


def violations(source: str) -> list[str]:
    """Each coupling to photo_fx's internals in a test file's `source`, as 'line N: what'."""
    tree = ast.parse(source)
    bound: set[str] = set()  # names that refer to photo_fx modules or objects in this file
    found: list[tuple[int, str]] = []
    for node in ast.walk(tree):
        if isinstance(node, ast.ImportFrom) and _from_photo_fx(node.module):
            for alias in node.names:
                bound.add(alias.asname or alias.name)
                if _private(alias.name) or any(_private(part) for part in node.module.split(".")):
                    found.append((node.lineno, f"imports private {node.module}.{alias.name}"))
        elif isinstance(node, ast.Import):
            for alias in node.names:
                if _from_photo_fx(alias.name):
                    bound.add(alias.asname or alias.name.split(".")[0])
                    if any(_private(part) for part in alias.name.split(".")):
                        found.append((node.lineno, f"imports private {alias.name}"))
    for node in ast.walk(tree):
        if isinstance(node, ast.Attribute) and _private(node.attr) and _refers_to_photo_fx(node.value, bound):
            found.append((node.lineno, f"reads private {ast.unparse(node)}"))
        elif isinstance(node, ast.Call) and _is_private_getattr(node, bound):
            found.append((node.lineno, f"reads private {ast.unparse(node)}"))
        elif isinstance(node, ast.Call) and _is_patch(node.func):
            target = node.args[0] if node.args else next((k.value for k in node.keywords if k.arg == "target"), None)
            if target is None:
                continue
            aimed = isinstance(target, ast.Constant) and isinstance(target.value, str) and _from_photo_fx(target.value)
            if aimed or _refers_to_photo_fx(target, bound):
                found.append((node.lineno, f"patches photo_fx: {ast.unparse(node.func)}({ast.unparse(target)}, ...)"))
    return [f"line {line}: {what}" for line, what in sorted(set(found))]


def _refers_to_photo_fx(node: ast.expr, bound: set[str]) -> bool:
    while isinstance(node, ast.Attribute | ast.Subscript | ast.Call):
        node = node.func if isinstance(node, ast.Call) else node.value
    return isinstance(node, ast.Name) and node.id in bound


def _is_private_getattr(node: ast.Call, bound: set[str]) -> bool:
    """`getattr(m, "_name")` (or hasattr, setattr, delattr) on a photo_fx module or object."""
    if not (isinstance(node.func, ast.Name) and node.func.id in ("getattr", "hasattr", "setattr", "delattr")):
        return False
    if len(node.args) < 2 or not _refers_to_photo_fx(node.args[0], bound):
        return False
    name = node.args[1]
    return isinstance(name, ast.Constant) and isinstance(name.value, str) and _private(name.value)


def _is_patch(func: ast.expr) -> bool:
    """`monkeypatch.setattr/delattr/setitem/delitem`, `patch(...)`, `mock.patch(...)`, `patch.object/dict(...)`."""
    name = ast.unparse(func)
    parts = name.split(".")
    return name.endswith(_PATCHERS) or parts[-1] == "patch" or (parts[-1] in ("object", "dict") and "patch" in parts)


def unexplained(root: Path, allowed: dict[tuple[str, str], str]) -> list[str]:
    """Every violation in the .py files under `root` that no `allowed` entry (file, name) covers."""
    problems = []
    for path in sorted(root.rglob("*.py")):
        relative = path.relative_to(root).as_posix()
        for problem in violations(path.read_text()):
            if not any(file == relative and name in problem for (file, name) in allowed):
                problems.append(f"{relative} {problem}")
    return problems


def stale(root: Path, allowed: dict[tuple[str, str], str]) -> list[str]:
    """The `allowed` entries that cover no violation any more, or give no reason."""
    return [
        f"{file}: {name}"
        for (file, name), reason in allowed.items()
        if not reason.strip()
        or not (root / file).is_file()
        or not any(name in problem for problem in violations((root / file).read_text()))
    ]


def test_no_test_reaches_into_or_patches_photo_fx():
    assert len(list(TESTS.rglob("*.py"))) > 30  # the scan really sees the suite
    problems = unexplained(TESTS, ALLOWED)
    assert problems == [], "\n".join(problems)
    assert stale(TESTS, ALLOWED) == []


def test_an_allowed_entry_silences_exactly_its_violation_and_a_stale_one_is_reported(tmp_path):
    (tmp_path / "test_a.py").write_text("from photo_fx.matte import _one\nfrom photo_fx.matte import _two\n")
    allowed = {("test_a.py", "_one"): "a reason", ("test_a.py", "_gone"): "it was removed", ("test_b.py", "_x"): "?"}
    assert unexplained(tmp_path, allowed) == ["test_a.py line 2: imports private photo_fx.matte._two"]
    assert stale(tmp_path, allowed) == ["test_a.py: _gone", "test_b.py: _x"]
    assert stale(tmp_path, {("test_a.py", "_one"): "  "}) == ["test_a.py: _one"]  # an entry needs a reason


KNOWN_BAD = {
    "private import": "from photo_fx.matte import _refine_edge\n",
    "private module": "from photo_fx.effects._shared import helper\n",
    "private module import": "import photo_fx.core._internal\n",
    "private attribute": "from photo_fx.ops import glyphs\nfonts = glyphs._FONTS\n",
    "private attribute of a dotted module": "import photo_fx.core.registry\nphoto_fx.core.registry._EFFECTS.clear()\n",
    "monkeypatch a module": (
        "from photo_fx import pipeline\n\ndef test(monkeypatch):\n    monkeypatch.setattr(pipeline, 'render', None)\n"
    ),
    "monkeypatch a dotted path": "def test(monkeypatch):\n    monkeypatch.setattr('photo_fx.matte.subject_matte', 0)\n",
    "monkeypatch delattr": "import photo_fx\n\ndef test(monkeypatch):\n    monkeypatch.delattr(photo_fx.fx, 'X')\n",
    "mock.patch": "from unittest import mock\n\nwith mock.patch('photo_fx.pipeline.render'):\n    pass\n",
    "patch.object": (
        "from unittest.mock import patch\nfrom photo_fx import gallery\n\n"
        "with patch.object(gallery, 'regenerate'):\n    pass\n"
    ),
    "monkeypatch with keywords": (
        "from photo_fx import pipeline\n\ndef test(monkeypatch):\n"
        "    monkeypatch.setattr(target=pipeline, name='render', value=None)\n"
    ),
    "getattr of a private name": "from photo_fx import matte\n\nhelper = getattr(matte, '_refine_edge')\n",
    "patch.dict": (
        "from unittest import mock\nfrom photo_fx.core import vocabulary\n\n"
        "with mock.patch.dict(vocabulary.DISCOURAGED, {'x': 'y'}):\n    pass\n"
    ),
    "monkeypatch setitem": (
        "from photo_fx.core import vocabulary\n\ndef test(monkeypatch):\n"
        "    monkeypatch.setitem(vocabulary.DISCOURAGED, 'x', 'y')\n"
    ),
}


@pytest.mark.parametrize("source", KNOWN_BAD.values(), ids=KNOWN_BAD.keys())
def test_the_checker_reports_each_kind_of_coupling(source):
    assert len(violations(source)) == 1


def test_the_checker_reports_the_line_and_the_name():
    (problem,) = violations("import numpy\n\nfrom photo_fx.matte import _refine_edge\n")
    assert problem.startswith("line 3:") and "_refine_edge" in problem


def test_the_checker_passes_public_use_and_the_tests_own_private_names():
    source = '''
import sys
import numpy as np
import photo_fx
from photo_fx import effects
from photo_fx.matte import refine_edge

def _probe(rgb, amount: float = 0.5):
    """A test's own helper may be private."""
    return rgb

class _Helper:
    _value = 1

def test(monkeypatch, tmp_path):
    effects.describe()
    photo_fx.__file__  # a dunder is public
    _Helper()._value  # the test's own private attribute
    monkeypatch.setattr(sys, "argv", ["photo-fx"])  # not photo_fx
    monkeypatch.setattr(np.random, "seed", None)  # not photo_fx
    monkeypatch.syspath_prepend(str(tmp_path))
    monkeypatch.delitem(sys.modules, "probe_module", raising=False)  # not photo_fx
    getattr(effects, "describe")  # a public name
    patch = dict(a=1)  # a test's own variable called patch
    script = "from photo_fx.effects import _order_log"  # text for another interpreter, not an import here
'''
    assert violations(source) == []
