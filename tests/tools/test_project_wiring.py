"""The project's tooling is wired by the rules CLAUDE.md promises: poe tasks, hook wiring, documentation.

Rules, not lists: these tests say what every task and every hook must satisfy (declared correctly, run a pinned tool,
check without rewriting, say how to reproduce a failure), so adding a gate or a hook edits no test.
"""

import re
import shlex
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

import pytest
import yaml

ROOT = Path(__file__).resolve().parents[2]
# the only commands that are not dev dependencies: tools every machine that can clone the repository already has
SYSTEM_COMMANDS = {"git", "uv"}
# tool settings must live in pyproject.toml, never in the hook wiring
TOOL_OPTIONS = ("--select", "--ignore", "--extend-select", "--line-length", "--config", "--strict", "--python-version")
FIXING_OPTIONS = ("--fix", "--unsafe-fixes", "--write")
# pre-commit prints a failing hook's name, so the name carries the command that fixes or reproduces the failure
COMMAND_IN_NAME = re.compile(r"`uv run [^`]+`")


@pytest.fixture(scope="module")
def pyproject():
    return tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))


def dev_dependency_names(pyproject) -> set[str]:
    return {re.split(r"[<>=!~ \[;]", spec, maxsplit=1)[0].lower() for spec in pyproject["dependency-groups"]["dev"]}


def poe_commands(task, tasks: dict | None = None) -> list[str]:
    """Every command line a poe task runs, flattened (sequence items may be strings, cmd tables or refs). A `ref` is
    expanded into the referred task's commands when `tasks` is given, and left out otherwise."""
    if isinstance(task, str):
        return [task]
    if "cmd" in task:
        return [task["cmd"]]
    if "sequence" in task:
        return [command for item in task["sequence"] for command in poe_commands(item, tasks)]
    if "ref" in task and tasks is not None:
        return poe_commands(tasks[task["ref"]], tasks)
    return []


def test_the_gate_runs_the_test_task_by_reference_so_the_test_command_is_defined_once(pyproject):
    assert {"ref": "test"} in pyproject["tool"]["poe"]["tasks"]["check"]["sequence"]


def test_the_gate_only_checks_and_never_rewrites_files(pyproject):
    tasks = pyproject["tool"]["poe"]["tasks"]
    commands = poe_commands(tasks["check"], tasks)
    assert [command for command in commands if set(shlex.split(command)) & set(FIXING_OPTIONS)] == [], commands
    assert "ruff format --check ." in commands  # the formatter, in its checking mode


def test_poe_accepts_every_task_and_would_run_exactly_its_declared_commands(pyproject):
    # poe's own dry run: it rejects a task that is wrongly declared (a bare string in a sequence is a task name, not a
    # command), and prints each command it would run, in order, without running it
    tasks = pyproject["tool"]["poe"]["tasks"]
    for task in tasks:
        result = subprocess.run(
            [sys.executable, "-m", "poethepoet", "--dry-run", task], cwd=ROOT, capture_output=True, text=True
        )
        assert result.returncode == 0, result.stderr
        output = result.stdout + result.stderr
        printed = [line.removeprefix("Poe => ") for line in output.splitlines() if line.startswith("Poe => ")]
        assert printed, f"poe task {task!r} runs nothing:\n{output}"
        assert printed == poe_commands(tasks[task], tasks), output


def test_every_poe_command_runs_a_dev_dependency_or_a_system_tool(pyproject):
    allowed = dev_dependency_names(pyproject) | SYSTEM_COMMANDS
    for name, task in pyproject["tool"]["poe"]["tasks"].items():
        for command in poe_commands(task):
            assert shlex.split(command)[0] in allowed, f"poe task {name!r} runs {command!r}"


def test_dev_group_names_the_tools_the_tasks_and_tests_use(pyproject):
    assert {"pytest", "ruff", "mypy", "pre-commit", "poethepoet", "pyyaml", "types-pyyaml"} <= dev_dependency_names(
        pyproject
    )


# ---------------------------------------------------------------------------------------------------------------------
# .pre-commit-config.yaml: hook wiring only


@pytest.fixture(scope="module")
def precommit():
    return yaml.safe_load((ROOT / ".pre-commit-config.yaml").read_text(encoding="utf-8"))


def hooks_by_id(precommit, repo_kind: str) -> dict[str, dict]:
    repos = [repo for repo in precommit["repos"] if (repo["repo"] == "local") == (repo_kind == "local")]
    return {hook["id"]: hook for repo in repos for hook in repo["hooks"]}


def all_hooks(precommit) -> dict[str, dict]:
    return {**hooks_by_id(precommit, "local"), **hooks_by_id(precommit, "remote")}


def words_of(hook: dict) -> list[str]:
    return shlex.split(hook.get("entry", "")) + hook.get("args", [])


def tool_options_in(arguments: list[str]) -> list[str]:
    return [argument for argument in arguments if argument.split("=")[0] in TOOL_OPTIONS]


def test_the_three_git_hook_types_are_installed_by_a_bare_install(precommit):
    assert precommit["default_install_hook_types"] == ["pre-commit", "commit-msg", "pre-push"]


