"""A throwaway git repository with a local bare `origin`, for the tests of the release tools.

The repository is isolated from the developer's git configuration (no global or system config, so no signing, hooks or
default branch of theirs leaks in), and the tools run with this environment's executables first on PATH, as under
`uv run`.
"""

import os
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]


class GitRepo:
    """A working repository on `main` whose `origin` is a bare repository next to it."""

    def __init__(self, base: Path) -> None:
        self.path = base / "work"
        self.origin = base / "origin.git"
        empty_config = base / "gitconfig"
        empty_config.write_text("", encoding="utf-8")
        self.env = {
            **os.environ,
            "GIT_CONFIG_GLOBAL": str(empty_config),
            "GIT_CONFIG_NOSYSTEM": "1",
            "PATH": f"{Path(sys.executable).parent}{os.pathsep}{os.environ.get('PATH', '')}",
            "UV_OFFLINE": "1",  # nothing here may need the network
            "NO_COLOR": "1",
        }
        self.path.mkdir()
        subprocess.run(["git", "init", "-q", "--bare", "-b", "main", str(self.origin)], check=True, env=self.env)
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.name", "Test Author")
        self.git("config", "user.email", "author@example.com")
        self.git("remote", "add", "origin", str(self.origin))

    def git(self, *args: str) -> str:
        """Run git in the working repository and return its output (stripped); fail the test if git fails."""
        result = subprocess.run(
            ["git", *args], cwd=self.path, env=self.env, capture_output=True, encoding="utf-8", check=False
        )
        assert result.returncode == 0, f"git {' '.join(args)}: {result.stderr}"
        return result.stdout.strip()

    def write(self, name: str, text: str) -> None:
        (self.path / name).write_text(text, encoding="utf-8")

    def read(self, name: str) -> str:
        return (self.path / name).read_text(encoding="utf-8")

    def commit(self, message: str, files: dict[str, str] | None = None) -> str:
        """Write `files` (default: one new file named after the commit count), commit everything, return the hash."""
        for name, text in (files or {f"file{self.git('rev-list', '--all', '--count')}.txt": message}).items():
            self.write(name, text)
        self.git("add", "--all")
        self.git("commit", "-q", "-m", message)
        return self.git("rev-parse", "HEAD")

    def push(self) -> None:
        self.git("push", "-q", "origin", "main")

    def run_tool(self, script: str, *args: str) -> subprocess.CompletedProcess:
        """Run `tools/<script>` from this checkout with the repository as its working directory."""
        return subprocess.run(
            [sys.executable, str(ROOT / "tools" / script), *args],
            cwd=self.path,
            env=self.env,
            capture_output=True,
            encoding="utf-8",
            check=False,
        )


@pytest.fixture
def git_repo(tmp_path) -> GitRepo:
    return GitRepo(tmp_path)
