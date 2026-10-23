"""tools/pre_push.py decides whether a push must pass the whole local gate first: pushes to main, and anything that
is not a branch (a release tag), do; any other branch is left to CI, so feature-branch pushes stay quick."""

import importlib.util
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]


def _load():
    spec = importlib.util.spec_from_file_location("pre_push", ROOT / "tools" / "pre_push.py")
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


pre_push = _load()


@pytest.mark.parametrize("ref", ["refs/heads/main", "refs/tags/v0.1.0", "refs/tags/v0.1.0rc1", "", "main"])
def test_main_tags_and_anything_unclear_must_pass_the_gate(ref):
    assert pre_push.must_run(ref)


@pytest.mark.parametrize("ref", ["refs/heads/feature/blur", "refs/heads/fix/windows-links", "refs/heads/main2"])
def test_other_branches_are_left_to_ci(ref):
    assert not pre_push.must_run(ref)


def test_a_feature_branch_push_skips_the_gate_and_succeeds(monkeypatch):
    monkeypatch.setenv("PRE_COMMIT_REMOTE_BRANCH", "refs/heads/feature/x")
    assert pre_push.main() == 0
