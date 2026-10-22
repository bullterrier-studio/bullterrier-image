"""The project's tooling is wired the way CLAUDE.md promises: poe tasks, commit-message settings, hook wiring."""

import re
import shlex
import tomllib
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
# the only commands that are not dev dependencies: tools every machine that can clone the repository already has
SYSTEM_COMMANDS = {"git", "uv"}


@pytest.fixture(scope="module")
def pyproject():
    return tomllib.loads((ROOT / "pyproject.toml").read_text())


def dev_dependency_names(pyproject) -> set[str]:
    return {re.split(r"[<>=!~ \[;]", spec, maxsplit=1)[0].lower() for spec in pyproject["dependency-groups"]["dev"]}


def poe_commands(task) -> list[str]:
    """Every command line a poe task runs, flattened (sequence items may be strings, cmd tables or refs)."""
    if isinstance(task, str):
        return [task]
    if "cmd" in task:
        return [task["cmd"]]
    if "sequence" in task:
        return [command for item in task["sequence"] for command in poe_commands(item)]
    return []  # a `ref` runs another task, whose commands are listed there


def test_commit_message_settings_are_the_documented_ones(pyproject):
    assert pyproject["tool"]["commit-message"] == {
        "types": ["feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert"],
        "header-max-length": 72,
        "body-max-line-length": 100,
        "scope-pattern": "^[a-z0-9][a-z0-9/_-]*$",
    }


def test_poe_tasks_exist_and_the_gate_runs_format_lint_tests_in_order(pyproject):
    tasks = pyproject["tool"]["poe"]["tasks"]
    assert {"check", "fix", "test", "setup"} <= tasks.keys()
    assert poe_commands(tasks["test"]) == ["pytest -q"]
    assert poe_commands(tasks["fix"]) == ["ruff check --fix .", "ruff format ."]
    # the test command is defined once (task `test`) and the gate refers to it
    assert tasks["check"]["sequence"] == ["ruff format --check .", "ruff check .", {"ref": "test"}]
    assert poe_commands(tasks["setup"]) == [
        "pre-commit install --install-hooks",
        "git config --local commit.template .gitmessage",
    ]


def test_every_poe_command_runs_a_dev_dependency_or_a_system_tool(pyproject):
    allowed = dev_dependency_names(pyproject) | SYSTEM_COMMANDS
    for name, task in pyproject["tool"]["poe"]["tasks"].items():
        for command in poe_commands(task):
            assert shlex.split(command)[0] in allowed, f"poe task {name!r} runs {command!r}"


def test_dev_group_names_the_tools_the_tasks_and_tests_use(pyproject):
    assert {"pytest", "ruff", "pre-commit", "poethepoet", "pyyaml"} <= dev_dependency_names(pyproject)
