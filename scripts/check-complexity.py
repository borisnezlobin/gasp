#!/usr/bin/env python3
"""Fail when any Rust function's cyclomatic complexity is over the limit.

rust-code-analysis-cli reports a function's metrics with its nested closures and
inner functions folded in, so each function's own score is its total minus the
totals of the spaces nested inside it.
"""
import json
import pathlib
import subprocess
import sys

LIMIT = 15
ROOTS = ["crates", "apps", "tools"]


def rust_files():
    for root in ROOTS:
        for path in sorted(pathlib.Path(root).rglob("*.rs")):
            if "target" not in path.parts:
                yield path


def metrics_for(path):
    output = subprocess.run(
        ["rust-code-analysis-cli", "-m", "-O", "json", "-p", str(path)],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return json.loads(output) if output.strip() else None


def own_cyclomatic(space):
    total = space["metrics"]["cyclomatic"]["sum"]
    nested = sum(child["metrics"]["cyclomatic"]["sum"] for child in space["spaces"])
    return total - nested


def over_limit(space, path):
    if space["kind"] == "function" and own_cyclomatic(space) > LIMIT:
        yield f"{path}:{space['start_line']} {space['name']} has cyclomatic complexity {own_cyclomatic(space):.0f} (limit {LIMIT})"
    for child in space["spaces"]:
        yield from over_limit(child, path)


def main():
    failures = []
    for path in rust_files():
        unit = metrics_for(path)
        if unit:
            failures.extend(over_limit(unit, path))
    for failure in failures:
        print(failure)
    if failures:
        sys.exit(1)
    print("cyclomatic complexity: every function is at or under", LIMIT)


if __name__ == "__main__":
    main()
