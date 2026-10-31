"""Check that no code file grows without bound: big files hurt navigation, above all for agents that read in chunks.

Run by hand with `python tools/check_file_size.py`; the test suite runs it too. The budgets and the exemptions are in
pyproject.toml, `[tool.file-size]`, which says what is measured.

Interface:
    check(root) -> problems, one string per problem, naming the file and the fix; empty when every file is fine
    main(argv)  -> exit code: 0 clean, 1 problems found (listed on stderr)

A file within its budget is fine. A file over budget is a problem, unless the config records it as exempt: an exempt
file may shrink but not grow past its recorded size. An exemption that no longer applies (the file is gone, or is
within budget now) is a problem too, so the list cleans itself.
"""

import argparse
import sys
import tomllib
from pathlib import Path

# the code that is measured: (folder, file suffix). Everything else (docs, examples, generated, data) is not.
SCOPE = (("src", ".py"), ("tools", ".py"), ("tests", ".py"), ("crates", ".rs"))
TABLE = "[tool.file-size.exempt]"


def check(root: Path) -> list[str]:
    """Return what is wrong with the file sizes under `root`, using the budgets and exemptions in its pyproject.toml."""
    root = Path(root)
    config = tomllib.loads((root / "pyproject.toml").read_text(encoding="utf-8"))["tool"]["file-size"]
    budgets = {"source": config["source"], "test": config["tests"]}
    exempt: dict[str, int] = config["exempt"]
    sizes = _line_counts(root)
    problems = []
    for path, lines in sizes.items():
        kind = _kind(path)
        budget = budgets[kind]
        if path in exempt:
            if lines > exempt[path]:
                problems.append(
                    f"{path}: {lines} lines, grew past its recorded {exempt[path]} (the {kind} budget is {budget}). "
                    f"An exempt file may only shrink: move code out of it, split by concept."
                )
        elif lines > budget:
            problems.append(
                f"{path}: {lines} lines, over the {budget}-line {kind} budget. Split it by concept into smaller files."
            )
    for path in sorted(exempt):
        if path not in sizes:
            problems.append(f"{path}: listed as exempt but no such checked file exists; remove it from {TABLE}.")
        elif sizes[path] <= budgets[_kind(path)]:
            problems.append(
                f"{path}: {sizes[path]} lines, within the {budgets[_kind(path)]}-line {_kind(path)} budget; "
                f"remove it from {TABLE}."
            )
    return problems


def _kind(path: str) -> str:
    return "test" if "tests" in path.split("/") else "source"


def _line_counts(root: Path) -> dict[str, int]:
    """Lines of every measured file, keyed by its path relative to `root` with `/` separators, sorted by path."""
    found = {
        path.relative_to(root).as_posix(): len(path.read_text(encoding="utf-8").splitlines())
        for folder, suffix in SCOPE
        for path in (root / folder).rglob(f"*{suffix}")
    }
    return dict(sorted(found.items()))


def main(argv: list[str] | None = None) -> int:
    """Check the repository root given as `argv[0]` (default: the repository this script lives in)."""
    parser = argparse.ArgumentParser(description="Check that no code file is over its size budget.")
    parser.add_argument("root", nargs="?", type=Path, default=Path(__file__).resolve().parent.parent)
    problems = check(parser.parse_args(argv).root)
    if problems:
        print("file-size:\n" + "\n".join(f"  {problem}" for problem in problems), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