def test_every_remote_hook_repository_is_pinned_to_a_release_tag(precommit):
    remotes = [repo for repo in precommit["repos"] if repo["repo"] != "local"]
    assert remotes
    for remote in remotes:
        assert re.fullmatch(r"v\d+\.\d+\.\d+", remote["rev"]), remote  # a tag, never a branch such as main


def test_local_hooks_run_the_projects_pinned_tools_through_uv(precommit, pyproject):
    assert shutil.which("uv"), "uv must be installed to run the hooks"
    for hook_id, hook in hooks_by_id(precommit, "local").items():
        tokens = shlex.split(hook["entry"])
        assert hook["language"] == "system", hook_id
        assert tokens[:2] == ["uv", "run"], hook_id
        if tokens[2] == "python":  # the project's own script: it must exist
            assert (ROOT / tokens[3]).is_file(), f"{hook_id} runs {tokens[3]}, which does not exist"
        else:
            assert tokens[2] in dev_dependency_names(pyproject), f"{hook_id} runs {tokens[2]}, not a dev dependency"


def test_file_hooks_run_on_commit_and_whole_repository_hooks_on_push(precommit):
    # a hook with no `stages` would otherwise also run on commit-msg and push, where its files are meaningless or stale
    assert precommit["default_stages"] == ["pre-commit"]
    for hook_id, hook in hooks_by_id(precommit, "remote").items():
        assert "stages" not in hook, hook_id
    local = hooks_by_id(precommit, "local")
    assert local["commit-message"]["stages"] == ["commit-msg"]
    whole = {hook_id for hook_id, hook in local.items() if hook.get("pass_filenames") is False}
    assert whole, "the whole-repository checks (tests, types) must run somewhere"
    for hook_id, hook in local.items():
        if hook_id in whole:
            # on push (and in `poe check`), never on every commit, so commits stay fast; always_run, or a push that
            # touches no Python file would skip it
            assert hook["stages"] == ["pre-push"], hook_id
            assert hook["always_run"] is True, hook_id
        elif hook_id != "commit-message":
            assert "stages" not in hook, hook_id  # a check of the staged files: the default stage, pre-commit


def test_every_local_hook_failure_says_how_to_fix_or_reproduce_it(precommit):
    for hook_id, hook in hooks_by_id(precommit, "local").items():
        assert COMMAND_IN_NAME.search(hook["name"]), f"{hook_id}: {hook['name']!r} names no `uv run ...` command"


def test_the_fix_command_detector_wants_a_quoted_uv_run_command():
    assert COMMAND_IN_NAME.search("mypy (reproduce with `uv run mypy`)")
    assert not COMMAND_IN_NAME.search("mypy (reproduce with uv run mypy)")
    assert not COMMAND_IN_NAME.search("mypy (reproduce with `mypy`)")


def test_no_hook_rewrites_files(precommit):
    for hook_id, hook in all_hooks(precommit).items():
        assert not [word for word in words_of(hook) if word in FIXING_OPTIONS], hook_id
    assert "--check" in words_of(hooks_by_id(precommit, "local")["ruff-format"])


def test_ruff_hooks_skip_the_generated_stub_file_and_say_how_to_fix(precommit):
    # explicit file arguments bypass ruff's `extend-exclude`, and src/photo_fx/fx.pyi is generated: only .py is linted
    for hook_id in ("ruff-format", "ruff-check"):
        hook = hooks_by_id(precommit, "local")[hook_id]
        assert hook["types"] == ["python"], hook_id
        assert "uv run poe fix" in hook["name"], hook_id  # pre-commit prints the name on the failure line


def test_no_tool_settings_live_in_the_hook_wiring(precommit):
    for hook_id, hook in all_hooks(precommit).items():
        assert tool_options_in(words_of(hook)) == [], f"{hook_id} carries tool settings: use pyproject.toml"


def test_the_tool_settings_detector_catches_each_leak_and_passes_clean_hooks():
    assert tool_options_in(["--select=E,F"]) == ["--select=E,F"]
    assert tool_options_in(["--line-length", "100"]) == ["--line-length"]
    assert tool_options_in(["--config=ruff.toml", "--strict"]) == ["--config=ruff.toml", "--strict"]
    assert tool_options_in(["--check", "--maxkb=1000", "-q"]) == []


# ---------------------------------------------------------------------------------------------------------------------
# the documentation names what the configuration defines


def test_claude_md_lists_every_commit_type_from_the_configuration(pyproject):
    claude_md = (ROOT / "CLAUDE.md").read_text(encoding="utf-8")
    for commit_type in pyproject["tool"]["commit-message"]["types"]:
        assert f"`{commit_type}`" in claude_md, f"CLAUDE.md does not list the commit type {commit_type!r}"


@pytest.mark.parametrize("document", ["CLAUDE.md", "README.md"])
def test_every_poe_command_the_documentation_mentions_is_a_task(pyproject, document):
    mentioned = set(re.findall(r"uv run poe ([a-z][a-z-]*)", (ROOT / document).read_text(encoding="utf-8")))
    assert mentioned, f"{document} should mention at least one poe task"
    assert mentioned <= pyproject["tool"]["poe"]["tasks"].keys()
