"""The pre-push hook: run the whole local gate (`uv run poe check`) when the push updates `main` or anything that is
not a branch (a tag), and skip it for any other branch.

Why: a feature branch is checked by CI when its pull request opens, so running the full gate on every push to it only
slows the work down. A direct push to `main` has no pull request in front of it, so the gate runs first, locally,
instead of leaving a red `main` for CI to find. pre-commit gives the hook the pushed ref in PRE_COMMIT_REMOTE_BRANCH
(for example `refs/heads/main`); when it is missing or is not a branch ref the gate runs: skipping is only for a
branch that is clearly not main.
"""

import os
import shutil
import subprocess
import sys


def must_run(remote_ref: str) -> bool:
    """Whether a push to `remote_ref` (as pre-commit reports it) must pass the local gate first."""
    if not remote_ref.startswith("refs/heads/"):
        return True
    return remote_ref == "refs/heads/main"


def main() -> int:
    """Run the gate when the push needs it; return the exit status git should see."""
    remote_ref = os.environ.get("PRE_COMMIT_REMOTE_BRANCH", "")
    if not must_run(remote_ref):
        print(
            f"pre-push: {remote_ref} is not main, so the full gate is left to CI (run `uv run poe check` to see it now)"
        )
        return 0
    uv = shutil.which("uv")
    if uv is None:
        print("pre-push: uv is not on the PATH, so the gate cannot run; install uv (https://docs.astral.sh/uv/)")
        return 1
    return subprocess.run([uv, "run", "poe", "check"], check=False).returncode  # noqa: S603  fixed argv, no input


if __name__ == "__main__":
    sys.exit(main())
