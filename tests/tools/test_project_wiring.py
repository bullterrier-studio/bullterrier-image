"""The project's tooling is wired the way CLAUDE.md promises: poe tasks, commit-message settings, hook wiring."""

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
REMOTE_HOOK_IDS = {
    "check-toml",
    "check-yaml",
    "check-merge-conflict",
    "check-case-conflict",
    "check-added-large-files",
    "detect-private-key",
}
LOCAL_HOOK_IDS = {"ruff-format", "ruff-check", "commit-message", "tests", "mypy"}
# what each local hook prints on failure (pre-commit shows the hook name): the command that fixes or reproduces it
FIX_COMMANDS = {
    "ruff-format": "uv run poe fix",
    "ruff-check": "uv run poe fix",
    "commit-message": "uv run python tools/check_commit_message.py",
    "tests": "uv run poe test",
    "mypy": "uv run mypy",
}


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


def test_poe_tasks_exist_and_the_gate_runs_format_lint_types_tests_in_order(pyproject):
    tasks = pyproject["tool"]["poe"]["tasks"]
    assert {"check", "fix", "test", "setup"} <= tasks.keys()
    assert poe_commands(tasks["test"]) == ["pytest -q"]
    assert poe_commands(tasks["fix"]) == ["ruff check --fix .", "ruff format ."]
    # the test command is defined once (task `test`) and the gate refers to it
    assert tasks["check"]["sequence"] == ["ruff format --check .", "ruff check .", "mypy", {"ref": "test"}]
    assert poe_commands(tasks["setup"]) == [
        "pre-commit install --install-hooks",
        "git config --local commit.template .gitmessage",
    ]


@pytest.mark.parametrize(
    ("task", "commands"),
    [
        ("test", ["pytest -q"]),
        ("fix", ["ruff check --fix .", "ruff format ."]),
        ("check", ["ruff format --check .", "ruff check .", "mypy", "pytest -q"]),
        ("setup", ["pre-commit install --install-hooks", "git config --local commit.template .gitmessage"]),
    ],
)
def test_poe_accepts_each_task_and_would_run_exactly_these_commands_in_order(task, commands):
    # poe's own dry run: it rejects a task that is wrongly declared (a bare string in a sequence is a task name, not a
    # command), and prints each command it would run, in order, without running it
    result = subprocess.run(
        [sys.executable, "-m", "poethepoet", "--dry-run", task], cwd=ROOT, capture_output=True, text=True, check=False
    )
    assert result.returncode == 0, result.stderr
    output = result.stdout + result.stderr
    printed = [line.removeprefix("Poe => ") for line in output.splitlines() if line.startswith("Poe => ")]
    assert printed == commands, output


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
    return yaml.safe_load((ROOT / ".pre-commit-config.yaml").read_text())


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


def test_the_hook_ids_are_exactly_the_documented_ones(precommit):
    assert set(hooks_by_id(precommit, "remote")) == REMOTE_HOOK_IDS
    assert set(hooks_by_id(precommit, "local")) == LOCAL_HOOK_IDS


def test_the_remote_hooks_come_from_one_repository_pinned_to_a_release_tag(precommit):
    [remote] = [repo for repo in precommit["repos"] if repo["repo"] != "local"]
    assert remote["repo"] == "https://github.com/pre-commit/pre-commit-hooks"
    assert re.fullmatch(r"v\d+\.\d+\.\d+", remote["rev"]), remote["rev"]  # a tag, never a branch such as main


def test_the_large_file_limit_admits_the_gallery_pictures_but_not_much_more(precommit):
    assert hooks_by_id(precommit, "remote")["check-added-large-files"]["args"] == ["--maxkb=1000"]


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


def test_hooks_run_at_the_documented_stages(precommit):
    # a hook with no `stages` would otherwise also run on commit-msg and push, where its files are meaningless or stale
    assert precommit["default_stages"] == ["pre-commit"]
    for hook_id, hook in hooks_by_id(precommit, "remote").items():
        assert "stages" not in hook, hook_id
    local = hooks_by_id(precommit, "local")
    assert local["commit-message"]["stages"] == ["commit-msg"]
    # the whole-repository checks run on push and in `poe check`, not on every commit: commits stay fast
    assert local["tests"]["stages"] == ["pre-push"]
    assert local["tests"]["entry"] == "uv run pytest -q"
    assert local["mypy"]["stages"] == ["pre-push"]
    assert local["mypy"]["entry"] == "uv run mypy"
    for hook_id in ("ruff-format", "ruff-check"):
        assert "stages" not in local[hook_id]  # default stage: pre-commit


@pytest.mark.parametrize("hook_id", ["tests", "mypy"])
def test_whole_repository_hooks_run_on_every_push_over_everything(precommit, hook_id):
    # pass_filenames: false, or pre-commit would hand over only the pushed files and the check would see a part;
    # always_run: true, or a push that touches no Python file would skip it
    hook = hooks_by_id(precommit, "local")[hook_id]
    assert hook["pass_filenames"] is False
    assert hook["always_run"] is True


def test_every_local_hook_failure_says_how_to_fix_or_reproduce_it(precommit):
    local = hooks_by_id(precommit, "local")
    assert set(FIX_COMMANDS) == set(local)  # a new hook must say its command too
    for hook_id, command in FIX_COMMANDS.items():
        assert f"`{command}" in local[hook_id]["name"], hook_id  # pre-commit prints the name on the failure line


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
    claude_md = (ROOT / "CLAUDE.md").read_text()
    for commit_type in pyproject["tool"]["commit-message"]["types"]:
        assert f"`{commit_type}`" in claude_md, f"CLAUDE.md does not list the commit type {commit_type!r}"


@pytest.mark.parametrize("document", ["CLAUDE.md", "README.md"])
def test_every_poe_command_the_documentation_mentions_is_a_task(pyproject, document):
    mentioned = set(re.findall(r"uv run poe ([a-z][a-z-]*)", (ROOT / document).read_text()))
    assert mentioned, f"{document} should mention at least one poe task"
    assert mentioned <= pyproject["tool"]["poe"]["tasks"].keys()
