#!/usr/bin/env python3
"""Delete the builds Cargo leaves behind in target/.

Cargo never removes an old build. Every change to the code, the flags or the
checkout's path compiles a crate under a new hash beside the old ones, so
target/ grows by gigabytes a day. This keeps the newest few builds of each
crate in every profile folder and deletes the rest. A build deleted by
mistake only costs a recompile: Cargo sees it's missing and builds it again.
"""
import pathlib
import re
import shutil
import sys
import time
from collections import defaultdict

TARGET = pathlib.Path(__file__).resolve().parent.parent / "target"
# A crate is current in up to about five builds at once: the library, its
# tests, and Clippy's check of each, plus a binary.
KEEP_PER_CRATE = 5
# Anything touched this recently stays, so a build running now is safe.
GRACE_SECONDS = 60 * 60
ARTIFACT = re.compile(r"^(?:lib)?(?P<crate>.+?)-(?P<hash>[0-9a-f]{16})(?:[.]|$)")


def profile_dirs():
    """Folders such as target/debug and target/x86_64-apple-darwin/dist."""
    return sorted({deps.parent for deps in TARGET.glob("**/deps") if deps.is_dir()})


def size_of(path):
    if path.is_dir():
        return sum(item.stat().st_size for item in path.rglob("*") if item.is_file())
    return path.stat().st_size


def builds_by_crate(folder):
    """{crate: {hash: [paths]}} for the artifacts directly in `folder`."""
    crates = defaultdict(lambda: defaultdict(list))
    for entry in folder.iterdir():
        match = ARTIFACT.match(entry.name)
        if match:
            crates[match["crate"]][match["hash"]].append(entry)
    return crates


def stale_paths(folder, now):
    for builds in builds_by_crate(folder).values():
        newest_first = sorted(
            builds.values(),
            key=lambda paths: max(path.lstat().st_mtime for path in paths),
            reverse=True,
        )
        for paths in newest_first[KEEP_PER_CRATE:]:
            if now - max(path.lstat().st_mtime for path in paths) > GRACE_SECONDS:
                yield from paths


def remove(path):
    if path.is_dir() and not path.is_symlink():
        shutil.rmtree(path, ignore_errors=True)
    else:
        path.unlink(missing_ok=True)


def main():
    if not TARGET.is_dir():
        return 0
    now = time.time()
    freed = 0
    for profile in profile_dirs():
        for folder in (profile / "deps", profile / "incremental"):
            if not folder.is_dir():
                continue
            for path in list(stale_paths(folder, now)):
                freed += size_of(path)
                remove(path)
    print(f"prune-target: freed {freed / 1e9:.1f} GB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
