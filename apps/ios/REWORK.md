# iPhone rework brief

The owner tried the iPhone app and isn't happy with it: "it's significantly worse than the desktop app." This brief lists everything they raised, with causes found in the code where there are any, and the direction to take. The bar they set: **nothing should need reading to be understood.** Every screen must make it obvious what it is and what you can do, from its shape alone. Nobody reads help text to figure out an app.

Work on a Mac with the simulator, and look at every screen you change in light and dark, before and after (`apps/ios/scripts/run-simulator.sh`, then screenshots). Follow PLAN.md's UI rules and the working rules: cyclomatic complexity ≤ 15 (SwiftLint), tokens from the theme rather than values at call sites, sentence-case labels, no all-caps, no letter-spacing. Delete this file when the work is done.

The Rust side is ready (commit "FFI: folders, moving notes and the appearance setting on the phone"). `VaultFolder` now has `move_note(path, folder)`, `create_folder(parent, name)`, `rename_folder(folder, name)`, `move_folder(folder, into)`, `trash_folder(folder)` and `appearance() -> Appearance { Light, Dark, System }`. Rebuild the bindings with `apps/ios/scripts/build-core.sh`.

## 1. Home: what a new tab shows

Today a new tab shows a search box, three pill buttons ("New note", "Today", "Set up sync") and a "Recent" list where every note carries its full folder path, so the list is mostly grey path text. You can't tell what you're looking at.

A new tab is a home screen, as in Obsidian's mobile app: a short column of large, plainly labelled actions, and nothing competing with them.

- **Actions, in this order:** "Search notes" (opens the note switcher with the keyboard up), "New note", "Today's note". Make each a full-width row with an icon, not a pill. They're the screen's subject, so make them the most prominent thing on it.
- **Recent notes, below:** titles only, a handful. If a folder is shown at all, it's the note's immediate folder as a small muted label, never the path from the vault root. Two notes with the same title are the only case that needs more.
- **Sync:** remove the "Set up sync" button from home. Sync lives in settings and in the tab bar's indicator. At most, show a single quiet row when sync is broken.
- **No title on home saying "New tab".** The tab bar at the bottom says "New tab"; that's the bar's job. Check whether the bar's centred title helps at all, or whether the note's title belongs there.

## 2. Settings

Screenshots from the owner show:
- **Theme said "Light" while the app was dark.** The app never reads `appearance.theme`: `Tokens` always follows the phone's trait (`apps/ios/Gasp/Theme/Tokens.swift`). Apply `vault.appearance()` with `.preferredColorScheme` / `overrideUserInterfaceStyle` on every window and sheet, and update it when the setting changes.
- **Values are indistinguishable from placeholders.** "Folder" and "Template" show grey placeholder text in the value's place, so you can't tell what's set, what's editable or what's a hint. The stepper sits on top of the description text. The spacing is uneven.
- **Every row has a sentence under it.** Much of it explains things that shouldn't need explaining, and some of it is confusing to someone who doesn't know the feature, e.g. "sentences with more words than this are long".
- **The mobile toolbar setting is a raw list of command IDs** (`keyboard.hide`, `note.import-image`, …) typed into a text field. No one can be expected to edit that.
- **The keyboard can't be dismissed** once a text field in settings has focus.

Direction: organise settings the way every iPhone app does, as in the owner's reference (ChatGPT's settings sheet). That means a short top level of grouped rows with an icon and a name each, where each row pushes to its own page:
- **Top level:** Appearance, Editor, Daily notes, Sync, Keyboard toolbar, Files, About. Pick the groups from the schema's sections, merging small ones. Sync goes first while it isn't set up.
- **Within a page:** standard iOS rows. A toggle for booleans, a menu or pushed picker for choices (showing the chosen value in the row), a stepper with its value for numbers, and a pushed text screen for free text. A value that's set shows in the primary colour; unset shows "None", or the default, in the secondary colour. Don't use a placeholder to stand in for a value.
- **Descriptions:** drop the sentence under each row. Where a setting truly can't be understood from its name, rename it, e.g. "Long sentence: more than N words" with the number in the row. Only then consider a short footer on the section.
- **Keyboard toolbar:** a list of the toolbar's commands shown by their names and icons (`CommandSymbols`), reorderable by drag (`.onMove`), with swipe to remove and an "Add command" row that pushes a searchable command list. Never show command IDs.
- **Keyboard:** `.scrollDismissesKeyboard(.interactively)` on every settings page, and a Done button in the keyboard toolbar for text fields. Audit the whole app for any other place the keyboard can get stuck.

**Sync with GitHub** has the same problem: a wall of text, and it's unclear what to do unless you already know. Make it a short sequence where each step is one thing: "Sign in to GitHub" (a button), then "Choose a repository" (a list), then a done state. Explanations go only where a step can fail, shown when it fails.

## 3. Settings across devices

`settings.toml` and `theme.toml` (the accent colour) are synced; only `device.toml` isn't. The owner has light mode and a red accent on the desktop but saw dark and not-red on the phone:
- **Light mode:** the phone ignoring `appearance.theme` (section 2) explains it.
- **Accent:** the phone does read the synced theme, so its vault copy may not have the desktop's latest `.gasp/theme.toml`. Check the phone's copy after a sync. Also check that the desktop really writes the accent to `.gasp/theme.toml` and not a legacy `.editor/` folder, now that the app is Gasp.

## 4. Sidebar

- **Moving notes:** drag a note onto a folder to move it (`move_note`), with the folder highlighting as a drop target. Folders drag too (`move_folder`). The context menu gets "Move to…", which pushes a folder picker, plus "New folder", "Rename" and "Delete" for folders.
- **Haptics:** throughout the app, where iOS apps use them. That covers opening and closing the sidebar at its detent, picking up and dropping in a drag, a successful move, checking a task, a snippet expanding, a sync finishing or failing, and reaching the end of a tab swipe. Use `UIImpactFeedbackGenerator` / `UISelectionFeedbackGenerator` / `.sensoryFeedback`, and keep them light. There should be none on ordinary typing or scrolling.

## 5. The note screen

- **No title.** The note shows no title. Show the note's title the way the desktop does (a large editable title above the text, renaming the file on commit).
- **Frontmatter** is rendered poorly. Replace it with a collapsed "Properties" chip above the title, showing the number of properties. Tapping it expands a tidy key/value list you can edit (values by type: text, list, date, checkbox). Collapsed is the default and is remembered per device.
- **Tab swipe:** swiping along the bottom bar doesn't create a new tab at the edge. Also, the title and the path in the bar move with different elasticity, so they visibly separate. Fix both. The bar's content moves as one piece, and a swipe past the last tab rubber-bands and opens a new tab (home) on release, with a haptic at the threshold.
- **Footnotes:** tapping a footnote reference sometimes shows the preview card and sometimes puts the cursor inside `[^1]`. The cause is `tapAction(at:)` in `apps/ios/Gasp/Editing/EditingController+Taps.swift`, which returns nil for footnotes while the text view is first responder (`guard !textView.isFirstResponder else { return nil }` sits before the footnote check). A tap on a footnote reference should always show the card, keyboard up or not. Editing the reference is reached from the card or by placing the cursor with the arrow keys or a long press. Check links and the other tap actions for the same inconsistency.

## Done means

Every screen above has been looked at in the simulator in light and dark, and the owner can find their way around the app without reading a sentence of it. Report back with before and after screenshots of home, settings (top level and one page), the toolbar editor, sync setup, the sidebar mid-drag, a note with properties collapsed and expanded, and a footnote card shown with the keyboard up.
