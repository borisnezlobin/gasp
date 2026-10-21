#!/usr/bin/env python3
"""Delete the builds Cargo leaves behind in target/.

Cargo never removes an old build. Every change to the code, the flags or the
checkout's path compiles a crate under a new hash beside the old ones, so
target/ grows by gigabytes a day. This keeps the newest few builds of each
crate in every profile folder and deletes the rest. A build deleted by
mistake only costs a recompile: Cargo sees it's missing and builds it again.

With `--cap GB` it also holds Cargo's folders under a hard limit, in steps
that stop as soon as they're under it: every incremental cache, then all but
the newest build of each crate, then whole profile folders, but never a release
build made in the last day, since packaging uses it. It also deletes
build folders agents left in worktrees and old Xcode builds of Gasp. The cap
steps wait while a cargo, rustc or xcodebuild process runs, so they never
pull files from under a build; the next run catches up.
"""
import argparse
import pathlib
import re
import shutil
import subprocess
import sys
import time
from collections import defaultdict

REPO = pathlib.Path(__file__).resolve().parent.parent
TARGET = REPO / "target"
# A crate is current in up to about five builds at once: the library, its
# tests, and Clippy's check of each, plus a binary.
KEEP_PER_CRATE = 5
# Anything touched this recently stays, so a build running now is safe.
GRACE_SECONDS = 60 * 60
ARTIFACT = re.compile(r"^(?:lib)?(?P<crate>.+?)-(?P<hash>[0-9a-f]{16})(?:[.]|$)")
BUILD_PROCESSES = ("cargo", "rustc", "xcodebuild")
STRAY_BUILD_FOLDERS = (
    ".claude/worktrees/*/target",
    ".claude/worktrees/*/target-*",
    "target-*",
    "target/ios-sim-check",
)
XCODE_BUILDS = pathlib.Path.home() / "Library/Developer/Xcode/DerivedData"
# Release builds feed `make dmg` and uploads, so the cap leaves one alone
# for a day after it was last built.
RELEASE_PROFILES = ("dist", "release")
RELEASE_KEEP_SECONDS = 24 * 60 * 60


def profile_dirs():
    """Folders such as target/debug and target/x86_64-apple-darwin/dist."""
    return sorted({deps.parent for deps in TARGET.glob("**/deps") if deps.is_dir()})


def size_of(path):
    if path.is_symlink() or not path.exists():
        return 0
    if path.is_dir():
        return sum(item.lstat().st_size for item in path.rglob("*") if item.is_file())
    return path.lstat().st_size


def builds_by_crate(folder):
    """{crate: {hash: [paths]}} for the artifacts directly in `folder`."""
    crates = defaultdict(lambda: defaultdict(list))
    for entry in folder.iterdir():
        match = ARTIFACT.match(entry.name)
        if match:
            crates[match["crate"]][match["hash"]].append(entry)
    return crates


def newest_touch(paths):
    return max(path.lstat().st_mtime for path in paths)


def stale_paths(folder, now, keep, grace):
    for builds in builds_by_crate(folder).values():
        newest_first = sorted(builds.values(), key=newest_touch, reverse=True)
        for paths in newest_first[keep:]:
            if now - newest_touch(paths) > grace:
                yield from paths


def remove(path):
    freed = size_of(path)
    if path.is_dir() and not path.is_symlink():
        shutil.rmtree(path, ignore_errors=True)
    else:
        path.unlink(missing_ok=True)
    return freed


def prune_profiles(now, keep, grace):
    freed = 0
    for profile in profile_dirs():
        for folder in (profile / "deps", profile / "incremental"):
            if folder.is_dir():
                freed += sum(remove(path) for path in list(stale_paths(folder, now, keep, grace)))
    return freed


def cargo_size():
    return sum(size_of(profile) for profile in profile_dirs())


def is_fresh_release(profile, now):
    if profile.name not in RELEASE_PROFILES:
        return False
    return now - profile.lstat().st_mtime < RELEASE_KEEP_SECONDS


def cappable_profiles(now):
    """Every profile folder but a release build made in the last day."""
    return [profile for profile in profile_dirs() if not is_fresh_release(profile, now)]


def a_build_is_running():
    for name in BUILD_PROCESSES:
        found = subprocess.run(["pgrep", "-x", name], capture_output=True, check=False)
        if found.returncode == 0:
            return True
    return False


def drop_incremental_caches(now):
    return sum(remove(profile / "incremental") for profile in cappable_profiles(now))


def drop_old_builds(now):
    freed = 0
    for profile in cappable_profiles(now):
        deps = profile / "deps"
        if deps.is_dir():
            freed += sum(remove(path) for path in list(stale_paths(deps, now, 1, 0)))
    return freed


def drop_profiles(now):
    return sum(remove(profile) for profile in cappable_profiles(now))


def drop_stray_build_folders():
    freed = 0
    for pattern in STRAY_BUILD_FOLDERS:
        freed += sum(remove(path) for path in REPO.glob(pattern) if path.is_dir())
    return freed


def drop_old_xcode_builds(now):
    if not XCODE_BUILDS.is_dir():
        return 0
    old = [
        path
        for path in XCODE_BUILDS.glob("Gasp-*")
        if now - path.lstat().st_mtime > GRACE_SECONDS
    ]
    return sum(remove(path) for path in old)


def enforce_cap(cap_bytes, now):
    """The cap's steps, mildest first, until Cargo's folders fit."""
    freed = drop_stray_build_folders() + drop_old_xcode_builds(now)
    for step in (drop_incremental_caches, drop_old_builds, drop_profiles):
        if cargo_size() <= cap_bytes:
            break
        freed += step(now)
    return freed


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--cap", type=float, metavar="GB", help="hard limit for Cargo's folders")
    args = parser.parse_args()
    now = time.time()
    freed = prune_profiles(now, KEEP_PER_CRATE, GRACE_SECONDS) if TARGET.is_dir() else 0
    capped = args.cap is not None
    if capped and a_build_is_running():
        print("prune-target: a build is running, so the cap waits for the next run")
    elif capped:
        freed += enforce_cap(args.cap * 1e9, now)
    print(f"prune-target: freed {freed / 1e9:.1f} GB, Cargo's folders hold {cargo_size() / 1e9:.1f} GB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
