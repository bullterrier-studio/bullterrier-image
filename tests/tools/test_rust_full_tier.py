"""A Rust test is ignored only to move it to the `full` tier: `poe check` skips it, `poe rust-test-full` and CI run it.

An architecture test: its subject is the Rust test sources, so it reads their text. Every `#[ignore]` in `crates/` must
give a reason that starts `full:`, so a test cannot be switched off for another reason without anyone noticing.
"""

import re
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
IGNORE = re.compile(r"#\[ignore(?P<reason>\s*=\s*\"[^\"]*\")?\s*\]")


def ignored_for_another_reason(source: str) -> list[str]:
    """The `#[ignore]` attributes of `source` whose reason does not start `full:` (a missing reason is one)."""
    return [
        found.group(0)
        for found in IGNORE.finditer(source)
        if not (found["reason"] or "").split("=", 1)[-1].strip().startswith('"full:')
    ]


@pytest.mark.parametrize(
    ("source", "offences"),
    [
        ('#[ignore = "full: every size"]', 0),
        ("#[test]\n#[ignore]\nfn t() {}", 1),
        ('#[ignore = "flaky on CI"]', 1),
        ('#[ignore = "slow, full: later"]', 1),
        ("fn ignore_me() {} // not an attribute", 0),
    ],
    ids=["a full reason", "no reason", "another reason", "full: not first", "no attribute"],
)
def test_only_a_reason_that_starts_full_is_accepted(source, offences):
    assert len(ignored_for_another_reason(source)) == offences


def test_every_ignored_rust_test_is_a_full_tier_test():
    offences = {
        str(path.relative_to(ROOT)): found
        for path in sorted((ROOT / "crates").rglob("*.rs"))
        if (found := ignored_for_another_reason(path.read_text(encoding="utf-8")))
    }
    assert not offences
