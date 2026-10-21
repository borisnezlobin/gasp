# Build folders

Build folders must never balloon. `scripts/prune-target.py --cap 15` holds Cargo's folders under 15 GB, and `.claude/settings.json` runs it after every turn and every agent; keep those hooks.

- After cargo or xcodebuild work, run `python3 scripts/prune-target.py --cap 15` before reporting.
- Run at most one Rust-building worktree agent at a time. Cargo hashes the source path into build names, so every worktree builds its own copy of every crate. Remove a worktree as soon as its branch is merged.
- Point xcodebuild checks at a `-derivedDataPath` inside `target/` and delete it afterwards.
- When cleaning up a throwaway file, delete that file, never the folder it sits in.
