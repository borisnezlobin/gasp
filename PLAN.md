# Editor plan

A native, very fast Markdown editor for your Obsidian vault. It runs on macOS, Windows, Linux and iPhone, syncs through GitHub, and can be customized at every level, including behaviour. "Editor" is a working name until we pick a real one (see [Open questions](#open-questions)). Until then, the crates, the binary and the config folder (`.editor/`) use `editor`.

## Start here

This section is for an agent picking the project up with no earlier context. Everything below it is the design.

### Where things stand

The plan is complete. No code exists yet, and the next step is Phase 0. This repo used to hold an abandoned Electron editor, which is still in the git history and should be ignored.

| Phase | Status |
|---|---|
| 0 Spikes | not started |
| 1 Core | not started |
| 2 Desktop editor | not started |
| 3 Sync and travel check | not started |
| 4 Search and prose | not started |
| 5 Export | not started |
| 6 MCP and headless modes | not started |
| 7 iPhone | not started |
| 8 Plugins and agents | not started |

Update this table, and the phase's section, when work lands.

### What you can and can't reach

A cloud session is probably a Linux container with this repo cloned. It can't see the owner's Mac.

You can reach these:

- **This repo** (`borisnezlobin/editor`), where all the code goes.
- **[Flo State](https://github.com/Altimor/flo-state)** is the native Swift editor that inspired this project. Read its `oracle/`, `fixtures/` and `Sources/FloCore/Render/RenderPlanner.swift` for the parity-testing and render-planner ideas. It's GPLv3, so read it for ideas and don't copy its code.
- **[PDF Export Plus](https://github.com/borisnezlobin/obsidian-pdf-export-plus)** is the owner's PDF plugin, which Phase 5 ports to Typst.
- **[The owner's website](https://github.com/borisnezlobin/website)**. `scripts/publish-article.mjs` and `app/styles/` describe the current HTML export flow that Phase 5 replaces.
- **Library docs and source** for GPUI, Typst, mitex, harper-core, Tantivy, git2, rquickjs and UniFFI.

These stay out of reach:

- **The notes vault and the private `borisnezlobin/notes` repo.** Never clone, read or push to it. Sync work uses local bare repos in tests, plus a throwaway repo the owner provides for network tests.
- **The owner's Obsidian config**, including the 212 Latex Suite snippets and the prettifier table. Settings that matter are quoted in this plan. Build the migrator against Obsidian's documented file formats with synthetic fixtures, and the owner runs it on the Mac against the real `.obsidian` folder.
- **`vault-sync` and Footnotes Plus.** Both exist only on the owner's Mac. Their behaviour is described in [Sync](#sync) and [Footnotes](#footnotes), and that description is the spec.
- **Obsidian itself**, which the parity oracle drives. Try running it on a macOS GitHub Actions runner against the synthetic corpus. If that doesn't work, the owner runs the oracle locally.

Steps marked **[Mac]** need the owner's machine or a macOS runner.

### First tasks

1. **Workspace and CI.** Create the Cargo workspace from [Repository layout](#repository-layout) and a GitHub Actions workflow that builds, tests and lints on `ubuntu-latest`, `windows-latest` and `macos-latest`. It's done when an empty crate passes on all three.
2. **Synthetic corpus.** Write a generator for a fake vault that uses every feature in [What your vault actually uses](#what-your-vault-actually-uses), in similar proportions. Math-heavy notes, callouts, footnotes, tables, tasks and raw HTML matter most. Every committed test fixture comes from this corpus.
3. **Phase 0 spikes**, each on its own branch with a short result written into the spike table: what passed, the measured numbers, and the decision. The GPUI, math, Typst and git2 spikes can run on Linux and CI. The iPhone spikes run on a macOS runner.

### Working rules

- **Complexity.** A cyclomatic complexity limit of 15 per function is a hard requirement, treated like a failing test. Enforce it in CI with Clippy's `cognitive_complexity` lint (threshold 15, set to deny) plus a cyclomatic check using `rust-code-analysis-cli`, and use SwiftLint's `cyclomatic_complexity` rule at 15 on the iPhone app. Split functions, use early returns and lookup tables rather than raising the limit.
- **Code style.** Names should explain the code. Comments are only for genuinely tricky logic.
- **Files.** Don't write summary Markdown files or backup copies of files. Git is the history.
- **Private data.** Never commit anything from the owner's notes. Fixtures recorded from the real vault stay on the Mac.
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
| Raw HTML tags (mostly `<br>` 326 and `<hr>` 312) | 370+ | 25 |
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
  prose/      sentence segmentation, sentence-length highlighting, grammar (Harper + second pass)
  snippets/   snippet engine, Latex Suite migrator
  math/       LaTeX → Typst conversion and layout for the editor
  export/     HTML (article body + publish), PDF (Typst)
  mcp/        MCP server
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

Everything the app does is described by files in a config folder inside the vault (working name `.editor/`). They sync with your notes, reload the moment they change, and are what the settings screen reads and writes. Settings that belong to one device, such as window size and open tabs, live in a separate file that doesn't sync.

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

## Features

### Editor

- **Markdown symbols** (`**`, `#`, `[]()`, `$`, `>` and so on) can be shown or hidden, and switching is one command with a hotkey. There are three modes:
  - *Always shown*, like a plain source editor.
  - *Revealed around the cursor*, like Obsidian's live preview and Typora. The reveal scope is configurable: only the element the cursor is in (Typora), the whole line, or the whole block.
  - *Always hidden*, so the note reads like a finished page while staying editable.

  Each kind of syntax can override the mode. For example, link URLs can stay hidden while emphasis marks follow the cursor. Because the mode is a setting, rules can switch it too, such as showing everything while a search-and-replace is open.
- **Blocks.** Headings with folding, lists, task lists, tables with a grid editor, callouts (all Obsidian types, foldable), code blocks, block and inline math, images and embeds, `%%` comments, highlights, raw HTML (`<br>`, `<hr>`, `<div>`, `<img>` rendered inline), and frontmatter.
- **Math.** It's rendered in place and reveals its source when the cursor enters. There's a live preview above the line while you edit, matching bracket colours, and concealed commands (Latex Suite's conceal). See [the math spike](#phase-0-spikes).
- **Code blocks** get syntax highlighting, an optional title (`title:` in the fence line, replacing Embedded Code Title), line numbers, highlighted lines, and a copy button. Their appearance is themed through tokens rather than Code Styler's separate theme system.
- **Links.** Markdown links and wikilinks both resolve, hovering shows a page preview, and renames update every link.
- **Formatting commands** toggle: with a selection they wrap or unwrap it, with no selection they act on the word under the cursor, and on an empty spot they insert the pair with the cursor between. Markdown has no underline syntax, so underline uses `<u>…</u>`, which the editor, both exports and the site all render. Default bindings are in [Default keymap](#default-keymap).
- **Tabs.** Opening a link uses a new tab, or switches to the tab that already has that note (replacing Opener).
- **Pasting images** saves them to `./images` named after the note, the way Paste Image Rename is set up now.
- **The status bar** shows word count (for the selection when there is one), reading time and edit time.

### Default keymap

`Mod` is Cmd on macOS and Ctrl on Windows and Linux. Every binding is a rule, so each can be changed or removed. The first group is standard formatting, and the rest carries over the owner's current Obsidian hotkeys.

| Keys | Command |
|---|---|
| `Mod+B` | Bold (`**`) |
| `Mod+I` | Italic (`*`) |
| `Mod+U` | Underline (`<u>`) |
| `Mod+K` | Insert or edit link |
| `Mod+E` | Inline code |
| `Mod+Shift+X` | Strikethrough |
| `Mod+Shift+H` | Highlight (`==`) |
| `Mod+Shift+M` | Inline math |
| `Mod+P` | Command palette |
| `Mod+O` | Quick switcher |
| `Mod+Shift+F` | Search |
| `Mod+Shift+R` | Search and replace |
| `Mod+J` | Toggle sentence-length highlighting |
| `Alt+0` | Insert or jump to footnote |
| `Mod+Shift+P` | Export PDF |
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

Your 212 existing snippets are migrated automatically, and about 170 of them are plain triggers. Most of the 42 regex ones only use regex for "whole word followed by a space" (for example `/(?<![\\A-Za-z])forall $/`), and they convert to the `whole word, after space` options. The 3 JavaScript-function snippets and any regex the migrator can't express are listed for you to review.

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

This replaces Chronotyper without touching frontmatter. Each device writes its own stats file under `.editor/stats/`, so two devices never conflict, and the app sums them. The `edited_seconds` values in your 49 existing notes are imported. After that, the migrator removes the `updated` and `edited_seconds` keys from those notes, and deletes a frontmatter block when nothing else is left in it. That happens in a single commit, so it's easy to revert.

### Link cards

Pasting a URL on an empty line can turn it into a preview card, as Link Embed does now, with metadata fetched locally. This is a later-phase feature.

## Sync

### What exists today

Your vault is already a git clone of `borisnezlobin/notes` on branch `main`. Your `vault-sync` daemon (a Node program on the owner's Mac, not on GitHub, run by the launchd agent `com.randomletters.vault-sync`) commits a minute after you stop typing, merges, and pushes. The iPhone uses GitSync triggered by Shortcuts automations. The repo's default branch is `fake-default-lol`, and there is no `master` branch, so the app targets `main`.

### Design

The app does the same job natively on every device, including the iPhone, using libgit2 through the `git2` crate. Keeping real git everywhere means the app coexists with `vault-sync` and GitSync during the transition, keeps full local history, and needs no custom server.

- **Setup** asks for the repo, the branch and a fine-grained GitHub token limited to that one repo. The token is stored in the Keychain on Apple platforms, Credential Manager on Windows, and the Secret Service on Linux.
- **Offline editing** works naturally, because edits are committed locally and pushed when there's a connection.
- **Sync timing** follows `vault-sync`: commit about a minute after you stop typing, then fetch, merge and push. The iPhone also syncs when the app opens, returns to the foreground or goes to the background, so the Shortcuts automation isn't needed.
- **The merge policy** is ported from `vault-sync`. Notes merge line by line and changes that don't overlap merge automatically. Images and binaries keep the local copy. Device-only files never sync.
- **Real conflicts** open a native resolver instead of the `localhost:7777` page. You get each clashing hunk side by side and pick this device, the other device or both, or edit the merged text directly. Until you resolve it, the note stays editable with both versions visible.
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

Your current settings carry over: A4 pages, 18/16/12/12 mm margins, page numbers, 8.5 pt footnotes, and the professional style in Iowan Old Style at 12 pt with line height 2 and a 3-line drop cap. Page breaks from the Break Page plugin's markup are honoured, and header and footer templates from Better Export PDF become an option. The live preview renders pages as you change settings.

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
- **Code quality.** Clippy with a cyclomatic complexity limit of 15, and SwiftLint with the same limit on the iPhone app. A complexity failure blocks a merge like a failing test.

## Phases

Each phase ends with something you can use and a check that proves it works.

### Phase 0: spikes

These are small throwaway experiments on the riskiest assumptions, each with a fallback if it fails.

| Spike | Passes if | Fallback |
|---|---|---|
| GPUI live-preview editor | Mixed font sizes, an inline image widget, selection and IME (Japanese input) work, typing stays under a frame on a long note, and it runs on all three desktops in CI | Slint or Makepad for desktop |
| Math via `mitex` → Typst | It renders the roughly 4,000 equations in your vault, and a typical one takes under 1 ms when cached and under 20 ms when new | KaTeX in a hidden web view on each platform, as Flo State does |
| libgit2 on iPhone | Clone, commit, merge and push to `borisnezlobin/notes` with a token from the simulator and a device | GitHub's REST API with our own three-way merge |
| QuickJS on iPhone | A sample plugin runs and hot-reloads | Declarative plugins only on iPhone |
| Typst PDF | It reproduces one of your existing PDF Export Plus exports with page-bottom footnotes and the drop cap | Paged.js in a hidden web view at export time |

### Phase 1: core

This covers the document model, transactions and undo, the parser with Obsidian extensions, the render planner, the config and rules engine, the command registry, the parity oracle, and the migrator for your `.obsidian` settings, hotkeys and snippets. It's done when the parser and render planner match the Obsidian fixtures for the synthetic corpus in CI, and for the real vault on the owner's Mac **[Mac]**.

### Phase 2: desktop editor

This is the GPUI app with the file sidebar, tabs, quick switcher, command palette, live preview for every block type in your vault, replacements, emoji, snippets, footnotes, math, code blocks and images. It's done when you can use it as your daily editor on the Mac, and CI produces working Windows and Linux builds.

### Phase 3: sync, then travel check

This builds git sync, the conflict resolver and the setup flow, and replaces `vault-sync` on the Mac. It's done when you've installed the app on the Windows/Ubuntu laptop, synced the vault on both systems, and edited offline and merged. This is the checkpoint that has to pass before your next trip. The app is only for you for now, so desktop builds are signed for personal use and there's no store listing.

### Phase 4: search and prose

This covers search with OCR, sentence-length highlighting, word count, reading time and edit-time tracking. Grammar starts with the evaluation set described under [Grammar](#grammar), then the layers are built in order, and the model is picked from the scores.

### Phase 5: export

This covers HTML export with one-step publish to your site, the one-time stylesheet rewrite and re-export on the site, and PDF export with a live preview. It's done when a new article and a re-exported old one both look right on the site, and a PDF matches your current output.

### Phase 6: MCP and headless modes

This covers the MCP server with full read and write access, the CLI modes and screenshots.

### Phase 7: iPhone

This is the SwiftUI and TextKit 2 app on the same core, with your mobile toolbar (attach file, indent, unindent, callout, inline math, footnote, sentence highlighting, table), sync on open and close, search and export. It's done when it replaces Obsidian and GitSync on your phone.

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

1. **Name.** What should it be called? The name sets the config folder, the bundle ID and the repo name.
2. **Apple Developer account (needed by Phase 7).** Without the paid account, an app you install on your own iPhone stops launching after 7 days and has to be reinstalled from the Mac. Do you have one, or want one?

## Decisions

- **HTML export** emits clean semantic HTML with MathML, and the site's CSS is updated to match.
- **Chronotyper frontmatter** is imported into stats files and then removed from the notes.
- **Audience** is just you for now, so there's no store listing, marketplace or public release work yet.
- **Grammar** aims to replace your pre-publish Claude pass with an offline checker, chosen by measuring false flags first.
