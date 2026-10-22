"""The recipe machinery is split into modules that depend on each other in one direction only:

    timing <- steps <- loader, runner          (and `recipe`, the public namespace, imports all four)

`ALLOWED` is that direction as a table: for each module, every package module it may import (anywhere in its text,
function-level imports included). The tests read the modules' source with `ast`, so an import that points the wrong
way, or a cycle, fails here, not later as an ImportError on some other import order. `loader` and `runner` must not know
each other: that is the seam later changes to the file format or to running a recipe are made on separately.
"""

import ast
import re
import subprocess
import sys
from pathlib import Path

import pytest

import photo_fx

PACKAGE = Path(photo_fx.__file__).parent


def internal_imports(source: str) -> set[str]:
    """Names of the photo_fx modules that `source` (a module inside the package) imports, anywhere in its text,
    including imports inside functions."""
    found = set()
    for node in ast.walk(ast.parse(source)):
        if isinstance(node, ast.ImportFrom):
            if node.level == 1:  # `from .x import y` or `from . import x`
                base = node.module
            elif node.level == 0 and node.module and node.module.split(".")[0] == "photo_fx":
                base = ".".join(node.module.split(".")[1:]) or None
            else:
                continue
            if base:
                found.add(base.split(".")[0])
            else:
                found.update(alias.name.split(".")[0] for alias in node.names)
        elif isinstance(node, ast.Import):
            found.update(a.name.split(".")[1] for a in node.names if a.name.startswith("photo_fx."))
    return found


# Who may import whom. Keep in step with the "Dependency direction" paragraph of each module's docstring.
ALLOWED = {
    "timing": set(),  # imports nothing from photo_fx
    "steps": {"timing", "effects"},
    "loader": {"steps", "timing"},
    "runner": {"steps", "effects", "imageio", "matte"},
    "recipe": {"timing", "steps", "loader", "runner"},
}


def violations(graph: dict[str, set[str]]) -> list[str]:
    """What is wrong with a module dependency graph (module -> package modules it imports): an import `ALLOWED` does
    not list, or a cycle among the modules of the graph. Empty when the graph is fine."""
    problems = [
        f"{module} imports {dependency}, which {module} may not (allowed: {sorted(ALLOWED[module]) or 'nothing'})"
        for module in sorted(graph)
        for dependency in sorted(graph[module] - ALLOWED[module])
    ]
    done, active = set(), []

    def visit(module: str) -> None:
        if module in active:
            problems.append("cycle: " + " -> ".join([*active[active.index(module) :], module]))
        elif module not in done:
            active.append(module)
            for dependency in sorted(graph.get(module, set()) & set(graph)):
                visit(dependency)
            active.pop()
            done.add(module)

    for module in sorted(graph):
        visit(module)
    return problems


def graph_of(sources: dict[str, str]) -> dict[str, set[str]]:
    return {module: internal_imports(source) for module, source in sources.items()}


@pytest.mark.parametrize(
    "source, expected",
    [
        ("from .steps import Step", {"steps"}),
        ("from . import runner, timing", {"runner", "timing"}),
        ("from photo_fx.loader import load_recipe", {"loader"}),
        ("from photo_fx import steps", {"steps"}),
        ("import photo_fx.runner", {"runner"}),
        ("def f():\n    from .steps import Step\n", {"steps"}),  # a lazy import is still a dependency
        ("import numpy as np\nfrom collections import deque\nfrom .. import x", set()),  # external or out of package
        ("from .effects.base import x", {"effects"}),  # only the first component names the module
    ],
)
def test_internal_imports_finds_every_way_of_importing_a_package_module(source, expected):
    assert internal_imports(source) == expected


GOOD = {
    "timing": "from dataclasses import dataclass",
    "steps": "from . import effects\nfrom .timing import Animated",
    "loader": "from .steps import Recipe\nfrom .timing import Animation",
    "runner": "from . import effects, imageio\nfrom .matte import subject_matte\nfrom .steps import Recipe",
    "recipe": "from .loader import load_recipe\nfrom .runner import render"
    "\nfrom .steps import Step\nfrom .timing import ramp",
}


def test_a_sound_set_of_modules_has_no_violations():
    assert violations(graph_of(GOOD)) == []


@pytest.mark.parametrize(
    "module, source, expect",
    [
        ("runner", "from .loader import load_recipe", "runner imports loader"),
        ("runner", "def render():\n    from . import loader\n", "runner imports loader"),  # lazy
        ("loader", "from .runner import render", "loader imports runner"),
        ("loader", "def load():\n    import photo_fx.runner\n", "loader imports runner"),  # lazy
        ("timing", "from .steps import Step", "timing imports steps"),
        ("timing", "def f():\n    from .steps import Step\n", "timing imports steps"),  # lazy
        ("timing", "from . import effects", "timing imports effects"),  # not one of the five, still not allowed
        ("steps", "from .runner import render", "steps imports runner"),
        ("steps", "from .loader import load_recipe", "steps imports loader"),
        ("runner", "from .recipe import Recipe", "runner imports recipe"),
    ],
)
def test_an_import_against_the_direction_is_reported(module, source, expect):
    problems = violations(graph_of({**GOOD, module: GOOD[module] + "\n" + source}))
    assert any(re.search(expect, problem) for problem in problems), problems


@pytest.mark.parametrize(
    "graph, expect",
    [
        ({"steps": {"loader"}, "loader": {"steps"}}, "cycle: loader -> steps -> loader"),
        (
            {"steps": {"runner"}, "runner": {"loader"}, "loader": {"steps"}},
            "cycle: loader -> steps -> runner -> loader",
        ),
    ],
)
def test_a_cycle_is_reported_even_when_the_table_is_edited_to_allow_its_edges(graph, expect, monkeypatch):
    monkeypatch.setitem(ALLOWED, "steps", {"runner", "loader"})
    monkeypatch.setitem(ALLOWED, "loader", {"steps", "runner"})
    monkeypatch.setitem(ALLOWED, "runner", {"loader", "steps"})
    assert expect in violations(graph)


def test_the_real_modules_depend_on_each_other_in_the_stated_direction_only(monkeypatch):
    # M1 moved the four engine modules into core/ (which now holds the registry) and made the runner take the subject
    # by injection; the import-linter contracts that replace this table follow in the next commit.
    monkeypatch.setitem(ALLOWED, "steps", {"timing", "registry"})
    monkeypatch.setitem(ALLOWED, "runner", {"steps", "registry", "ops"})
    monkeypatch.delitem(ALLOWED, "recipe")
    graph = {name: internal_imports((PACKAGE / "core" / f"{name}.py").read_text()) for name in ALLOWED}
    assert violations(graph) == []
    assert graph["steps"] >= {"timing"} and graph["runner"] >= {"steps"} and graph["loader"] >= {"steps"}  # not vacuous


def test_the_recipe_namespace_is_reachable_from_a_fresh_import_of_the_package():
    """`import photo_fx; photo_fx.recipe` worked before the split, with every public name of the old module."""
    names = "Animated Animation Recipe Stage Step load_recipe ramp render render_all render_frames trace".split()
    code = (
        "import photo_fx\n"
        f"names = {names!r}\n"
        "missing = [n for n in names if not hasattr(photo_fx.recipe, n)]\n"
        "assert not missing, missing\n"
        "assert photo_fx.recipe.render is photo_fx.render and photo_fx.recipe.load_recipe is photo_fx.load_recipe\n"
    )
    result = subprocess.run([sys.executable, "-I", "-c", code], capture_output=True, text=True, check=False)
    assert result.returncode == 0, result.stderr
