"""Builds the scratch vault the reel is recorded in.

    python make_vault.py <repo> <out> <gasp-corpus binary> [archive notes]

The vault is the repository's synthetic corpus (fixtures/corpus), a large
generated archive for the search shot, and the showcase notes in notes/.
Nothing here comes from a real person's vault. The vault is a git clone of
a local bare repository, so the app's sync has something to push to.
"""
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent

SETTINGS = """\
[appearance]
base-font-size = 17
theme = "light"

[prose.sentence-length]
enabled = false

[prose.grammar]
enabled = false

[sync]
auto = false
"""

# The caret red from the app icon, used as the accent.
THEME = """\
[color]
accent = "#c02b4a"
"""


def git(cwd: Path, *args: str) -> None:
    subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True)


def main() -> None:
    repo, out, corpus_bin = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
    archive_notes = int(sys.argv[4]) if len(sys.argv) > 4 else 5000
    vault = out / "Field Notes"
    remote = out / "remote.git"
    for path in (vault, remote):
        if path.exists():
            shutil.rmtree(path)
    shutil.copytree(repo / "fixtures" / "corpus", vault)
    subprocess.run(
        [corpus_bin, "--out", str(vault / "Archive"), "--notes", str(archive_notes), "--seed", "124"],
        check=True,
        capture_output=True,
    )
    for note in (HERE / "notes").glob("*.md"):
        shutil.copy(note, vault / note.name)
    config = vault / ".gasp"
    config.mkdir(exist_ok=True)
    (config / "settings.toml").write_text(SETTINGS)
    (config / "theme.toml").write_text(THEME)

    subprocess.run(["git", "init", "--bare", "-q", "-b", "master", str(remote)], check=True)
    git(vault, "init", "-q", "-b", "master")
    git(vault, "config", "gc.auto", "0")
    git(vault, "config", "user.name", "Reel")
    git(vault, "config", "user.email", "reel@example.com")
    git(vault, "add", "-A")
    git(vault, "commit", "-q", "-m", "Field notes")
    git(vault, "remote", "add", "origin", remote.as_uri())
    git(vault, "push", "-q", "-u", "origin", "master")
    count = sum(1 for path in vault.rglob("*.md") if ".git" not in path.parts)
    print(f"{vault}: {count} notes")


if __name__ == "__main__":
    main()
