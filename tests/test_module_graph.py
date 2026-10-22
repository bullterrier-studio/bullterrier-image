"""The recipe machinery is split into modules that depend on each other in one direction only:

    timing <- steps <- loader, runner          (and `recipe`, the public namespace, imports all four)

These tests read the modules' source with `ast`, so a cycle or an import that points the wrong way fails here, not
later as an ImportError on some other import order.
"""

import ast
import re
from pathlib import Path

import pytest

import photo_fx

PACKAGE = Path(photo_fx.__file__).parent
MODULES = ("timing", "steps", "loader", "runner", "recipe")


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


def violations(graph: dict[str, set[str]]) -> list[str]:
    """What is wrong with a module dependency graph (module -> modules it imports): a cycle, or `timing` importing any
    other module of the set. Empty when the graph is fine."""
    problems = (
        [f"timing imports {sorted(graph['timing'] & set(graph))}"] if graph.get("timing", set()) & set(graph) else []
    )
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


def test_a_sound_graph_has_no_violations():
    graph = {
        "timing": set(),
        "steps": {"timing"},
        "loader": {"steps", "timing"},
        "runner": {"steps"},
        "recipe": {"loader", "runner"},
    }
    assert violations(graph) == []


@pytest.mark.parametrize(
    "graph, expect",
    [
        (
            {"timing": set(), "steps": {"loader"}, "loader": {"steps"}, "runner": set()},
            "cycle: loader -> steps -> loader",
        ),
        (
            {"timing": set(), "steps": {"runner"}, "runner": {"loader"}, "loader": {"steps"}},
            "cycle: loader -> steps -> runner -> loader",
        ),
        ({"timing": set(), "steps": {"steps"}}, "cycle: steps -> steps"),
        (
            {"timing": {"steps"}, "steps": set()},
            "timing imports \\['steps'\\]",
        ),  # not a cycle, but timing must be a leaf
        ({"timing": {"recipe"}, "recipe": {"timing"}}, "timing imports"),
    ],
)
def test_a_cycle_or_a_timing_import_is_reported(graph, expect):
    assert any(re.search(expect, problem) for problem in violations(graph)), violations(graph)


def test_the_real_modules_depend_on_each_other_in_one_direction_only():
    graph = {name: internal_imports((PACKAGE / f"{name}.py").read_text()) & set(MODULES) for name in MODULES}
    assert violations(graph) == []
    assert graph["steps"] >= {"timing"} and graph["runner"] >= {"steps"} and graph["loader"] >= {"steps"}  # not vacuous
