# Gasp plan

Gasp is a native, very fast Markdown editor for your Obsidian vault. It runs on macOS, Windows, Linux and iPhone, syncs through GitHub, and can be customized at every level, including behaviour. The binary is `gasp` and each vault's config folder is `.gasp/`. The crates are still named `editor-*` from before the app had a name.

## Start here

This section is for an agent picking the project up with no earlier context. Everything below it is the design.

### Where things stand

The workspace, CI and the synthetic corpus exist, the Linux-runnable Phase 0 spikes have results in [the spike table](#phase-0-spikes), the core is built and tested, and the desktop app covers most of Phases 2 to 5. This repo used to hold an abandoned Electron editor, which is still in the git history and should be ignored.

| Phase | Status |
|---|---|
| 0 Spikes | math, git, Typst PDF and GPUI run on Linux; the iPhone spikes need a macOS runner |
| 1 Core | built: document, input pipeline, parser with Obsidian extensions, render planner, snippets, replacements, footnotes |
| 2 Desktop editor | built: live preview, tabs and splits with drag to split, file tree, both sidebars (backlinks, outgoing links, outline, tags), palette, switcher, settings with editable shortcuts, toolbars from `toolbars.toml` (the status bar is one) with a settings page, light and dark themes, hover previews, emoji picker, link cards, code blocks, daily notes, templates, file recovery, edit time. Not yet built on macOS by CI's Metal step; the title bar and Keychain code are untested on a Mac |
| 3 Sync and travel check | sync built in the app (status bar, popover, settings page, conflict resolver) and syncing the owner's vault on `master`, with `main` merged in one way; a conflict waits in the resolver while every other note keeps syncing; travel check not started |
| 4 Search and prose | vault search with an in-memory index (not Tantivy yet), sentence-length highlighting, grammar layers 1 and 2 (Harper's mechanical checks and vault-learned spelling); the local grammar model and OCR aren't started |
| 5 Export | PDF (Typst) and HTML for the website built; the website still needs `crates/export/assets/article.css` and its drop-cap script updated |
| 6 MCP and headless modes | MCP server built (`gasp mcp`): note, attachment, link, config and render tools, plus a bridge to the running app for its state, commands and unsaved notes. `gasp --snapshot` draws a note's editor, or the whole window driven by a script of clicks, keys and commands on a copy of a vault, to PNGs with no window shown (macOS). The other headless modes aren't started |
| 7 iPhone | built as a browser on the core (UniFFI, SwiftUI, TextKit 2): tabs with an overview, an edge-swipe sidebar (search, files, outline, links, tags), live preview from the core's planner with math rendered by Typst, vault images, link cards with their image and coloured code, wide tables as a sideways-scrolling grid with cell editing, heading and callout folding, the grammar checker's underlines and cards, footnote cards, OCR of images and PDFs in search, reading positions kept per note, the keyboard bar and the bottom bar (the `keyboard` and `browser-bar` toolbars in `toolbars.toml`, which the phone's settings change too), undo and redo in a pill above the keyboard, find and replace, and every registry command but panes through the palette, the bar or hardware keys (`UIKeyCommand`). Syncs with GitHub like the desktop (setup, the tab bar indicator, the resolver, sync settings; on open, foreground, background, a background refresh and after edits), verified against local repositories. Runs in the simulator; not yet run on the owner's iPhone |
| 8 Plugins and agents | not started |

Update this table, and the phase's section, when work lands.

### What you can and can't reach

A cloud session is probably a Linux container with this repo cloned. It can't see the owner's Mac.

You can reach these:

- **This repo** (`borisnezlobin/gasp`), where all the code goes.
- **[Flo State](https://github.com/Altimor/flo-state)** is the native Swift editor that inspired this project. Read its `oracle/`, `fixtures/` and `Sources/FloCore/Render/RenderPlanner.swift` for the parity-testing and render-planner ideas. It's GPLv3, so read it for ideas and don't copy its code.
- **[PDF Export Plus](https://github.com/borisnezlobin/obsidian-pdf-export-plus)** is the owner's PDF plugin, which Phase 5 ports to Typst.
- **[The owner's website](https://github.com/borisnezlobin/website)**. `scripts/publish-article.mjs` and `app/styles/` describe the current HTML export flow that Phase 5 replaces.
- **Library docs and source** for GPUI, Typst, mitex, harper-core, Tantivy, git2, rquickjs and UniFFI.

- **`reference/`** in this repo holds material copied from the owner's Mac:
  - `reference/obsidian/` has the owner's Obsidian app settings, hotkeys and plugin settings, including all 212 Latex Suite snippets (`plugins/obsidian-latex-suite.json`) and the prettifier table. The migrator's tests run against these real files.
  - `reference/footnotes-plus/` has the source and tests of the owner's Footnotes Plus plugin, which is the spec for [Footnotes](#footnotes).

These stay out of reach:

- **The notes vault and the private `borisnezlobin/notes` repo.** Never clone, read or push to it, and never commit note content anywhere. Sync work uses local bare repos in tests, plus a throwaway repo the owner provides for network tests.
- **`vault-sync`** exists only on the owner's Mac. Its behaviour is described in [Sync](#sync), and that description is the spec.
- **Obsidian itself**, which the parity oracle drives. Try running it on a macOS GitHub Actions runner against the synthetic corpus. If that doesn't work, the owner runs the oracle locally.

Steps marked **[Mac]** need the owner's machine or a macOS runner.

### Building and running

```
cargo run -p gasp-desktop -- <vault folder>     # the desktop app
cargo run -p gasp-desktop -- mcp <vault folder> # the MCP server on stdio
cargo run -p gasp-desktop -- --snapshot <note> <out.png> [--width N --height N --theme light|dark --cursor LINE:COL]
                                                  # the note's editor drawn to a PNG, no window shown
make snapshot SCRIPT=steps.txt VAULT=<vault> [OUT=dir] [OPEN=note]
                                                  # the whole window, driven by a script, to PNGs
cargo test --workspace                            # every crate's tests
python3 scripts/check-complexity.py               # the complexity limit
```

On Linux, GPUI needs `libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev libx11-xcb-dev libxcb1-dev libfontconfig-dev libfreetype-dev libssl-dev clang` (the CI workflow installs the same list).

On macOS, GPUI compiles its Metal shaders at build time, so it needs Apple's Metal Toolchain (Xcode, then `xcodebuild -downloadComponent MetalToolchain` if the build asks for it). Without it, build with `cargo run -p gasp-desktop --features runtime-shaders`, which compiles the shaders when the app starts instead.

The iPhone app needs a Mac with Xcode and XcodeGen (`brew install xcodegen`):

```
apps/ios/scripts/build-core.sh          # the Rust core for iPhone and simulator, with Swift bindings
cd apps/ios && xcodegen generate        # Gasp.xcodeproj from project.yml
apps/ios/scripts/run-simulator.sh       # build, install and launch on a simulator, headless
open -a Simulator                       # to watch it
```

`build-core.sh` runs again after any change to the Rust code (`--debug` builds faster and runs slower). The core, the bindings and the Xcode project are generated, so git ignores them. `xcrun simctl launch <device> com.borisnezlobin.gasp -open 'gasp://open?path=Summary.md&line=3' -run palette.open` opens a note with the cursor on a line and runs a command, which is how the screenshots are taken: `xcrun simctl io <device> screenshot shot.png`.

To run it on your iPhone: plug it in, turn on Developer Mode (Settings › Privacy & Security), open `apps/ios/Gasp.xcodeproj`, pick the phone as the destination and press Run. The project already uses team `K2MB68Z582` with automatic signing and the bundle ID `com.borisnezlobin.gasp`; the first time, Xcode registers the phone with the team, and the phone asks you to trust the developer in Settings › General › VPN & Device Management.

To sync your notes on the phone: make a fine-grained token at github.com/settings/personal-access-tokens/new limited to `borisnezlobin/notes`, with read and write access to Contents. On the phone, tap Set up sync on the start page, enter `borisnezlobin/notes`, keep the branch `master`, paste the token and tap Clone notes. The legacy branch is `main` by default, so what the laptop pushes there keeps merging in. Once a sync each way has worked, delete GitSync's Shortcuts automations so only one thing syncs the phone.

### First tasks

These three are done. They're kept here as a record of how the project started.

1. **Workspace and CI.** Create the Cargo workspace from [Repository layout](#repository-layout) and a GitHub Actions workflow that builds, tests and lints on `ubuntu-latest`, `windows-latest` and `macos-latest`. It's done when an empty crate passes on all three.
2. **Synthetic corpus.** Write a generator for a fake vault that uses every feature in [What your vault actually uses](#what-your-vault-actually-uses), in similar proportions. Math-heavy notes, callouts, footnotes, tables, tasks and raw HTML matter most. Every committed test fixture comes from this corpus.
3. **Phase 0 spikes**, each ideally on its own branch with a short result written into the spike table: what passed, the measured numbers, and the decision. The GPUI, math, Typst and git2 spikes can run on Linux and CI. The iPhone spikes run on a macOS runner.

### Working rules

- **Complexity.** A cyclomatic complexity limit of 15 per function is a hard requirement, treated like a failing test. Enforce it in CI with Clippy's `cognitive_complexity` lint (threshold 15, set to deny) plus a cyclomatic check using `rust-code-analysis-cli`, and use SwiftLint's `cyclomatic_complexity` rule at 15 on the iPhone app. Split functions, use early returns and lookup tables rather than raising the limit.
- **Code style.** Names should explain the code. Comments are only for genuinely tricky logic.
- **Files.** Don't write summary Markdown files or backup copies of files. Git is the history.
- **Private data.** Never commit anything from the owner's notes. Fixtures recorded from the real vault stay on the Mac. The repo is private now but will be made public later, so never commit secrets, tokens, or absolute paths from the owner's machine either.
- **Git.** Work on a branch per spike or phase and open a pull request to `master`. Never force-push `master`. End commit messages with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- **Decisions.** Anything in [Decisions](#decisions) or [Assumptions](#assumptions) is settled. If a spike shows one is wrong, say so in the pull request instead of silently changing course.
- **UI.** The owner's design rules apply to every screen:
  - Icons come from the Phosphor set, and emoji are never used as icons.
  - Text is never all-caps unless the content really is uppercase (acronyms, codes), and letter-spacing is never changed.
  - Monospace is only for actual code, paths and aligned figures.
  - No gradient text, and no visible coloured border on a rounded element (use a fill, shadow or ring instead).
  - Every visual value comes from a theme token defined once.
  - Every state (selected, disabled, syncing, conflict) gets a visible change, not a text label.
  - Copy is short, plain and conversational.

## Goals

1. **Speed.** Typing, launching, searching and switching notes should feel instant. Idle CPU should be zero.
2. **Customization of everything.** Appearance, layout, behaviour and the editor's own editing logic are all plain files you can edit, and the built-in features use the same hooks you do.
3. **Your workflow as first-class features.** Footnotes, PDF export, HTML export to your site, math snippets, search, sentence-length highlighting, grammar, edit-time tracking and MCP are built in.
4. **One vault on every device.** The Mac, the Windows/Ubuntu travel laptop and the iPhone all share one vault, synced through `borisnezlobin/notes`, with offline editing and a conflict resolver that doesn't get in the way.
5. **Agent-extensible.** A coding agent can see the running app, change its config, write plugins and verify its own work.

Non-goals for v1 are Android, Obsidian plugin compatibility, canvas, Bases, Excalidraw and graph view.

## What your vault actually uses

I measured this from `~/Documents/Obsidian/Vault` so the feature list is driven by real usage. The vault has 204 notes.

| Feature | Uses | Notes |
|---|---:|---:|
| Inline math `$…$` | 3,660 | 50 |
| Block math `$$` delimiters | 748 | 28 |
| Raw HTML tags (mostly `<br>` 326 and `<hr>` 312) | 638+ | 25 |
| Embeds `![[…]]` (mostly images) | 238 | 47 |
| Task list items | 243 | 14 |
| Table rows | 203 | 17 |
| Markdown links `[..](..)` | 187 | 52 |
| Code fence lines | 107 | 19 |
| Callouts | 91 | 29 |
| Frontmatter blocks (`updated`, `edited_seconds` from Chronotyper) | 55 | 55 |
| Footnote references and definitions | 54 and 44 | 19 |
| `%%` comments | 17 | 7 |
| Tags | 10 | 5 |
| `==highlights==` | 7 | 7 |
| Wikilinks `[[…]]` | 1 | 1 |

Mermaid, Dataview, canvas and `.base` files have zero uses, and there is one Excalidraw drawing.

Obsidian core features you have turned on and that v1 keeps: the file explorer, quick switcher, command palette, hover page preview, and file recovery (local snapshots). App settings that carry over are: attachments go to `./images` next to the note, renaming a file updates links to it, deleted files go to the system trash, and the inline title is shown.

## Architecture

```
┌──────────────────────────────── Rust core (all platforms) ────────────────────────────────┐
│ document model + transactions + undo      parser (CommonMark, GFM, Obsidian extensions)    │
│ input pipeline (snippets, replacements…)  render planner (styled runs + widgets per line)  │
│ config + rules engine + command registry  sync (git)     search (Tantivy)     prose       │
│ math (LaTeX → Typst)   export (HTML, PDF via Typst)   MCP server   plugin runtime (QuickJS)│
└───────────────────────────────┬──────────────────────────────────────────┬────────────────┘
                                │ Rust API                                 │ UniFFI bindings
                 ┌──────────────┴──────────────┐              ┌────────────┴────────────┐
                 │ Desktop app (GPUI)          │              │ iPhone app              │
                 │ macOS · Windows · Linux     │              │ SwiftUI + TextKit 2     │
                 └─────────────────────────────┘              └─────────────────────────┘
```

The core owns every decision about what the document means and how it should look. The two apps only draw what the core's render planner hands them and forward input back. This split is what lets a customization, a rule or a plugin work identically on all four platforms. Flo State uses the same split (its `FloCore` render planner is platform-independent), and it's the part of that project most worth copying.

### Why these choices

- **Rust for the core** runs on all four platforms, has the libraries we need (Harper, Tantivy, Typst, libgit2, QuickJS), and has no garbage-collector pauses while typing.
- **GPUI for desktop** is Zed's UI framework. It renders on the GPU and supports macOS, Windows and Linux (X11 and Wayland) from one codebase. It's pre-1.0, so we pin a version. It gives us fast text shaping and drawing, and we write the live-preview editor component ourselves: line layout with mixed font sizes, inline widgets, selection, hit testing and IME. That's the largest single piece of work in the project, and it also gives us full control over layout.
- **Swift with TextKit 2 for iPhone** gets Apple's text input for free: the keyboard, autocorrect, dictation and the selection handles. GPUI has no iOS support, and rebuilding iOS text input is where cross-platform editors usually fall apart.
- **UniFFI** generates the Swift bindings to the Rust core.

### Repository layout

```
crates/
  core/       document, transactions, undo, parser, input pipeline, render planner
  config/     file loading, schemas, hot reload, rules engine, command registry
  sync/       git engine, merge policy, conflict model
  search/     index, query, excerpts, OCR ingestion
  vault/      link index, links that follow moves, file operations, atomic writes
  prose/      sentence segmentation, sentence-length highlighting, grammar (Harper + second pass)
  snippets/   snippet engine, Latex Suite migrator
  math/       LaTeX → Typst conversion and layout for the editor
  export/     HTML (article body + publish), PDF (Typst)
  mcp/        MCP server, and both ends of its bridge to the running app
  plugins/    QuickJS runtime and the TypeScript API
  ffi/        UniFFI surface for Swift
apps/
  desktop/    GPUI app, plus headless CLI modes for tests and agents
  ios/        Xcode project
tools/
  oracle/     drives real Obsidian over the Chrome DevTools Protocol to record parity fixtures
  migrate/    one-shot importer for your .obsidian settings, hotkeys and snippets
```

## Customization model

Everything the app does is described by files in the vault's config folder, `.gasp/`. They sync with your notes, reload the moment they change, and are what the settings screen reads and writes. Settings that belong to one device, such as window size and open tabs, live in a separate file that doesn't sync.

Before the app was called Gasp, the folder was `.editor/`. The desktop app, `gasp mcp` and the phone rename a vault's `.editor/` to `.gasp/` when they open it, so sync commits it as a rename; if both exist, `.gasp/` is used and `.editor/` is left alone. The desktop app also moves its own `editor` folder in the platform's config and data folders to `Gasp` (`gasp` on Linux), and a sync token stored under the old `editor-sync` Keychain name moves to `gasp-sync` the first time it's read. `.editor/device.toml` stays in the default device-only list for one release, for devices that haven't moved yet.

There are four levels, and each covers what the one above can't.

### 1. Settings

Settings are typed options with a schema, so the settings screen is generated from them and never drifts from the files.

```toml
[sidebar.files]
reveal = "hover"      # "always" | "toggle" | "hover"
mode   = "overlay"    # slides over the text instead of pushing it
```

### 2. Theme tokens and layout

Every colour, font, size, spacing value, radius, shadow and animation curve anywhere in the app is a named token. Components never hard-code a value. Layout is a tree that says which component fills each slot (sidebars, tab bar, editor, status bar, toolbar) and how the slots are arranged. Your current look becomes the default theme: Charter for text and UI, Courier New for code, a black accent, light mode, and no ribbon.

### 3. Rules

A rule binds any event to any command. Keys are only one kind of event. Pointer enter and leave, focus, typing, idle time, opening a file and window size are all events too, and a rule can be limited to certain platforms.

```toml
[[rule]]
on   = "pointer.enter"
at   = "window.left-edge"
do   = "sidebar.files.show"
platform = "desktop"

[[rule]]
on    = "pointer.leave"
at    = "sidebar.files"
after = "300ms"
do    = "sidebar.files.hide"
```

Two things make this work:

- **Every action is a named command.** That includes UI actions (show, hide, focus, resize a panel) and editor actions (move the cursor, insert a footnote, run a snippet).
- **Built-in behaviour is written as rules in a defaults file.** Nothing is hard-coded, so any default can be edited or deleted.

Commands can be wrapped with `before`, `after` or `instead`, so "Paste" can turn a URL into a link card, or "Save" can format first.

### 4. The input pipeline

Every keystroke becomes an edit request that passes through ordered, named steps before it touches the document:

```
keystroke → snippets → replacements → emoji → footnotes → list continuation → auto-pair → apply
```

Each built-in feature is one step. You can disable, reorder, replace or add steps, and every step knows its context (text, math, code, link, frontmatter, table), so "never in math" or "only in code" is one line of config.

### 5. Plugins

When rules aren't enough, a TypeScript plugin uses the same commands, events and pipeline steps. Plugins run in an embedded QuickJS engine, which works on iOS (Apple doesn't allow apps to compile code at runtime). Each plugin declares the permissions it wants, such as network access or writing outside the current note, and the app shows those before enabling it. A plugin can also replace a whole component by pointing a layout slot at its own implementation.

### Toolbars

A toolbar is data too: a `[toolbar.<id>]` table in `.gasp/toolbars.toml`, layered on the built-in `crates/config/defaults/toolbars.toml` the way settings are. A vault's table changes only the fields it names on a built-in toolbar, a new id adds a toolbar, and `enabled = false` turns one off. The built-in file has four: `status` (the status bar), `selection` (bold, italic, highlight, link and code, floating over selected text), `keyboard` (the iPhone's bar above the keyboard) and `browser-bar` (the iPhone's bar at the bottom of the screen).

```toml
[toolbar.status]                 # the built-in status bar, with Export as HTML at its left
items = ["export.html", "spacer", "word-count", "character-count", "reading-time", "edit-time", "cursor-position", "sync"]

[toolbar.writing]                # a new bar under the notes
title     = "Writing"
place     = "editor-bottom"
behaviour = "hide-while-typing"
style     = "icons-and-labels"
density   = "compact"
items     = ["format.bold", "format.italic", "separator", "format.bullet-list", "format.numbered-list", "edit.indent", "spacer", "menu:insert"]

[toolbar.selection]
enabled = false                  # no bar by the selection

[menu.insert]                    # a dropdown any bar can hold as "menu:insert"
title = "Insert"
icon  = "plus"
items = ["table.insert", "format.callout", "footnote.insert-or-jump"]

[timing]
hover-delay  = "150ms"           # before a bar shown on hover appears
hide-delay   = "400ms"           # before it goes once the pointer leaves
typing-pause = "1s"              # before a bar hidden while typing comes back
```

- **Place**: `status-bar`, `editor-top` and `editor-bottom` (above or below the notes, across every pane), `window-left` and `window-right`, `selection` (floating just above the selected text, or below it near the note's top), `cursor-line` (floating at the end of the cursor's line), `keyboard` (the iPhone, read through the FFI's `keyboard_toolbar()`), and `browser-bar` (the iPhone's bottom bar, read through `browser_bar()`, where the spacer stands for the note's title and the `sync` widget is the sync indicator).
- **Behaviour**: `always`, `on-hover` (a strip along its edge reveals it after `hover-delay`, floating over the edge rather than moving the notes), `hide-while-typing`, `with-selection`, and `in-context` with `contexts = ["text", "math", "code", "table"]`. A docked bar that's hidden keeps its room, so the notes never jump; a bar the keyboard is in always shows.
- **Style**: `icons`, `icons-and-labels` or `labels`, and `compact` or `comfortable`. Sizes are the theme's `toolbar.` tokens (button sides, gaps, icon sizes, padding, the button radius, which plus the padding gives a floating bar concentric corners).
- **Items**: any command id, shown with its icon from the registry (every command has a Phosphor icon there), a tooltip with its title and current shortcut, a disabled look where it can't run and a pressed look for a toggle that's on at the cursor (bold in bold text, the list kind in a list, from `gasp_core::commands::active_commands`); the status widgets `word-count`, `character-count`, `reading-time`, `edit-time`, `cursor-position` and `sync`; `separator`; `spacer`, which pushes what follows to the far end; and `menu:<id>`. Unknown commands and menus load with a warning.

`toolbar.focus` (`Alt+Shift+T`) moves the keyboard into the first bar (a bar floating by the selection or the cursor's line comes before the docked ones), and again to the next: arrows move between buttons, Home and End jump, Enter or Space presses, Tab and Shift+Tab change bar, and Escape goes back to the note. A pressed command runs as its key would, in the note. A right-click on a docked bar offers "Add to …", "Customize toolbars" and "Hide …"; the status bar's `+` (while the pointer is on it or the keyboard reaches it) and "Add to …" open the Toolbars settings page with that bar's picker open.

The settings screen's **Toolbars** page lists every toolbar on its own card: a switch to turn it on or off (a bar the vault added also has a remove button, and a changed built-in one a reset), its place and "When it shows" as dropdowns, "Buttons show" and "Size" as segmented controls, the kinds of text for `in-context`, each item with its icon and shortcut, and "Add a button", a searchable picker of commands, widgets, separators, spacers and menus. Items reorder by dragging (onto another bar too) or with `Alt+Up` and `Alt+Down`, and `Delete` takes one off. "Add a toolbar" starts an empty one above the notes, and "Reset toolbars" (pressed twice) puts the built-in ones back. It writes with the same comment-keeping writers as the rest of the screen (`gasp_config::toolbar_files`), a built-in toolbar's field only while it differs, and changes apply at once. `gasp mcp` has `get_toolbars` and `set_toolbars`.

## Keyboard first

Everything in the app can be done from the keyboard, and the mouse is optional. Concretely:

- **Every action is a command** (see [Rules](#3-rules)), and every command appears in the command palette with its current shortcut next to it. A command with no shortcut can still be run from the palette, and you can bind a key to it from the palette.
- **Every part of the UI can take focus.** You move focus between the editor, the file tree, search results, the outline and any panel with shortcuts. Inside each one, arrow keys move, Enter opens, and Escape goes back to the editor.
- **Every list and dialog works the same way.** That covers the file tree, search results, the quick switcher, the emoji picker, the snippet and footnote pickers, the conflict resolver and the settings screen. Arrows move, Enter accepts, Escape closes, and typing filters. The conflict resolver has single keys for "this device", "other device" and "both".
- **Mouse-only behaviour gets a keyboard twin.** For example, the hover-to-reveal file sidebar also has a toggle shortcut.
- **Shortcuts are shown where the action lives:** in menus, in tooltips and in the palette. Holding `Mod` for a moment shows an overlay of the shortcuts available in the focused area.
- **The iPhone uses the same keymap** when a hardware keyboard is attached.
- **Defaults avoid OS-reserved shortcuts.** On macOS that includes `Cmd+H`, `Cmd+M`, `Cmd+Q`, `Cmd+Alt+H`, `` Cmd+` `` and `Cmd+Space`, and each platform has its own equivalents to avoid.

Verification covers this too: a CI test walks every command in the registry and fails if one can't be reached from the keyboard.

## Features

### Editor

- **Markdown symbols** (`**`, `#`, `[]()`, `$`, `>` and so on) can be shown or hidden, and switching is one command with a hotkey. There are three modes:
  - *Always shown*, like a plain source editor.
  - *Revealed around the cursor*, like Obsidian's live preview and Typora. The reveal scope is configurable: only the element the cursor is in (Typora), the whole line, or the whole block.
  - *Always hidden*, so the note reads like a finished page while staying editable.

  Each kind of syntax can override the mode. For example, link URLs can stay hidden while emphasis marks follow the cursor. Because the mode is a setting, rules can switch it too, such as showing everything while a search-and-replace is open.
- **Blocks.** Headings with folding, lists, task lists, tables with a grid editor (below), callouts (all Obsidian types, foldable), code blocks, block and inline math, images and embeds, `%%` comments, highlights, raw HTML (`<br>`, `<hr>`, `<div>`, `<img>` rendered inline), and frontmatter.
- **Tables** are edited as a grid, as in Obsidian; nobody wants to edit them as text. A table stays a grid with the caret in it: each row is its source line laid out as cells in shared columns, the pipes and the delimiter row never show, and a cell's own Markdown (bold, links, code, math) reveals around the caret as a paragraph's does. The source stays the truth. Typing in a cell edits its source, a typed `|` is written `\|`, and pasted text goes in on one line, its lines joined by spaces, a whole line copied with its break included (a cell can't hold a line break). Structural edits rewrite the table with its columns padded to line up in the source, as one undo step; typing doesn't re-pad, but a table typed in is padded when the caret leaves it, folded into the typing's undo step.
  - *Keys.* Tab and Shift+Tab go to the next and previous cell with its text selected, and Tab in the last cell adds a row. Enter goes to the cell below, adding a row at the bottom. Mod+Enter (off a link) and Escape leave the table for an empty line below it with a blank line between, made when it isn't there, since a line right under a table is one more of its rows. Left and Right cross into the neighbouring cell at a cell's edge and out of the table at its ends; Up and Down move through a cell's wrapped lines, then to the cell above or below in the same column, and out past the header or the last row. Backspace and Delete stop at a cell's edge.
  - *Right-click a cell* (or a handle) for insert row above or below and insert column left or right at the top of the menu, then three submenus: *Row* (move up, move down, delete row), *Column* (move left, move right, align left, center or right as `:--`, `:-:`, `--:`, sort A→Z or Z→A with the header staying put, delete column) and *Table* (copy as Markdown or as tab-separated text, edit as Markdown, delete table). Context menus open below the pointer, or above it where there isn't room, and a submenu slides to stay in the window. Items that don't apply are greyed out: nothing goes above the header, it can't be deleted or moved into the body, the first body row can't move up, the only column can't be deleted. Every item is also a palette command (Tables), with Insert table (three columns, a header and two rows, the caret in the first header cell); none has a default key, so bind them in settings.
  - *Handles.* Hovering a row fades in a handle at its left edge, and hovering a column one above its header. Clicking a handle selects the row or column; dragging it lifts the row or column onto an opaque card under the pointer (the page's own fill, shadowed, its text a little faded, its own place veiled), with the caret, the cell's ring and revealed markup hidden while it's held. A line under the card, running out past the table's edges so it shows wherever the card is, marks where it would drop; there's none over its own place, and dropping there changes nothing. Letting go moves it there as one undo step and leaves it selected; Escape puts it back. Dragging a row near the note's top or bottom scrolls it, as a drag selection does.
  - *Several cells.* A selection from one cell to another, by dragging or Shift and an arrow from a cell's edge, takes in the whole block of cells, drawn as whole cells without revealing their markup. Delete and Backspace empty them, Copy gives tab-separated text, and typing empties them and types into the first.
  - *Look.* A grid reads as a table at the default size, and every value in it is a theme token. Columns fit their widest cell up to the reading width, and when they don't fit, the columns with the most to wrap give way. No column is narrower than `table.min-column-width` lines of body text (5) and no row shorter than `table.min-row-height` (1.5), so a table just inserted is three by three cells big enough to click into. Cells are padded by `table.cell-padding-x` and `table.cell-padding-y` (`space.lg` and `space.sm`), and a one-line row sits centred in its height. Square rules in `color.table.rule` (`table.rule-width`) go around and between every cell. The header row has bold text, a faint fill (`color.table.header`) and a heavier rule under it (`color.table.header-rule`, `table.header-rule-width`). The cell being edited is tinted and ringed inside its square edges, over its own rules, in `color.table.active` (the accent) at `opacity.table-active-fill` and `opacity.table-active-ring`. Cells selected together fill with `color.table.selection`. The handles, the drop line and the dragged card take `table.handle-size`, `table.handle-gap`, `table.handle-radius`, `table.drop-line-width`, `table.drag-shadow-blur` and `opacity.table-drag`; the card is rounded, so its edge is a hairline ring drawn as a shadow rather than a border.
  - *Escape hatch.* "Edit table as Markdown" shows the table's source until the caret leaves it, and showing all symbols shows every table's source.
  - Column widths come from every row's cells, measured once and kept until the row changes, so typing in a cell of a long table re-measures only its row; rows off screen are never laid out. The parser likewise reparses only the edited row (with the header and delimiter row for context) while it stays a row of the same shape, rather than the whole table: typing in a cell of a 150-row table in a 10,000-line note paints in about 1.5 ms (p50), against about 1.0 ms for prose in the 5,000-line corpus.
- **Inline HTML and CSS**, a small safe subset rather than a web engine. Paired `<b>`/`<strong>`, `<i>`/`<em>`, `<s>`/`<del>`, `<u>`, `<sup>`, `<sub>`, `<mark>` (the `==` highlight), `<kbd>` (a key cap from the keycap tokens), `<a href>` (a link like a Markdown one: hover preview, Mod+click opens; `javascript:` and `data:` are never links) and `<span>` style their text; `<p>`, `<div>` and `<center>` blocks can align it. `style=` on span, p, div and center takes `color` and `background-color` (names, `#hex`, `rgb()`, `hsl()`), `font-size` (px, em, rem, %, clamped to 0.5–3× the text around it), `font-weight`, `font-style`, `text-decoration` and, on blocks, `text-align`; every other property is ignored, and nothing runs or loads (only `<img>` loads an image). Styles nest, the inner one winning; an unclosed or mismatched tag styles nothing. Attribute values may be in straight or curly quotes, since Smart Typography curls them. Tags hide and reveal like Markdown symbols, and nothing inside a tag, even one still being typed, gets replacements or smart quotes. Colours a note writes keep at least 3:1 contrast against the page, so navy stays readable in dark mode. Both exports carry the same subset, with styles rebuilt from what was understood.
- **Math.** It's rendered in place and reveals its source when the cursor enters. There's a live preview above the line while you edit, matching bracket colours, and concealed commands (Latex Suite's conceal). See [the math spike](#phase-0-spikes).
- **Code blocks** get syntax highlighting, an optional title (`title:` in the fence line, replacing Embedded Code Title), line numbers, highlighted lines, and a copy button. Their appearance is themed through tokens rather than Code Styler's separate theme system.
- **Links.** Markdown links and wikilinks both resolve, hovering shows a page preview, and renames update every link.
- **Formatting commands** toggle: with a selection they wrap or unwrap it, with no selection they act on the word under the cursor, and on an empty spot they insert the pair with the cursor between. Markdown has no underline syntax, so underline uses `<u>…</u>`, which the editor, both exports and the site all render. Default bindings are in [Default keymap](#default-keymap).
- **Indenting.** `edit.indent` and `edit.outdent` (Tab and Shift+Tab, and the iPhone's keyboard bar) move whole blocks wherever the caret is, in `gasp_core::commands::indent`. A list item nests under the item before it with everything nested under it, or comes out a level; the first item of a list has nothing to nest under, an empty nested item (which Markdown folds into the text above it) moves as a line of its own, and a list that uses spaces keeps spaces (as wide as the item before's marker going in, back to the parent item's column coming out). A plain paragraph never moves in, because Markdown reads an indented paragraph as code; one already indented comes back out. A block quote or callout nests in one more `> `, or loses one. Fenced code and math keep Tab's tab at the caret and shift the lines a selection touches; headings, tables and the like stay put, so the only literal tab indent types is inside code and math. A selection moves every block it touches, and the caret and selection stay on their text. Lists and code inside quotes indent inside the quote markers.
- **Tabs.** Opening a link uses a new tab, or switches to the tab that already has that note (replacing Opener).
- **Pasting images** saves them to `./images` named after the note, the way Paste Image Rename is set up now.
- **Look up** (macOS) shows the system dictionary popover for the word under a force click or three-finger tap, or at the cursor with `Ctrl+Cmd+D` and the right-click menu. GPUI 0.2.2 delivers neither pressure events nor `quickLookWithEvent:` to the app, so `apps/desktop/src/look_up/macos.rs` adds both methods to GPUI's `GPUIView` class at startup: a pressure event reaching stage 2 (the force click itself) and a Look up gesture each trigger it, and one arriving right after the other is dropped. The first build hooked only `quickLookWithEvent:`, and force click didn't work on the owner's Mac; the pressure route is untested on hardware. If it still fails, the next step is logging which of the two methods AppKit calls, and whether GPUI's own `mouseDown:` handling keeps pressure events from starting.
- **The status bar** shows word count (for the selection when there is one), reading time and edit time. It's the built-in `status` toolbar (see [Toolbars](#toolbars)), so commands, menus and separators can go on it too, from the `+` that shows while the pointer is on it.

### Default keymap

`Mod` is Cmd on macOS and Ctrl on Windows and Linux. Every binding is a rule, so each can be changed or removed. Most defaults follow common editor conventions, and the owner's existing Obsidian hotkeys carry over, except for two. `Mod+P` opens the command palette, as in most editors, and printing moves to `Mod+Shift+P`, which used to export a PDF. Export moves to `Mod+Shift+S`.

| Keys | Command |
|---|---|
| **Formatting** | |
| `Mod+B` | Bold (`**`) |
| `Mod+I` | Italic (`*`) |
| `Mod+U` | Underline (`<u>`) |
| `Mod+K` | Insert or edit link |
| `Mod+E` | Inline code |
| `Mod+Shift+X` | Strikethrough |
| `Mod+Shift+H` | Highlight (`==`) |
| `Mod+Shift+M` | Inline math |
| `Mod+/` | Toggle `%%` comment |
| `Mod+;` | Cycle Markdown symbols: always shown, revealed at cursor, always hidden |
| `Alt+0` | Insert or jump to footnote |
| `Mod+J` | Toggle sentence-length highlighting |
| **Find and search** | |
| `Mod+F` | Find in note |
| `Mod+G` and `Mod+Shift+G` | Next and previous match |
| `Mod+Shift+R` | Find and replace in note |
| `Mod+Shift+F` | Search all notes (replace across notes from the same panel) |
| **Notes and navigation** | |
| `Mod+O` | Quick switcher |
| `Mod+P` | Command palette |
| `Mod+N` | New note |
| `Mod+Shift+O` | Jump to heading |
| `Mod+Alt+[` | Fold or unfold the heading the cursor is in (the iPhone so far) |
| `Mod+Enter` | Follow link under cursor |
| `Mod+[` and `Mod+]` | Back and forward |
| **Tabs and panels** | |
| `Mod+T` | New tab |
| `Mod+W` | Close tab |
| `Mod+Shift+T` | Reopen closed tab |
| `Mod+1` to `Mod+9` | Go to tab |
| `Ctrl+Tab` and `Ctrl+Shift+Tab` | Next and previous tab |
| `Mod+\` | Toggle file sidebar |
| `Mod+Shift+E` | Focus file tree |
| `Alt+Shift+T` | Focus toolbars (again for the next one; arrows move, Enter presses, Tab to the next bar, Escape back to the note) |
| `Mod+Alt+Left` and `Mod+Alt+Right` | Move focus between panes |
| `Mod+Alt+Up` and `Mod+Alt+Down` | Move focus to the pane above or below |
| `Mod+Alt+Shift` with an arrow | Move the tab to the pane that way, splitting one off if there's none |
| `Mod+Alt+W` | Close pane |
| **App** | |
| `Mod+Shift+P` | Print (opens the print preview, which can also save a PDF) |
| `Mod+Shift+S` | Export (HTML or PDF, with publish to the site) |
| `Mod+S` | Sync now |
| `Mod+,` and `Mod+L` | Settings |
| `Mod+Shift+N` | Open another vault |

### Replacements

Smart Typography and Symbols Prettifier become one table of replacements, imported from your current settings. It covers curly quotes, em dashes, ellipses, arrows and comparison symbols, plus all the prettifier entries, keeping the ones you disabled switched off. Replacements never fire inside code or math unless a rule says so.

### Emoji and symbols

Typing `:` followed by at least one letter opens a small picker that fuzzy-searches emoji names and a list of Unicode symbols and ASCII emoticons. It never opens inside math, code, URLs, or after a digit, so `10:30` and `$f:A\to B$` stay quiet. Enter or Tab accepts the pick and Escape dismisses it.

### Math snippets

The snippet engine replaces Latex Suite with a format you can read without knowing regex. A snippet is a trigger, an expansion and a few plain-word options:

```
mk          → $●$                     anywhere, instant
reals       → \mathbb{R}              math, instant
forall      → \forall ␣               math, instant, whole word, after space
{letter}{digit} → {letter}_{digit}    math, instant
//          → \frac{●}{●}             math, instant
```

`●` marks a tab stop, and `{letter}`, `{digit}`, `{greek}` and `{word}` are named patterns. Raw regex is still allowed for the rare case that needs it. The snippet editor has a test box that shows the expansion while you type, and a "teach by example" mode: you type what you'd write and what it should become, and it proposes the snippet.

Your Latex Suite file has 212 snippets, and 7 of them are commented out. Of the 205 active ones, 163 are plain triggers, 41 use regex and 1 is a JavaScript function. Most of the regex ones only use regex for "whole word followed by a space" (for example `/(?<![\\A-Za-z])forall $/`), and they convert to the `whole word, after space` options. The migrator converts 202 to the readable format and keeps 2 as `regex:`, because `${MORE_SYMBOLS}` and the trig "add space" class have no named pattern. The JavaScript `iden(\d)` snippet is listed for you to review.

Latex Suite's other features are rebuilt as well: auto-fraction, matrix shortcuts (Tab and Enter inside `pmatrix`, `cases`, `align` and friends), tab-out of brackets, auto-enlarged brackets, and the `mk`/`dm` math-block shortcuts.

### Footnotes

Footnotes are a core feature, carrying over everything from your Footnotes Plus plugin:

- one command inserts the next numbered reference and its definition (your `Alt+0` hotkey carries over)
- a command jumps between a reference and its definition
- numbered footnotes renumber automatically
- missing, duplicate, unused and empty footnotes are highlighted
- the `^[1]` typo converts to `[^1]`

References render as superscripts, hovering shows the footnote text, and PDF export puts each footnote at the bottom of the page that cites it.

### Search

Search replaces Omnisearch and is a core panel, not a modal bolted on top.

- **Index.** Tantivy, a Rust search engine, holds the index. It updates incrementally as you type, so there's no reindex pause.
- **Ranking** uses your current weights: file name 10, folder 7, H1 6, H2 5, H3 4, tags 2. It ignores diacritics and splits camelCase.
- **Results** update on every keystroke. Each one shows the best-matching passages with highlights, and the preview pane renders the matched section with the live-preview renderer, scrolled to the hit.
- **Images and PDFs** are searchable through OCR. That's Apple's Vision framework on Mac and iPhone, Windows' built-in OCR engine on Windows, and the `ocrs` Rust library on Linux. The extracted text is cached so each file is processed once.
- **Search and replace** across the vault uses the same panel.

### Sentence-length highlighting

This rebuilds Musical Text from scratch in the `prose` crate. The segmenter starts from Unicode sentence boundaries (UAX #29) and adds the cases that broke the plugin:

- em dashes and en dashes never end a sentence
- an ellipsis ends a sentence only when a capitalised word follows it
- abbreviations and acronyms (e.g., i.e., U.S., Dr., St.) and initials don't end sentences
- decimals, version numbers, times and URLs are left alone
- a closing quote or bracket after the period stays with its sentence
- headings, list items, table cells, code and math are handled as their own units or skipped

It keeps your thresholds (under 7 words is short, over 18 is long) and your colours as theme tokens. The test corpus is every sentence in your vault, checked against hand-labelled boundaries.

### Grammar

Harper misses real errors and flags things that are fine, and you still run everything past Claude before publishing. The goal is an offline checker good enough to replace that final Claude pass. It has four layers, and each one does only what it's reliable at:

1. **Mechanical checks** catch double spaces, repeated words, and spacing around punctuation. These are deterministic, and they're the part of Harper that works well, so they come from `harper-core` with its noisy rules off by default.
2. **Spelling** uses a dictionary that learns from your vault. A word you've used in several notes, such as a name or a piece of jargon, is not flagged.
3. **A local grammar model** reads each sentence when you pause and proposes the smallest edit that fixes it. It runs on the device through llama.cpp on every platform, and Apple's on-device model is also a candidate on Mac and iPhone. It only shows an edit when it's confident, because false flags are the bigger complaint.
4. **Proofread before publishing** is a command that runs the model over the whole note and shows the proposed edits as a list to accept or skip, the way you use Claude now. If you're online and have added a key, it can use Claude instead.

Checking skips code, math, links, raw HTML and block quotes. Dismissing a flag is remembered for that phrase, and a rule you keep dismissing mutes itself and tells you so.

The model is chosen by testing rather than by guessing. Phase 4 starts with an evaluation set built from your own writing: your published essays with realistic errors inserted, plus the real mistakes Claude catches from now on. Each candidate is scored on false flags first and missed errors second, including Harper alone as the baseline to beat.

### Edit-time tracking

This replaces Chronotyper without touching frontmatter. Each device writes its own stats file under `.gasp/stats/`, so two devices never conflict, and the app sums them. The `edited_seconds` values in your 49 existing notes are imported. After that, the migrator removes the `updated` and `edited_seconds` keys from those notes, and deletes a frontmatter block when nothing else is left in it. That happens in a single commit, so it's easy to revert.

### Link cards

Pasting a URL on an empty line can turn it into a preview card, as Link Embed does now, with metadata fetched locally. This is a later-phase feature.

## Sync

### What exists today

Your vault is already a git clone of `borisnezlobin/notes` on branch `main`. Your `vault-sync` daemon (a Node program on the owner's Mac, not on GitHub, run by the launchd agent `com.randomletters.vault-sync`) commits a minute after you stop typing, merges, and pushes. The iPhone uses GitSync triggered by Shortcuts automations. The repo's default branch is `fake-default-lol`, and there is no `master` branch yet.

### Design

The app does the same job natively on every device, including the iPhone, using libgit2 through the `git2` crate. Keeping real git everywhere means the app coexists with `vault-sync` and GitSync during the transition, keeps full local history, and needs no custom server.

- **Branch.** The app syncs a new `master` branch, not `main`. At cutover, the first clone creates `master` from the tip of `main`. Until every device has moved to the app, each sync also merges anything `vault-sync` or GitSync push to `main` into `master`, one way, so nothing is lost. The app never pushes to `main`. Once every device runs the app, `main` is retired. `crates/sync` does this through `VaultConfig::legacy_branch`, which defaults to `main`, and it's tested with a simulated old tool. The `master` branch in `borisnezlobin/notes` isn't created until cutover.
- **Setup** asks for the repo, the branch and a fine-grained GitHub token limited to that one repo. The token is stored in the Keychain on Apple platforms, Credential Manager on Windows, and the Secret Service on Linux.
- **Offline editing** works naturally, because edits are committed locally and pushed when there's a connection.
- **Sync timing** follows `vault-sync`: commit about a minute after you stop typing, then fetch, merge and push. The iPhone also syncs when the app opens, returns to the foreground or goes to the background, so the Shortcuts automation isn't needed.
- **The merge policy** is ported from `vault-sync`. Notes merge line by line and changes that don't overlap merge automatically. When both sides only add lines at the same place, such as two appends to the end of a daily note, both are kept, this device's first, and lines both additions start or end with are kept once. A missing newline at the end of a note doesn't count as a change to its last line. Images and binaries keep the local copy. Device-only files never sync.
- **Real conflicts**, where both sides changed or deleted the same lines, open a native resolver instead of the `localhost:7777` page. You get each clashing hunk side by side and pick this device, the other device or both, or edit the merged text directly. Until you resolve it, the note stays editable with both versions visible.
- **A conflict never stops sync.** The merge still finishes with a commit, so git is never left mid-merge. In the branch, the conflicted note keeps its `master` side: the remote's version when merging `master`, and this device's when merging `main`. Nobody's text disappears from `master` while you decide, and this device's version is in the pushed history too. On disk the note shows both versions between markers, and `.git/gasp-sync-conflicts` records the three versions for the resolver. Commits leave the note out until it's resolved, while every other note keeps committing, merging and pushing. Resolving writes the note and syncs it right away. Deleting the markers by hand counts as resolving it. If another device changes the note again first, its change is merged into what's on disk, so it stays waiting with nothing lost. A merge an older build left paused is finished the same way.
- **Commit messages** name the device and the files, as `vault-sync` did: `mac: Lemma.md, Habit Ideas.md`, or `mac: 5 files changed` past three files. The device name is the machine's short hostname (`scutil --set HostName mac` sets it on a Mac).
- **Status** shows as a small indicator (synced, syncing, offline with N changes waiting, or conflict) that opens a sync log.

Only one thing should sync a given device's vault at a time. When the app takes over on the Mac, the `vault-sync` launchd agent gets unloaded; on the iPhone, GitSync and its automations get removed.

The repo is 252 MB, and about 77 MB of it is Obsidian plugin binaries (`mcp-tools`' 58 MB `mcp-server` and Harper's 19 MB bundle). Removing those from the repo after migration is optional, and I'll ask before doing it.

## Export

### HTML export to your website

Today the flow has three steps. Webpage HTML Export writes to `~/Downloads/bored/HTML Exports`. Then [`scripts/publish-article.mjs`](https://github.com/borisnezlobin/website/blob/main/scripts/publish-article.mjs) in the website repo extracts the `.markdown-preview-view` body, strips scripts, styles and frontmatter, archives it to `website/html/blog/<slug>.html`, and can publish it to the live site. The site's CSS (`obsidian.css`, `mjx.css`, `code-styler.css`) is written against Obsidian's class names and MathJax's HTML output.

The native export produces the cleaned article body directly and can publish it to the site in one step. The site's credentials are kept in the OS credential store, and the archive copy lands in the website repo.

The export emits clean semantic HTML instead of Obsidian's markup. It uses plain elements for text, `<aside>` for callouts, a standard footnote list, and MathML for math, which current browsers render natively. The site's stylesheets get rewritten once to match (`obsidian.css`, `mjx.css` and `code-styler.css` are replaced by one article stylesheet), and the articles already on the site are re-exported so old and new share the same markup.

### PDF export

This ports PDF Export Plus to Typst, a Rust typesetting engine that runs on every platform including the iPhone. Typst places footnotes at the bottom of the page that cites them natively, which removes the Paged.js machinery the plugin needs inside Electron. LaTeX math converts to Typst math with `mitex`.

Your current settings carry over: A4 pages, 18/16/12/12 mm margins, page numbers, 8.5 pt footnotes, and the professional style in Iowan Old Style at 12 pt with line height 2. The drop cap is off by default, as in your PDF Export Plus settings (`applyLedeStyles: false`), and can be turned on with its line count. Page breaks from the Break Page plugin's markup are honoured, and header and footer templates from Better Export PDF become an option. The live preview renders pages as you change settings.

## MCP and agent access

The MCP server is built into the app and replaces Local REST API and MCP Tools. It can:

- read, search, create, edit, patch, move and delete notes and attachments
- read and change every setting, theme token, layout, rule, snippet and replacement
- run any command and read the editor state (open notes, cursor, selection)
- install, reload and inspect plugins
- take screenshots of the running app or of a headless render of any note

The desktop app also has headless CLI modes (render a note to an image, replay keystrokes, run a perf trace, dump the layout tree), as Flo State does. Agents and CI use these to verify changes without a person watching.

### The "emergent editor" idea

Every feature ships as a package with three parts: a plain-language spec, the code it produced, and tests. Anyone can read it, run it, fork it, or ask their own agent to rebuild it from the spec. Sharing specs alone doesn't work, because two agents given the same spec build two different features, and each user would be running code nobody reviewed. A marketplace comes after the plugin API has survived a few months of your own use.

## Verification

- **Parity oracle.** `tools/oracle` launches Obsidian with remote debugging enabled and records what it does: parse trees, rendered output, and the effect of keystrokes for replacements, snippets, footnote commands and list editing. The native core replays those recordings in tests and fails on any difference. Where we're deliberately fixing Obsidian's behaviour, the fixture is edited and the change is noted.
- **Two corpora.** Your vault is private, so fixtures recorded from it stay on your machine and run locally. The committed fixtures come from a synthetic corpus that covers the same features.
- **CI on every push** runs on GitHub's macOS, Windows and Ubuntu machines. It builds and tests the desktop app on all three, takes screenshots of a fixed set of notes on each and compares them with the expected images, and builds and tests the iPhone app in the simulator.
- **Performance budgets** are checked in CI, and a regression fails the build:

  | Measure | Target |
  |---|---|
  | Launch to editable note | under 300 ms |
  | Keystroke to pixels | under one frame at 120 Hz (8 ms) |
  | Search results per keystroke, on your vault | under 16 ms |
  | CPU while idle | 0% |
  | Memory with your vault open | under 100 MB |

  These are targets we set, not measurements yet. The spikes and the first builds will show whether they're realistic.

  Idle CPU is measured by `scripts/idle-cpu.sh`, which CI runs after the layout bench: an untouched window must draw no frames and stay under 1.5% CPU. It measures 0.6% on Linux; the owner measured 0.9% on macOS (Obsidian: 1.7%). What's left isn't the app's work: GPUI 0.2.2 keeps a loop running at the display's refresh rate while a window is visible (a `CVDisplayLink` on macOS, a timer on X11), and each tick asks whether the window needs drawing. Reaching 0% means patching GPUI so the loop stops after a few frames with nothing to draw and restarts when anything invalidates the window. That's a fork of GPUI's platform code; the macOS half can only be proven on a Mac, and a mistake there leaves a window that doesn't redraw, so it waits for a run on the owner's machine.
- **Benches of the shared crates.** Each crate's hot paths have an example bench built on `crates/bench` (`gasp-bench`: timing, a counting allocator, budgets and corpus fixtures), run with `cargo run --release -p <crate> --example <name>_bench`. CI's Core benches job runs `core_bench`, `config_bench`, `ffi_bench`, `search_bench`, `vault_bench`, `math_bench`, `prose_bench`, `export_bench` and `sync_bench` with `GASP_BENCH_ENFORCE=1`, and a measurement over its budget (about three to eight times what it measures on a Mac) fails the build. `plan_digest` prints a digest of every parse and plan of the corpus, to show a change to the parser or planner changes no output. On a busy machine, `GASP_BENCH_CLOCK=cpu` times the thread's CPU instead of the wall clock. Measured on an M-series Mac, CPU clock:

  | Hot path | Now |
  |---|---|
  | Full parse of a 200 KB note | 3.5 ms, 22.7k allocations, 1.7 MB tree |
  | Reparse per keystroke, 200 KB note | 26 µs mid-note, 48 µs near the top |
  | Plan a 60-line viewport | 15 µs |
  | Input pipeline per key, the owner's 226 snippets and 82 replacements | 3 to 6 µs |
  | Phone keystroke (update, plan, sentence tints, folds, lowering), median note / longest corpus note / 200 KB note | 27 µs / 87 µs / 1.0 ms |
  | Phone cursor move, same notes | 10 µs / 54 µs / 0.66 ms |
  | Vault search per keystroke, the corpus | 125 µs median, 1.3 ms p95 |
  | New equation to pixels / keystroke inside an equation | 92 to 110 µs / 27 to 33 µs |
  | Sentence tints after a keystroke, 200 KB note | 0.25 to 0.33 ms |
  | Loading the owner's migrated config / compiling its snippets | 1.3 ms / 8.4 ms, in the background |
  | Sync with nothing to do / one edited note, a 570-file, 261 MB vault with 500 commits since the legacy branch moved | 3.0 ms / 14.4 ms, plus the network |

  What the phone does is still whole-note work per keystroke in two places the core can't fix alone: UniFFI encodes the whole plan (330 KB for a 200 KB note) for Swift to decode, and `PlanStyler` compares line plans by value with absolute offsets, so every line after an edit reads as changed and is restyled. A plan delta (changed lines plus a shift for the rest) with Swift keeping its own copy would make both proportional to the edit.
- **Code quality.** Clippy with a cyclomatic complexity limit of 15, and SwiftLint with the same limit on the iPhone app. A complexity failure blocks a merge like a failing test.

## Phases

Each phase ends with something you can use and a check that proves it works.

### Phase 0: spikes

These are small throwaway experiments on the riskiest assumptions, each with a fallback if it fails.

| Spike | Passes if | Fallback | Result |
|---|---|---|---|
| GPUI live-preview editor | Mixed font sizes, an inline image widget, selection and IME (Japanese input) work, typing stays under a frame on a long note, and it runs on all three desktops in CI | Slint or Makepad for desktop | **Keep GPUI.** `gpui` 0.2.2 from crates.io, in `apps/desktop`. On Linux (Xvfb, software Vulkan), mixed heading sizes, inline images that grow their row, selection and typing all work. On a 5,026-line note a keystroke takes 1.3 ms from edit to painted frame (p95 about 2 ms). IME composition is implemented and tested in GPUI's test context, but real Japanese conversion still needs a desktop with an input method **[Mac]**. The macOS and Windows builds are left to CI. |
| Math via `mitex` → Typst | It renders the roughly 4,000 equations in your vault, and a typical one takes under 1 ms when cached and under 20 ms when new | KaTeX in a hidden web view on each platform, as Flo State does | **Passes on the synthetic corpus.** All 4,034 corpus equations and 485 hand-written ones render. A new equation takes 0.3 ms median and 2.3 ms at most, and a cached one under 1 µs. Setup costs 19 ms once per launch. mitex 0.2.4 ignores the spec it's given and uses symbol names Typst 0.15 removed, so `crates/math` vendors mitex's newer Typst scope and adds a compat layer. Still to do: run `cargo run --release -p gasp-math --example math_spike -- <vault> --failures` on the real vault **[Mac]**. |
| libgit2 on iPhone | Clone, commit, merge and push to `borisnezlobin/notes` with a token from the simulator and a device | GitHub's REST API with our own three-way merge | **Linux half done.** `crates/sync` clones, commits, merges and pushes between two simulated devices against local bare repos. With 200 notes, a clone takes 10 ms, a commit 7 ms and a merge 4.6 ms. **iPhone half mostly done:** `gasp-sync` builds for `aarch64-apple-ios` and `aarch64-apple-ios-sim` with no changes (libgit2 uses Secure Transport on Apple platforms; OpenSSL is vendored alongside), and the simulator app clones, commits, merges and pushes against local repositories (see [Phase 7](#phase-7-iphone)). HTTPS to GitHub from a device is still to try. |
| QuickJS on iPhone | A sample plugin runs and hot-reloads | Declarative plugins only on iPhone | Not started. It needs a macOS runner. |
| Typst PDF | It reproduces one of your existing PDF Export Plus exports with page-bottom footnotes | Paged.js in a hidden web view at export time | **Works; comparison pending.** `crates/export` compiles all 204 corpus notes with footnotes at the bottom of the citing page and an optional drop cap (off by default), in about 44 ms for a typical note and 87 ms for the most math-heavy one. The side-by-side check against a real export is still to do **[Mac]**. |

### Phase 1: core

This covers the document model, transactions and undo, the parser with Obsidian extensions, the render planner, the config and rules engine, the command registry, the parity oracle, and the migrator for your `.obsidian` settings, hotkeys and snippets. It's done when the parser and render planner match the Obsidian fixtures for the synthetic corpus in CI, and for the real vault on the owner's Mac **[Mac]**.

| Part | Where | State |
|---|---|---|
| Document, selections, transactions | `crates/core/src/document.rs`, `transaction/` | Done. A rope with byte offsets, and change sets that apply, invert, compose and map offsets, with property tests. |
| Undo | `crates/core/src/history.rs` | Done. Typing groups into steps within 500 ms, commands get their own step, and remote edits rebase both stacks. |
| Input pipeline | `crates/core/src/pipeline/`, `steps/` | Done except emoji. List continuation, auto-pair, snippets and replacements run as named steps with context filters. The emoji slot is an empty placeholder. |
| Parser | `crates/core/src/syntax/` | Done. It uses pulldown-cmark plus Obsidian's extensions, keeps markup ranges apart from content, reparses only the changed blocks, and answers `context_at`. On an M-series Mac a 200 KB note parses in about 3.6 ms in full (22.7k allocations, a 1.7 MB tree), and a one-character edit reparses in 26 to 50 µs; `SyntaxTree::edit` answers which top-level blocks it parsed again. |
| Render planner | `crates/core/src/render/` | Done. It produces styled runs, hidden ranges and widgets per line for all three reveal modes and scopes. A 60-line viewport plans in about 15 µs and a whole 200 KB note in about 0.9 ms. `KeptPlan` keeps a whole-note plan and re-plans only the blocks an edit reparsed or the cursor left or entered, which the phone uses. Heading folding isn't in the plan output yet. |
| Footnotes | `crates/core/src/footnotes/`, `commands/footnote.rs` | Done. Every Footnotes Plus test is ported. |
| Formatting commands | `crates/core/src/commands/format.rs` | Done for the eight toggles in the keymap. `format.link` isn't built yet. |
| Config, rules, commands | `crates/config` | Done. Layered defaults, a settings schema, theme tokens, the layout tree, the rules engine, the command registry and the whole default keymap, with the keyboard-reachability check. |
| Snippets and replacements | `crates/snippets` | Done. |
| Migrator | `tools/migrate` | Done for Latex Suite, replacements, hotkeys and app settings, checked against `reference/obsidian`. Chronotyper removal needs the vault and comes later. |
| Parity oracle | `tools/oracle` | Not started. Recording fixtures needs Obsidian **[Mac]**. |

### Phase 2: desktop editor

This is the GPUI app with the file sidebar, tabs, quick switcher, command palette, live preview for every block type in your vault, replacements, emoji, snippets, footnotes, math, code blocks and images. It's done when you can use it as your daily editor on the Mac, and CI produces working Windows and Linux builds.

**Settings audit.** Every page and control of the settings screen was walked with `gasp --snapshot` and fixed where it moved, hid a state or lost the keyboard; each fix has a test in `apps/desktop/tests/settings_view.rs`.

- A resettable row keeps its reset button's room (`controls::reset_slot`), so picking an accent, a choice, a number or a font no longer shifts and rewraps the row.
- The chosen accent swatch has a concentric ring and a check, drawn as filled circles in a control-sized square (a shadow's spread kept the swatch's radius and drew a rounded square).
- Dropdowns are as wide as their widest option (`controls::widest_of`), and font dropdowns have one width, so a new choice doesn't move the row.
- "+" on a shortcut stays pressed while it waits, with the prompt and any refusal hung under it, instead of a box that pushed the row.
- Arming "Remove toolbar" or "Reset toolbars" keeps the button's size; the armed state is a warning fill and, for remove, a note.
- Hover and press fills are see-through tokens (`hover_fill`, `pressed`), so icon buttons, menu options, sections and switches show hover on cards and in dark mode, where the old fill matched the card or the popover. Buttons, switches, segments and swatches now have a pressed look, an open dropdown stays pressed, and a chosen chip keeps its fill under the pointer.
- A row whose switch is off takes no clicks or keys (its stepper and dropdown used to still change it).
- A refused value rings its field in the warning colour and keeps the keyboard there; Escape puts the saved value back instead of closing the screen.
- Moving the first item up or the last down is an inert button with no hover; the move and remove tooltips name Alt+Up, Alt+Down and Delete.
- Focused toolbar items and list entries get an opaque fill under their ring (it used to fill the group grey), switches and the swatch group get concentric rings, and the repository row of a vault that doesn't sync no longer takes an invisible focus.
- "Create a token" is reachable from the keyboard (Left and Right on the account row).
- Menus and their options have concentric radii, the snippet slot's outline is square-cornered, the section list fits "Snippets and replacements", the add-button picker says "Pick a button", and the status widgets say what they count.
- GPUI draws a shadow's spread with the element's own radius, so focus rings on radius-6 controls (buttons, fields, dropdowns) are still 2 px short of concentric.

**Workspace audit.** The chrome around the notes (sidebars, tab bar, panes, status bar, docked toolbars, menus, tooltips, palette and switcher) was checked for layout shift, hover regions, states, keyboard reach, menus, drags, hit areas and copy. What changed:

- The file sidebar shown on hover no longer hides under its own sort or vault menu, or while it has the keyboard or a drag. Whether the pointer is in it is worked out from where it was last drawn (`ui::DrawnArea`), from a window-level listener for moves, releases and the window's mouse-exit, rather than from GPUI's hover, which ends whenever anything is drawn over the panel, during any drag, and never reports a leave it didn't see start. A hide that comes due while the panel is in use is dropped.
- Toolbars shown on hover follow the same rule: they stay while their menu is open or the keyboard is in them, and hide when the pointer leaves straight from their edge strip, where they used to stay up.
- The status bar's widgets use tabular figures, keep the widest width they've had for the note, and the cursor position keeps room for `000:00`, so moving the caret no longer shifts the counts beside it.
- With the sidebar over the note, the first tab bar keeps the window buttons' room and its show-sidebar button while the sidebar shows, so the tabs no longer jump on every reveal.
- Escape cancels a drag: a tab or note goes back, and a divider or sidebar resize returns to its size.
- Sort order, collapse all, new folder, the vault switcher and the shortcuts list are commands (`file-tree.sort`, `file-tree.collapse-all`, `file-tree.new-folder`, `vault.switch`, `help.shortcuts`), so the keyboard reaches them; the phone leaves them out.
- The sidebars' resize strips show a line on hover and while dragged, as pane dividers do; the tab drop zone uses a ring rather than a border on its rounded shape; the sync button has a pressed look; a file tree menu opened by the pointer starts with no row highlighted; a background tab's close button no longer names Cmd+W, which closes the active tab.

**Editor audit.** The editor and its overlays (selection, reveal, widgets, tables, links, floating toolbars, find, suggestions, prose cards, scrolling) were walked with `gasp --snapshot`; each fix has a test.

- The bar by the selection is laid out before it's placed, so it sits a gap above the first selected row on screen, flips below the last one near the pane's top, and stays inside the pane at its edges or when the selection fills it; a selection begun at a line's end is placed by the text it covers (`toolbar/floating.rs` tests).
- A list or task marker shown at the cursor sits in its bullet's room, flush against the text (the planner's `LinePlan::shown_marker`), so a list item's text no longer shifts or rewraps as the caret enters it (`live_preview.rs`, `render/tests.rs`).
- Scroll anchoring: lines above the first one in view that change height as they're laid out (an estimate replaced, an image or equation arriving) move the scroll with them, so scrolling up onto a tall line no longer jumps (`live_preview.rs`).
- The find bar's current match is drawn over the selection, in a deeper amber than the others; it used to read grey. Replace opened with an empty query types into the query (`find_bar.rs`, `element.rs`).
- `Alt+Shift+T` goes to the bar by the selection before the docked bars (`toolbars.rs`).
- Still open: heading folds have no desktop control or visible folded state, and a callout's fold has no key, so `fold.toggle` stays iPhone-only; showing a block's source (math, code, callouts, images) still moves what's below by the source's height; a shown task marker (`- [ ] `) is wider than the box and nudges its text; an image found through the vault index first draws the placeholder's size for a frame; flags from a check that finished before the vault's words loaded wait for the next redraw.

**Usability pass.** Each workflow (first launch, everyday writing, finding things, layout, output, customizing, recovering from mistakes) was walked end to end with `gasp --snapshot`, looking for dead ends, missing keyboard paths, hidden state and errors with no way forward. Each fix has a test, most in `apps/desktop/tests/usability.rs`.

- Notices (`crate::notices`): a line at the window's bottom right for what happened out of sight. Done ones leave after `notice_duration`; problems and offers stay until dismissed; one can carry a command as its button. Failures that only reached stderr now show there: a save that failed (the edits stay open), a pasted image that couldn't be kept, a note or vault that couldn't open, link updates after a rename, footnote messages.
- Opening a vault search result closes search, so the match is in view instead of under the panel.
- `vault.import-obsidian` runs `gasp-migrate` from the app (File menu, General settings, palette): the vault's `.obsidian` folder, or a picked one, goes into `.gasp/` with what the vault already sets winning (`gasp_migrate::import`), and the report opens in a tab. A vault with Obsidian settings and no config of its own is offered it once, in a notice, remembered in `device.toml`.
- The migrator no longer swaps Mod+P and Mod+Shift+P: importing hotkeys used to take the palette off Mod+P.
- Trashing a note keeps its text for the session, unsaved edits included; the notice's Undo and `note.restore-deleted` bring it back and open it.
- A rename to a taken or impossible name keeps the typed name selected in the title with a notice, instead of a dialog that threw it away.
- Leaving a table with Escape or Mod+Enter keeps a blank line under it, so what's typed next isn't another row; a caret right after a link counts as on it, so Mod+Enter follows a link just picked from the suggestions.
- `export.copy-rich-text` (macOS) puts the selection or note on the pasteboard as the HTML export's article with its Markdown as plain text; `view.toggle-dark-mode` switches light and dark by writing `appearance.theme`; `note.move` moves the open note into a folder picked by name.
- The welcome screen can make a new vault, and the arrows or Tab reach its buttons and recent vaults.
- Readable width off gave the note a third of the pane (its gutters shared the row); the column now fills it. Zoom and readable width apply to every open note and are kept per device (`text-zoom`, `full-width` in `device.toml`), where they were per tab and forgotten.
- `![[Note]]` and `![[Paper.pdf]]` read as links (followed with Mod+Enter, previewed on hover) instead of the image placeholder; the placeholder for a missing image is a see-through hatched grey rather than a colour gradient that looked like a picture.
- Settings show the vault's folder from `~`; the note menu's Export item says it offers PDF or HTML; snapshot runs draw prompts in the window, since a hidden window has no native one.
- Still open: a vault that isn't a git clone can't be connected to a repository from the desktop (the Sync page says to clone it); notes embedded with `![[…]]` show as links, not their text; a deleted note can only come back in the session it was deleted in (after that, from the system trash); `sync.now` on a vault that doesn't sync does nothing, by design, so Mod+S out of habit doesn't open settings.

### Phase 3: sync, then travel check

This builds git sync, the conflict resolver and the setup flow, and replaces `vault-sync` on the Mac. It's done when you've installed the app on the Windows/Ubuntu laptop, synced the vault on both systems, and edited offline and merged. This is the checkpoint that has to pass before your next trip. The app is only for you for now, so desktop builds are signed for personal use and there's no store listing.

The git engine is in `crates/sync`. It has clone, commit, fetch, merge and push on one branch, the one-way merge of the legacy `main` branch, the line-by-line merge policy with both sides' insertions at one place kept, local copies kept for binaries, device-only files, commit messages naming the device and files, and the clock-driven scheduler. Conflicts are parked rather than paused (see [Sync](#sync)): `vault/parking.rs` parks, carries and settles them, `parked.rs` keeps the record, and the resolver reads the note back from disk, so edits made while it waits are kept and a resolution for a note that changed since is refused. The app side in `apps/desktop/src/sync/` has token sign-in over HTTPS, keychain storage, the status bar indicator and popover, the settings page and the resolver. It's tested with two and three simulated devices against local bare repos, in `crates/sync/tests/` and `apps/desktop/tests/sync.rs`.

**Packaging for macOS.** `make dmg` (see `make help` for the rest) runs `scripts/package-macos.sh` with the owner's Developer ID and the `editor-notary` notarytool profile. `scripts/package-macos.sh` builds `Gasp.app` for Apple Silicon and Intel in one file with the `dist` profile (thin link-time optimization, symbols stripped), signs it and packages `target/package/Gasp-<version>.dmg`. OpenSSL is built into the app (`git2`'s `vendored-openssl`), so the app needs nothing from Homebrew. Without `DEVELOPER_ID` it signs ad hoc, which only runs on the Mac that built it. To give it to other people, set `DEVELOPER_ID` to the owner's "Developer ID Application" certificate and `NOTARY_PROFILE` to a profile saved with `xcrun notarytool store-credentials`, and the script signs with the hardened runtime and notarizes and staples the `.dmg`. The icon is built by `apps/desktop/assets/icon/build_icon.sh`; its whale is CC BY 4.0, credited on the General settings page and in the bundle's `THIRD_PARTY_NOTICES.txt`. The app still needs an updater.

### Phase 4: search and prose

This covers search with OCR, sentence-length highlighting, word count, reading time and edit-time tracking. Grammar starts with the evaluation set described under [Grammar](#grammar), then the layers are built in order, and the model is picked from the scores.

### Phase 5: export

This covers HTML export with one-step publish to your site, the one-time stylesheet rewrite and re-export on the site, and PDF export with a live preview. It's done when a new article and a re-exported old one both look right on the site, and a PDF matches your current output.

PDF export already exists in `crates/export` from the Typst spike. HTML export, publishing and the live preview are still to do.

### Phase 6: MCP and headless modes

This covers the MCP server with full read and write access, the CLI modes and screenshots.

**The MCP server is built.** `gasp mcp [VAULT]` serves one vault (the last one opened when `VAULT` is left out) over MCP on stdin and stdout. It lives in `crates/mcp`, uses the `rmcp` SDK for JSON-RPC and the handshake, and never loads GPUI, so it answers `tools/list` within a few milliseconds of starting: 8.7 ms median from spawning the process to the reply (7.3 to 12.5 ms over 30 runs) on the synthetic corpus in a release build on Linux. A first search of the corpus takes 14 ms and later ones 4 ms, since notes stay in memory and only changed files are reread.

| Tools | What they do |
|---|---|
| `list_notes`, `read_note`, `search` | List notes (folder, glob, limit); read a note or a range of its lines with its frontmatter as JSON; search with the app's vault search engine and ranking, `tag:` included |
| `create_note`, `write_note`, `patch_note` | Create; replace; or change part of a note: exact find and replace with an expected count, or append and prepend to the note or a heading's section (`Parent::Child` for a repeated heading) |
| `move_note`, `delete_note` | Move with link updates as the app's rename does, honouring `files.update-links-on-rename`; delete to the system trash or the vault's `.trash` (never for good, whatever `files.trash` says) |
| `list_attachments`, `read_attachment`, `write_attachment`, `move_attachment`, `delete_attachment` | The same for other files, as base64 (images come back as images), with a 5 MB read limit |
| `backlinks`, `outgoing_links`, `tags` | The link index the sidebars use |
| `get_settings`, `set_setting`, `get_theme`, `set_theme_token`, `list_commands`, `get_rules`, `set_rules`, `get_snippets`, `set_snippets`, `get_replacements`, `set_replacements` | Config, written with the settings screen's writers so comments stay, and checked before writing: a value the app wouldn't load is refused with the reason |
| `render_note` | A page of a note as the PDF export lays it out, as a PNG |
| `editor_state`, `run_command`, `open_note` | The running app: panes, tabs, the active note, cursor and selection; any command by id; open a note at a line |

Paths are vault-relative. Absolute paths, `..`, symbolic links that lead out of the vault and hidden folders (`.git`, `.gasp`, `.trash`) are refused before anything touches the disk, and every write is atomic. A tool's failure comes back as a readable error result, not a protocol error.

**The bridge to the app.** With `mcp.enabled` on (the default; it's on the settings screen's General page), the app listens once its first frame is on screen, on a Unix socket named after a hash of the vault's path in the user's runtime folder (`$XDG_RUNTIME_DIR/gasp/mcp/`, else the app's `Gasp` or `gasp` folder in the local data folder; on Windows a localhost port written to a file there). The folder is readable only by the user, and beside the socket a file readable only by the user holds a random token the app picks each time it starts; a request without it gets no answer. A thread blocks in `accept`, so an idle app spends nothing on it, and requests are answered on the main thread between frames. When a note is open with unsaved edits, `read_note` reads the editor's text and `write_note` and `patch_note` go into the editor as one undoable edit and save; otherwise they write the file, which an open app reloads. Without the app, the app tools say it isn't running and the rest work as before.

**Connecting a client.** Build the app (`cargo build --release -p gasp-desktop`), then for Claude Code:

```
claude mcp add gasp -- /path/to/target/release/gasp mcp ~/Vault
```

and for Claude Desktop, in `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "gasp": { "command": "/path/to/target/release/gasp", "args": ["mcp", "/Users/you/Vault"] }
  }
}
```

**Snapshots.** `gasp --snapshot <note> <out.png> [--width N] [--height N] [--theme light|dark] [--cursor LINE:COL]` draws a note's editor pane as the app would show it and writes a PNG, without a window appearing, a Dock icon, or the app taking focus, so an agent can look at its work while the owner keeps working. The note is a path to a Markdown file (`.md` may be left off); its vault is found as `gasp <note>` finds it, and the vault's theme and settings are read, and nothing in the vault or the app's own folders is written: no folder migration, no saving, syncing, watching or MCP bridge (a link card's image can still land in the image cache, as it does whenever a card is drawn). The window defaults to 900 by 700 and the image is at twice its size in points, whatever screen is attached. `--cursor` puts the caret at a line and column counted from one, with a few lines above it in view and the editor focused, so the caret and the table editor's marks show; without it the caret sits after the frontmatter as a note opens.

How it works, in `apps/desktop/src/snapshot/`: the app runs with its activation policy held at "prohibited" (GPUI's application class gets a `setActivationPolicy:` that always asks for it), and opens its window with `show: false`, so AppKit never orders it in and never starts its display link. The view's `CAMetalLayer` is switched to a subclass whose `nextDrawable` keeps the last drawable it handed out, with `framebufferOnly` off so its texture can be read. Frames are asked for by sending the view `displayLayer:`, which runs GPUI's own frame callback, every 30 ms until none has been drawn for 400 ms and no equation, code block, web image or hover preview is still being drawn on a background thread (`crate::pending_renders` counts them; the limit is 30 s). The last drawable's texture is blitted into shared memory and saved. GPUI's window class answers that it's on no screen, so GPUI draws it at its fallback scale of 2 on any Mac. It needs no Screen Recording permission, since it never reads the screen. It's macOS only for now; Linux could do the same with Blade's Vulkan texture, or a window under Xvfb.

**Scripted snapshots of the whole window.** `gasp --snapshot --vault <vault> --script <script> --out <dir> [--open <note>] [--window WxH] [--theme light|dark] [--allow-writes] [--keep-temp]` (or `make snapshot SCRIPT=… VAULT=… [OUT=…] [OPEN=…]`) opens the workspace window `gasp <vault>` would, with its sidebars, tabs, toolbars, status bar, popovers, menus and settings screen, never shown, and follows a script, writing a PNG into `<dir>` at each `snap`. The window defaults to 1200 by 800 points, drawn at twice that. `--theme` sets the system appearance the app sees (light unless asked), so a vault whose settings pin a theme keeps it. `--script -` reads the script from standard input.

It never touches real data. The vault is copied (APFS clones, so it's quick and takes no room) into a new folder in the system's temporary folder, leaving out `.git` and folders behind symbolic links, and the app runs on the copy; the app's own folders (the last vault, recovery snapshots, the link card cache) go in the same temporary folder, and the app's folders are never migrated. `crate::sandbox` keeps the run from reaching outside: no syncing, no MCP bridge, no other windows, open or save panels, browser, Finder, other apps, Look up or print panel (each is logged as "left out"), a private clipboard, and deleted notes go to the vault's own trash rather than the system's. Unless `--allow-writes` is given, notes, the window state and edit times aren't saved and the vault isn't watched; a note that would have been saved looks saved, as it would a moment after typing. Actions that are writes by nature, such as changing a setting or making a note, land in the copy. The folder is removed when the run ends, unless `--keep-temp` keeps it (its path is printed) so an agent can look at what was written.

The script has one step per line. Words are split at spaces, a double-quoted word may hold spaces, `\"` and `\\`, and `#` outside quotes starts a comment. Positions are window points from the window's top-left, as `bounds` reports them.

| Step | What it does |
|---|---|
| `size WxH` | Resizes the window |
| `theme light\|dark` | Switches the system appearance the app sees |
| `open NOTE` | Opens a note (relative to the vault, or a path in the original vault) in the active tab and focuses it |
| `command ID` | Runs a registry command, as its shortcut would: the workspace's own, or dispatched from where the keyboard is |
| `move X Y` | Moves the pointer there, in steps of about 16 points from where it was, drawing a frame after each |
| `click X Y [MODS]`, `double-click X Y [MODS]`, `right-click X Y [MODS]` | Moves there, then presses and releases; `MODS` is like `cmd` or `cmd-shift` |
| `drag X1 Y1 X2 Y2 [MODS]` | Presses at the first point, moves to the second with the button held, releases |
| `scroll X Y DY` | A wheel scroll of `DY` points there; positive scrolls toward the end |
| `hover-element SEL`, `click-element SEL [MODS]`, `double-click-element SEL [MODS]`, `right-click-element SEL [MODS]` | The same at the centre of the element with that selector, after the view settles |
| `keys K…` | Presses keys as the keymap writes them (`cmd-shift-p`, `enter`, `escape`, `left`) and types quoted text a character at a time |
| `wait DURATION` | Keeps drawing for `300ms`, `2s` or `1.5s`, for timers such as hover delays |
| `settle` | Waits for the view to stop changing |
| `snap NAME` | Settles, then writes `<dir>/NAME.png` |
| `bounds SEL` | Settles, then prints the element's bounds |
| `selectors [PREFIX]` | Settles, then prints every named element on screen (starting with `PREFIX`) and its bounds |

Each output is a JSON line on standard output, such as `{"line":6,"selector":"settings-text-appearance-1","x":412.0,"y":416.5,"width":308.0,"height":39.0}` or `{"line":5,"snap":"/…/accent-before.png"}`, so an agent can compare an element's bounds before and after an action to measure a layout shift. An error names the script's line, and a selector that isn't on screen lists similar ones; the run then stops with status 1.

Selectors are the names elements get with `Selectable::selector` (`crate::ui::selector`), which replaced GPUI's `debug_selector` at every call site. GPUI 0.2.2 compiles `debug_selector` away outside its `test-support` feature and keeps the bounds where only its test context can read them, so the app's own trait passes the name to GPUI for tests and, in a snapshot run only, lays an empty canvas over the element that records its bounds as each frame is laid out; a lookup reads the last frame presented. Outside a snapshot run no canvas is added, so a normal build draws the same elements as before and pays one thread-local check per named element. Elements covered by another (a sidebar under the settings screen) still have bounds, and an element that's itself a scroll container reports bounds moved by its scroll.

Input goes through the system's path, not around it. The pointer's events are made as Core Graphics mouse and wheel events, turned into `NSEvent`s and sent to GPUI's view (`mouseMoved:`, `mouseDown:`, `rightMouseDown:`, `mouseDragged:`, `scrollWheel:` …), which makes GPUI events of them exactly as it does for a mouse, so hover, clicks, double clicks, right clicks and drags behave as they do for a person. Keys go through GPUI's `Window::dispatch_keystroke`, which does what its platform code does for a key: the keymap first, then the focused text input for a character nothing bound. The window is never the active one, which the app doesn't draw differently; the caret doesn't blink, so it's always drawn.

What can't be scripted yet: anything outside GPUI's view, since the window is never on screen. The window buttons (traffic lights) aren't drawn, AppKit's own title-bar handling and pointer tracking (entering and leaving the window, the title bar region) never run, the native menu bar isn't there, and modifier-only presses (holding Mod for the shortcut sheet) and key releases aren't sent. Only the left button drags, `DY` scrolls vertically, and there's no IME composition. Text layout uses the Mac's installed fonts, so images match the machine they're made on.

Still to do in this phase: plugin tools (Phase 8) and the other headless modes (a perf trace, dumping the layout tree).

### Phase 7: iPhone

This is the SwiftUI and TextKit 2 app on the same core, with your mobile toolbar (attach file, indent, unindent, callout, inline math, footnote, sentence highlighting, table), sync on open and close, search and export. It's done when it replaces Obsidian and GitSync on your phone. Build and run instructions are in [Building and running](#building-and-running).

**How it's put together.** `apps/ios` is an XcodeGen project (`project.yml`) for the app Gasp (target and scheme `Gasp`, sources in `apps/ios/Gasp`), bundle ID `com.borisnezlobin.gasp`, team `K2MB68Z582`, iOS 17 and up. `crates/ffi` is the whole bridge: a UniFFI surface with offsets in UTF-16, as UIKit counts them. `apps/ios/scripts/build-core.sh` builds it with `cargo rustc --crate-type staticlib` for `aarch64-apple-ios` and `aarch64-apple-ios-sim`, generates the Swift bindings with `tools/uniffi-bindgen` (UniFFI 0.32, library mode), and packs `apps/ios/Core/GaspCore.xcframework`. The phone decides nothing about Markdown itself:

| FFI object | What the phone gets from the core |
|---|---|
| `NoteDocument` | The note's text and syntax tree, kept in step with the text view (each change reparses only the blocks it touched); the render plan for a selection, read with the vault's `markdown.symbols` settings, with its folds applied; every editing command in the registry, returned as UTF-16 replacements the text view applies as one undo step; the outline; the link and card address under the cursor; sentence lengths; heading and callout folds (toggled, listed, kept as heading lines); a table cell's text written back with the table padded, and which table commands apply to a cell; a footnote's text; the syntax colours of its code blocks |
| `GrammarChecker` | One per vault: Harper's mechanical checks and vault-learned spelling (the desktop's layers 1 and 2) on the paragraphs around what's on screen, cached per paragraph, with the vault's shared ignore file |
| `ImageTexts` | The text Vision recognised in the vault's images and PDFs, cached on the device by file stamp; `VaultFolder::search_everything` adds the notes that embed a matching file to vault search |
| Functions | `render_math` (LaTeX through `gasp-math` to coverage pixels the phone tints), `warm_up_math`, `code_color_token`, `config_folder` |
| `VaultFolder` | Notes and folders; reading and atomic saving; the theme tokens and `appearance.base-font-size`, and any colour token by name; the file an image target names; the command registry as the phone sees it, the iOS key bindings and the `keyboard` toolbar; making, renaming (with link updates) and trashing notes; daily notes, templates, attached images and link resolution; backlinks, outgoing links, tags and the desktop's search; settings from the schema and writing them back; open tabs and reading positions (cursor, top line, folded headings) in `device.toml`; recovery snapshots; HTML and PDF export |
| `VaultSync` | The desktop's sync for one clone: its scheduler (the same 60-second idle debounce, retry and `sync.interval-minutes`), the commit, fetch, merge and push steps, the one-way merge of `sync.legacy-branch`, and the overview the indicator reads, in the desktop's phases and words; the notes waiting for a person, each place with its context, settled with this device, the other device, both or edited text; signing in and out with the token in the Keychain; changing the repository; following the sync settings. `set_up_sync` reads GitHub shorthand, clones (starting `master` from `main` when `master` doesn't exist yet) and keeps the token; `merge_note_edits` folds unsaved edits into a note a sync just wrote |

To give the phone the desktop's behaviour rather than a copy of it, code that lived in the desktop app moved into shared crates: daily notes, templates, Moment.js dates, recovery snapshots, attachment naming and note link resolution into `gasp-vault`; the reveal settings for `markdown.symbols`, the link-card metadata parser and callout folds (now with heading folds, `render::folds`) into `gasp-core`; remembering note positions into `gasp-config`; the grammar ignore file's format into `gasp-prose`; the code highlighter's scope table into the new `gasp-highlight`; the Keychain store (now covering iOS, behind `gasp-sync`'s `keychain` feature) and the sync phases and their sentences (`gasp_sync::phase`) into `gasp-sync`. The desktop re-exports them. The search crate gained `ocr`, the cache of recognised text and its search. The registry gained `format.callout` (Obsidian's Insert callout, bound on the desktop too), `keyboard.hide` and `tab.overview` on iOS, Look up on iOS as well as macOS, and on iOS `fold.toggle` (`Cmd+Alt+[`), `fold.all` and `fold.unfold-all`.

**Drawing.** A `UITextView` on TextKit 2 keeps the note's exact source in its storage and styles it from the core's plan: fonts and colours from the theme tokens (Charter, Courier New, heading scales, line heights), hidden markup drawn tiny and clear, and only lines whose plan changed restyled. `NSTextContentStorageDelegate` draws substitutes of the same length (a bullet for `-`, an SF Symbol checkbox for `[x]`, tabs between table cells, a picture for the first character of math or an image) and leaves out collapsed lines; an `NSTextLayoutFragment` subclass draws code block and callout surfaces, quote bars, rules, a table's header line, pictures in room a line makes above or below itself, the grammar underlines and a heading's fold control. Checkboxes toggle on a tap.

- *Math.* Inline `$…$` and `$$…$$` blocks render in place through `render_math`: mitex and Typst lay the equation out, resvg rasterises it at the screen's scale as coverage (one byte a pixel), and the phone fills it with the text colour, so one render serves light and dark mode. It's an attachment on the text's baseline at the size of the line's text (a heading's math is heading-sized), and a line holding a tall one grows. The cursor entering math shows its TeX, with the live preview above an inline equation or below a block, as on the desktop. `MathImages` caches renders (48 MB of coverage) and renders on up to four threads; a note asks for all its equations as it opens, nearest the cursor first, and lines restyle in batches as renders land, showing the TeX until then. The 705 equations of a 1,600-line corpus note render in about a second in the simulator (release core, debug app), and scrolling it drops no frame over 25 ms while it moves.
- *Images.* `![[…]]`, `![](…)` and `<img>` from the vault draw at the width the note asks for (`|300`) or their own, never wider than the column, with rounded corners. The file's pixel size comes from its header, so the text lays out at the right height at once, and the pixels are decoded off the main thread at the drawn size (ImageIO thumbnails, 64 MB cache). With the cursor in the embed its source shows and the image sits below it.
- *Tables.* Tables that fit are laid out in columns with tab stops and edited in place. A table wider than the screen becomes a grid over its rows that scrolls sideways: the rows keep their lines (hidden, one line each) and the grid draws each cell's styled text, math included, the header bold, hairlines between rows. A tap edits a cell in a field over it; Return goes to the cell below and Tab to the next (adding a row past the end), and the text goes back through the core, which escapes pipes and pads the table, as one undo step. A long press shows the desktop's menu: inserting rows and columns, then Row, Column and Table submenus, with what doesn't apply greyed out. A cursor arriving in the hidden rows opens the cell it's in.
- *Folding.* A heading with something under it has a chevron in the margin; a tap folds its section up to the next heading of its level, and `fold.toggle`, `fold.all` and `fold.unfold-all` do it from the palette or keys. A foldable callout folds from its icon. A cursor in a folded section opens it again, so folding moves the cursor to the heading.
- *Prose.* The desktop's grammar layers underline problems with a wavy line (red for spelling, blue for the rest) in the paragraphs around the screen, checked off the main thread half a second after typing or scrolling stops. A tap on an underline shows a card with the message, the suggestions to apply and "Ignore everywhere", which writes the shared ignore file. The system's own spell check is off while the checker is on.
- *Cards and code.* A tap on a footnote number (while reading) shows the footnote's text with a link to it; link cards show their image at the right, downloaded once into the app's caches; fenced code with a language is coloured with the theme's `color.code.*` tokens through `gasp-highlight`, as on the desktop.

**The browser.** The app works like Safari rather than a list you drill into:

- *Tabs.* Several notes stay open, each with its own editing session (cursor, scroll, undo) and back and forward history. The bar at the bottom is the `browser-bar` toolbar: the note showing sits where its items have the spacer, and by default there's only the sidebar button on its left and the tab overview (with the tab count) on its right; sync's indicator moved to the sidebar's foot, and can go back on the bar as the `sync` widget. The titles sit on a track: dragging one follows the finger with the neighbouring tab's title sliding in, and letting go past a third of the way (or with a flick) settles on that tab with a spring. Nothing wraps round: before the first tab the title only gives a little (rubber-banded) and springs back, and past the last one waits a new tab on the start page, as in Safari. VoiceOver swipes up and down to change tab. Tapping the title opens the overview, a grid of note cards to switch to, close or add. A new tab opens on a search and the recent notes. Open tabs are saved per device in `.gasp/device.toml`, which never syncs. The bar hides while the keyboard is up.
- *Sidebar.* A swipe in from the left edge (or the bar's sidebar button) slides it over the note, above a dimmed backdrop that a tap or swipe closes. It holds the vault search (by name, text, or `tag:`), the file tree, the note's outline, its backlinks and outgoing links, and the vault's tags, with new note, today's note, the sync indicator (for the synced vault) and settings at its foot. There's no navigation stack, so nothing else claims that edge.
- *Keyboard bar.* An `inputAccessoryView` above the software keyboard whose buttons are the `keyboard` toolbar's items in `toolbars.toml` (commands, separators, spacers and menus; the status widgets are left out), read through the FFI's `keyboard_toolbar()`, in a row that scrolls sideways and fades out under the hide-the-keyboard button, which stays fixed at the right end whatever the items say. The default is Obsidian's mobile toolbar (insert image, indent, outdent, callout, inline math, footnote, sentence highlighting, table), then find, bold, italic, highlight, link, inline code, task, and the palette. `enabled = false` takes the bar away and `style` can add labels. A vault's old `mobile.toolbar` setting moves there the first time the vault opens.
- *Undo and redo.* A small pill floating at the right edge just above the keyboard bar while a note is typed in, rather than bar items. Each button greys out to `color.icon-disabled` and stops taking taps when there's nothing to undo or redo, following the text view's undo manager as it changes; the text view keeps room under the cursor for it.
- *Toolbar settings.* Settings has a Toolbars section (and `toolbar.customize` opens it from the palette) with a page per phone bar: for the keyboard bar a switch and how its buttons read, then the bar's items in a list in edit mode (drag to reorder, the red minus to remove; the bottom bar's note title can move but not go), "Add a button" with a searchable picker of every command the phone runs by category plus lines, gaps, menus and (on the bottom bar) sync's status, and a reset once it differs from the built-in bar. It writes through the core's `gasp_config::toolbar_files` writers via the FFI (`phone_toolbars`, `toolbar_choices`, `set_toolbar_items`, `set_toolbar_labels`, `set_toolbar_enabled`, `reset_toolbar`), so the change syncs to the desktop like any setting, and the desktop's Toolbars page shows both bars too.
- *Palette.* Every command the phone has, searchable, grouped by category, with its hardware key.
- *Find.* UIKit's find navigator, with next, previous and replace.
- *Hardware keyboard.* The desktop keymap's iOS bindings become `UIKeyCommand`s on the text view, which answer first while a note is edited (Command-B, I and U also come through UIKit's own formatting actions), and hidden SwiftUI shortcut buttons elsewhere, which list the keys in the overlay shown while Command is held. Arrows, deleting and plain Tab and Escape stay with the text view.
- *Where you were.* Each note's cursor, the line at the top of the screen and its folded headings are kept in `device.toml` (never synced) when the app goes to the background, and restored when the note opens.
- *Search.* Vault search also finds notes by the text in the images and PDFs they embed: Vision reads each file once in the background (a PDF's own text layer when it has one), and the hit names the file and quotes the match.
- *Reading.* While the keyboard is away, a tap on a link follows it: web links open in the browser, note links open the note (making it if it doesn't exist) at its heading.

**Sync.** The phone syncs the vault the way the desktop does, through `VaultSync`, and nothing else syncs it: once it's set up, GitSync and its Shortcuts automations can go.

- *Setup.* The start page's "Set up sync" (or Settings, or `sync.now` before sync exists) asks for the repository (`you/notes`, `github.com/you/notes` or a full address), the branch (`master` unless changed) and a fine-grained token, in a secure field with a paste button and a link to GitHub's token page. It clones into `Documents/Synced/<name>`, keeps the token in the Keychain under the repository's address (as the desktop does), writes the branch into `sync.branch` if the vault named another, and opens the clone. The clone's place is kept relative to `Documents`, so it survives the container moving on an update.
- *Vaults.* The synced notes open from then on. The bundled sample notes are only the fallback when nothing syncs, and Settings switches between the synced notes, the sample notes and a folder picked in Files. Sync only ever runs on the clone it set up, never on the sample or a picked folder another app may be syncing.
- *When.* On opening, coming to the front and going to the background (with background time so the push can finish), from a `BGAppRefreshTask` (`com.borisnezlobin.editor.sync`, no sooner than the interval or 15 minutes), and after an edit on the desktop's idle schedule: a saved note, a new, renamed or trashed note, or a setting change counts. Open notes are saved first, and the git work runs on a background queue.
- *Notes a sync changed.* Open tabs whose note changed are read again, keeping the cursor. Edits not saved yet are folded into what sync wrote with the sync merge policy; if they clash on the same lines, the tab keeps its text and saves over the note.
- *Indicator.* A symbol in the tab bar for the desktop's phases: synced, syncing (turning, unless Reduce Motion is on), offline with a count of changes waiting, conflict with a count of notes, sign in, needs setting up, and failed. A tap opens the details: the core's headline and explanation, Sync now, Resolve conflicts or Open sync settings when those are the way forward, recent syncs with what came in and went out, and the repository and branches.
- *Resolver.* Each waiting note (a list first when there are several) shows every place with two lines of context, this iPhone's lines and the other device's in their theme colours, and four choices: keep this iPhone's, keep the other's, keep both, or edit (prefilled with both). A kept side gets a check and a stronger fill, a dropped one fades with a strikethrough. Saving settles the note through the core, which refuses if the note changed since it was shown, and syncs it at once.
- *Settings.* The repository, branch, legacy branch and interval, signing out (with a confirmation) or pasting a new token; `sync.auto` and `sync.device-only` stay in the generated list.
- *Testing it.* `-syncRepository <url> -syncBranch master -syncToken <token>` opens the setup filled in, and `-syncStart YES` clones straight away; `editor://sync/setup?repository=…&branch=…` fills it in without cloning. `-run sync-details`, `-run sync.now` and `-run sync.resolve-conflicts` open the sheets, and `-resolverChoice mine|theirs|both|edit:<text>` makes the resolver pick that in every place and save.

**Commands.** `CommandRunner` runs every command in the registry: editing commands through the core, the rest on the browser. The phone leaves out the `pane.` commands, since it shows one note at a time.

| Group | On the phone |
|---|---|
| Formatting, footnotes, lines, tasks, tables, callouts, links | The core's commands on the note, as one undo step each |
| Find | The system find navigator |
| Tabs and history | Tabs, the overview, back and forward, reopening a closed tab |
| Sidebar, file tree, outline, backlinks, tags, search, switcher | The sidebar, opened on the right section or with the search focused |
| New, daily, rename, trash, recover, template, import image | The vault operations above; recovery lists snapshots and restores one after keeping the current text; images come from Photos |
| Export and print | HTML or PDF (Typst, reading the iPhone's system fonts) to the share sheet; the PDF to the print panel |
| Settings, another vault | A settings screen generated from the schema, with sync and the vault switcher on top; a folder picked in Files, kept with a security-scoped bookmark |
| Sync now, resolve sync conflicts | A sync (or the setup, before sync exists); the resolver |
| View | Zoom per device, readable line length on wide screens, cycling Markdown symbols in every open note, sentence-length tints |
| Editing and the clipboard | Undo, redo, copy or cut the line, paste, paste as plain text, select all, Look up, hide the keyboard |
| Cursor and selection keys | The text view's own |

**Verified** in the iPhone 17 Pro simulator (iOS 26.4), headless, with `simctl io screenshot` and a throwaway XCUITest that taps, swipes and types keys: the start page and recent notes; notes with headings, emphasis, links, footnotes, tasks, callouts, quotes, code and tables; markup showing around the cursor; the sidebar's files and outline; the tab overview; the palette; the keyboard bar in the Obsidian order; find and replace; a callout inserted through the core and saved to disk; sentence tints; the settings screen; printing a Typst PDF; rename; dark mode. Then: inline and block math on the baseline in light and dark mode, the block's source and preview while editing it, math in a grid cell; vault images at their width; a wide table's grid scrolling, a cell edited and Return moving down, the long-press menu; code colours and a link card's image; a callout opened from its icon and a section folded with `Cmd+Alt+[`; the grammar card applying a suggestion; the footnote card; the edge swipe opening the sidebar and a swipe on the tab bar changing tab; `Cmd+P`, `Cmd+I`, `Cmd+U` and `Cmd+K` reaching their commands; the reading position surviving a relaunch; a note found by the words in an embedded image. Not verified: `Cmd+B` and Escape (the simulator's test harness never delivers them to the app), and a run on a real iPhone.

The owner's first hands-on round (the pinned hide button, block indenting, toolbar settings, the undo pill, the tab bar's swipe and a smaller bottom bar) was checked headless with screenshots only, since no XCUITest ran: the keyboard bar with the hide button pinned and the row fading under it, with icons and with labels from a vault's `toolbars.toml`; a list item nested by `edit.indent` with the caret kept on its text, and the undo pill turning from greyed to enabled; the bottom bar's default items and a vault's own (history back beside the sidebar); a drag shown part of the way to the new tab past the last one and rubber-banded before the first; and the Toolbars settings pages. Launch arguments for this: `-editing YES` puts the cursor in the note with the keyboard up, `-tabBarDrag <points>` starts a drag (a leftward one as `'<real>-130</real>'`, since a bare `-130` reads as another flag), and `-run toolbar.customize -toolbarPage keyboard` opens a bar's settings page. Not verified by hand: the swipe's feel and its spring, dragging rows in the settings list, the delete buttons and the picker's taps. The core's indent tests cover the Markdown side (`crates/core/src/commands/indent/tests.rs`), and the FFI's toolbar tests the settings writers.

Sync was verified the same way, in a simulator of its own against a local bare repository reached as `file://`, with a second clone as the laptop and a third on `main` as the old tool, checking every result in the repositories: setup cloning `master` from `main` and pushing it; the token read back from the Keychain across a dozen relaunches; an edit on the phone (the core's toggle-task) committed as `iphone: Groceries.md` and pushed a minute after it stopped; the laptop's edit arriving in the open note on going to the background and back; a commit on `main` merged one way into `master`, with `main` left alone; an unsaved phone edit folded into a note the launch sync changed; a conflict parked (the laptop's line on `master`, both on disk between markers, the indicator counting one note) and resolved with an edit, with keep both and with keep this iPhone's, each pushed; offline with one change waiting, then pushed once the repository was back; switching to the sample notes. `cargo test -p gasp-sync -p gasp-ffi` covers the same through `VaultSync`. Not verified: HTTPS to GitHub (only `file://` was reachable), the Keychain on a real iPhone, `BGAppRefreshTask` actually firing (the simulator doesn't run them), and the syncing phase on screen (a local sync is over before a screenshot).

**Still to do in this phase:**

- Sync against GitHub from the owner's iPhone: set it up there and watch one sync each way, then remove GitSync and its automations. A progress bar for the first clone of the 252 MB repository, which now shows only a spinner.
- The grammar model and proofreading (layers 3 and 4), web images in `![](https://…)`, dragging a table's rows and columns, and the edit-time stats file.
- Tables that match the desktop's. The phone's grid (`apps/ios/Gasp/Tables/TableGridView.swift`) pads cells by `spacing.md`, draws only a hairline under each row in `divider` or `fill`, sizes columns from their text with no minimum, and marks the cell being edited with a rounded `fill`. It should read the desktop's `table.*`, `color.table.*` and `opacity.table-*` tokens, passed through `Tokens` in `crates/ffi`: square rules around and between cells, the header's fill and heavier rule, the least column width and row height in lines of body text, and the cell being edited as a square tint with an inset ring in the accent.
- A UI test target, `SwiftLint` and a simulator build in CI.

### Phase 8: plugins and agents

This covers the TypeScript plugin API, permissions, hot reload, the package format (spec, code and tests), and an in-app "describe a feature" flow that hands the request to a coding agent over MCP.

## Assumptions

These are my defaults. Tell me if any are wrong.

- Hover page preview and link cards stay. Excalidraw, Bases, the web viewer and graph view are dropped.
- Your Harper ignore list isn't migrated, because Harper stores it as hashes of the surrounding text, which can't be converted back into rules. The personal dictionary does migrate.
- The `Mod+M` hotkey for Obsidian Hider (no longer installed) is dropped. The rest of `hotkeys.json` is imported.
- Plugins are written in TypeScript.
- Default font sizes come from your current settings (base size 12) and can be changed like everything else.

## Open questions

1. **Apple Developer account (needed by Phase 7).** Without the paid account, an app you install on your own iPhone stops launching after 7 days and has to be reinstalled from the Mac. Do you have one, or want one?
2. **Pane focus keys on Linux.** `Mod+Alt+Left` and `Mod+Alt+Right` become Ctrl+Alt+Left and Right on Linux, which GNOME has used to switch workspaces. Should Linux get different default keys for moving focus between panes?
3. **Device-only files the repo already tracks.** Sync never commits changes to device-only files, but it leaves any that are already in the repo alone, because removing them would delete them on your other devices too. Should the first sync remove them from the repo?

## Decisions

- **HTML export** emits clean semantic HTML with MathML, and the site's CSS is updated to match.
- **Chronotyper frontmatter** is imported into stats files and then removed from the notes.
- **Audience** is just you for now, so there's no store listing, marketplace or public release work yet.
- **PDF drop cap** is off by default.
- **Sync branch** is a new `master`, fed one way from `main` until the old tools are retired (see [Sync](#sync)).
- **Grammar** aims to replace your pre-publish Claude pass with an offline checker, chosen by measuring false flags first.
