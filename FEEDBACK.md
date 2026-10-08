# Feedback

Everything the owner has asked for, newest last, ticked off as it lands on master.

## Naming, builds and install

- [x] Call the app Gasp, and rename the repo, the code and the crates, with a seamless move for the owner's vault and settings.
- [x] Make a DMG for the Mac that can go on a GitHub release.
- [x] Stop the Keychain asking on every launch.
- [x] Install the iPhone app on the owner's phone.
- [x] Make an up to date Mac build.
- [x] Put the other session's iPhone fixes on the plugged-in phone.

## Mac

- [x] Tables look small and odd by default.
- [x] Configurable toolbars: the owner chooses where a bar sits, which buttons it has and how it behaves.
- [x] The settings screen's reset icon shifts the explanation text; nothing should move when a control changes state.
- [x] With the sidebar on hover, reaching for New note makes the sidebar disappear.
- [x] A pass over every interaction in the app.
- [x] The selection menu should be off by default.
- [x] A bar with icons and labels runs off the edge of the window.
- [x] The top bar sits too high, up by the window buttons.
- [x] A bar can float over the note as an overlay instead of taking its own strip.

## First launch and the new tab (Mac)

- [x] Add the design principles (motif, visual communication, restraint, interactivity) to the global CLAUDE.md.
- [x] A first-launch flow that runs through the basics of a Markdown editor.
- [x] Offer making a vault first; opening an existing one (Obsidian works) and a template vault are quieter choices.
- [x] Explain how sync works and how to set it up before anything asks for Keychain access, and let it be skipped.
- [x] The new tab page should meet the same bar.
- [x] Split the sync screen in two; it was cluttered, with nothing to look at first.
- [x] Draw GitHub as something that makes sense for GitHub, not app windows.
- [x] Remove the three lines along the bottom of the tour.

## iPhone

- [x] The button that hides the keyboard stays visible, pinned on the right.
- [x] Indent and outdent move the whole paragraph or list item, not a tab.
- [x] The keyboard bar is configurable.
- [x] Undo and redo are a pill that greys out when there's nothing to undo or redo.
- [x] The tab swipe doesn't wrap around; swiping past the last tab opens the start page, as in Safari.
- [x] The bottom row has too many buttons; make it configurable.
- [x] A first-run setup on the iPhone, matching the Mac's.
- [x] Drag to move notes instead of typing a folder path.
- [x] Number settings overlap their labels in Settings.
- [x] Settings values can be typed in, not only stepped.
- [x] A number setting's value doesn't line up with its label; it sits inside its − and + control now.
- [x] Move notes by dragging in the sidebar, with no sheet and no "Move here" buttons.
- [x] Haptics: checkbox, fold, tab swipe and the start page, number steps and limits, undo and redo, sync conflicts, and the sidebar opening.
- [x] The sidebar opens from a swipe that starts anywhere in the left quarter of the screen.
- [x] Notes show their title above the text, as on the Mac.

## Settings copy (both apps)

- [x] The MCP setting's description says too much and explains too little. Say what it's for, such as "access and change Gasp from the command line, useful for agents".
- [x] Explain date formats instead of naming "Moment.js formats".
- [x] Go through every setting's description the same way.

## Releases, website and usage

- [x] Release notes stay short.
- [x] A website at gaspmd.com, deployed on Vercel. *(Live, with HTTPS.)*
- [x] Count downloads and daily use, with a plain "send anonymous usage data" setting that's on by default and easy to turn off. *(Needs the Upstash store connected in Vercel to start counting.)*

## Sync (from friends trying it)

- [x] Sync commits no longer fill the GitHub contribution graph.
- [x] Changing the branch in settings moves sync to that branch, and a vault stuck on the old one gets a "Switch to" button.
- [x] Sync for people with no GitHub account: iCloud first, one button. *(Built. The iPhone's Files picker still needs a try on a real phone.)*
- [ ] GitHub as the second choice: sign in with GitHub, and Gasp makes the private repository. *(Built. It waits on a GitHub OAuth App and its client ID.)*

## Titles

- [x] Rename a note by editing its title on the Mac (a click on the title now reaches it).
- [x] A long title wraps instead of being cut off, on the Mac and the iPhone.

## Two windows

- [ ] Two windows on two vaults show the same styles even when the vaults' settings differ. *(Fixed in code: each window keeps its own theme. Not yet tried with two real vaults.)*

## Agents and MCP

- [ ] Set up MCP without editing files: Settings → General lists Claude, Claude Code, Cursor and Codex with Connect buttons that write each app's settings. *(Built and tested in temporary folders; still needs a try against the real apps.)*
- [ ] Make Gasp fully customizable: any change someone asks for should be possible through its settings files. The audit of what's still hard-coded, with a suggested key for each:
  - [ ] `layout.toml` isn't read at all: build the window from its slot tree, which also lets the sidebar move to the right.
  - [ ] Sizes with tokens nobody reads: `size.sidebar-width`, `tab-height`, `status-height`, `hover-edge-width` (hard-coded in `apps/desktop/src/theme.rs` near line 1331).
  - [ ] `shadow.*`, `curve.*`, `duration.*` and `animation.*` tokens are unread: use them for popovers and the sidebar reveal.
  - [ ] Interface text size: `font.scale.ui` is unread (13 and 12 are hard-coded); add `font.scale.ui-small`.
  - [ ] Headings: `color.heading`, `font.heading.family`, `font.weight.heading` (they use the body colour and font today).
  - [ ] Editor details: `size.cursor-width`, `size.quote-bar` and `color.quote-bar`, `size.checkbox-border` and `color.checkbox-mark`, `font.scale.bullet`, `size.list-marker`, `editor.tab-width`.
  - [ ] Superscripts and contrast: `font.scale.script`, `font.script.rise` and `drop`, `contrast.note-colours`.
  - [ ] Images and link cards: `size.image-height`, `link-card.*`, `link-cards.timeout`.
  - [ ] Focus ring and keycaps: `opacity.focus-ring`, `size.focus-ring`, `color.keycap`, `color.keycap-text`.
  - [ ] Pickers, panels and the find bar: `picker.*`, `panel.*`, `find.*`; font fallbacks: `font.fallbacks.*`.
  - [ ] Behaviour: `editor.reading-speed` (238 words a minute), `stats.idle-seconds` (60), `editor.renumber-delay` (1.2 s), `search.weights.*`, `palette.recent-boost` and `recent-count`, `embed.max-depth` (4), `timing.copied`, `notices.max-shown` (3), the table handle's timings (`table.*`).
  - [ ] Not settings at all yet: animation speed, custom shadows, background images or custom CSS, vim mode, the app icon.

## Website (gaspmd.com)

Hero whale
- [x] Use a real breach pose, not the swimming whale tilted up.
- [x] Keep the top of the whale from being cut off at steep angles.
- [x] Replace the unnatural click motion: it now rolls onto its back and falls in, and the lines splash.
- [x] Put the whale beside the wordmark, not over the "p".
- [x] Don't hang the whale frozen mid-air, and don't lean it at the pointer.
- [x] Don't leave it floating belly-up ("looks dead"). After the breach it dives and a live whale swims under the lines.
- [x] Remove the hard cut-off under the water and the dark snout the wash missed.
- [ ] Keep making the hero feel natural. *(Waiting on the owner's look at the new cycle.)*

Speed
- [x] "Opens in 300 ms" felt slow. Show real screen recordings of Gasp, Apple Notes and Obsidian opening side by side.
- [x] Give each app its icon.
- [x] Remove the dark edge on the Obsidian clip.
- [x] Move the method text behind an info button.
- [x] Plain heading: "Ready to edit in under half a second".

Long notes and lightness
- [x] Make the 48,000-line field really draw 48,000 lines.
- [x] Replace "One frame at 120 Hz is 8.3 ms" with "no lag, 455 fps".
- [x] Bring back the CPU trace with no numbers, and make it plain (no red glowing dot).

Formatting, screens and features
- [x] Formatting intro: "Gasp uses Markdown, an open format. You're never tied to Gasp."
- [x] Show the screenshot, the switcher and the caption on one screen.
- [x] Put more math in the math and code screenshot.
- [x] Rewrite the useless command palette caption.
- [x] Make "Everything else" quick to scan.
- [x] "Open the folder you have" becomes "Switch with no hassle"; the AI line becomes "Your AI can edit notes, and even Gasp itself".
- [x] In "A note to play with", a click puts the cursor where it lands instead of at the end of the line.

Change anything
- [x] The heading is "Change anything"; the intro is "Any part of Gasp can be edited by your AI agent, like ChatGPT or Claude. Just ask."
- [x] Show it really: the live AI demo is replaced by real renders of one note in five behaviours (Markdown shown, long sentences tinted, buttons on selection, the file list open), each with the request and the settings file behind it.
- [x] Let people ask for any change, and show as many features as possible. *(Replaced by the showcase of real renders, at the owner's choice.)*

Copy and layout
- [x] No requirements line under the download button.
- [x] "Ready to edit in…" instead of "Open, with your note on screen, in 300 ms."
- [x] Delete "A friend who tried it".
- [x] Move the whale credit to a credits page.
- [x] The footer closes on the water distortion, not wavy lines. A whale swims under still lines.
- [x] Remove the whale swimming along the bottom of the window.
- [x] A link preview image that looks good.
- [x] The whole page should feel fast and visceral.

## Mac app (since the website rounds)

- [x] The vault switcher lists each real vault once, skips temporary ones, and says where same-named vaults live ("iCloud Drive", "~/Documents").
- [x] Agent access: each app's real icon in one compact row of tiles instead of a full row per app.
- [x] Set up MCP in one click from Settings, never by editing JSON.
- [x] Each window keeps its own vault's theme.
- [x] Tell people about new versions and update in one click (checks gaspmd.com/api/version).
- [x] Cmd-click an item in the sidebar to move it.
  - Chosen: Cmd-click selects several items, like Finder, and dragging or cutting moves the whole group. *(Shift-click picks a range; Escape clears it; trash works on the group too.)*
- [x] A setting to open notes in new tabs, so Cmd-O opens a tab. *(Files and links → Open notes in new tabs. It covers the file list, Cmd-O and search; Cmd-Enter opens the other way.)*
- [x] Drag notes out of the sidebar the same way as dragging a note's title in the editor. *(Drop on the tab bar to open a tab there, on the note to open it next to the current tab, or on an edge to split.)*
- [x] Checkmarks (connecting Claude and the like) shouldn't use the theme colour. *(They're green now, from a new `color.connected` token.)*
- [x] On the new tab page, typing goes straight into the search. *(The first letter opens the note switcher with it already typed.)*
- [x] Room below a note's last line: half the window on the Mac, 30% of the screen on the iPhone, so the last line can sit mid-screen. *(Not in the 0.2.2 draft.)*
- [x] Hovering an image in the sidebar shows a preview of it. *(After the same pause as a link preview, beside the row, with its size in pixels.)*
- [x] Pasting images is unreliable. After pasting #1, pasting #2, then deleting #1 from the note and the sidebar, the next paste showed the deleted image under #1's name; the paste after that showed the clipboard as #3. Triage every cause.
- [x] Images can't be opened from the sidebar.
  - *Triage: open notes cache each image by its link name and never notice the file was deleted or replaced (Mac and iPhone); new pastes reuse the lowest free number, so a deleted image's name comes back; the sidebar opens every file as a note, so images fail with "stream did not contain valid UTF-8".*
  - [x] Fix the stale image cache on the Mac and the iPhone.
  - [x] Stop new pastes reusing a deleted image's name. *(Numbered one past the highest used.)*
  - [x] Open images from the sidebar in an image tab. *(Fits the pane, a click shows actual size; PDFs and other files open in their default app.)*
  - [x] When an embed you just pasted is removed, offer to delete its image file too. *(Owner: "good idea". Undo withdraws the offer.)*
- [x] Show whether iCloud is syncing; the owner on 0.2.1 can't tell.
  - The owner's vault syncs through GitHub, not iCloud (it's a git clone in ~/Documents). The sync popover now names the repository, like "borisnezlobin/notes on GitHub".

## Releases

- [ ] A better drag-to-install window in the DMG: Gasp on the left, Applications on the right, a designed background with an arrow, no toolbar. *(Built; check the label positions when the next DMG opens.)*
- [x] Publish 0.2.0.
- [ ] Publish 0.2.1 (drafted, notarized): the agent tiles, the vault switcher and the new install window. It's the first release the one-click updater will offer, so check that it updates and relaunches cleanly.
- [x] Draft 0.2.2 on GitHub releases. *(Drafted with notes and a notarized, stapled DMG.)*
- [x] Publish 0.2.2.
- [x] Make 0.2.3. *(Draft on GitHub with a notarized, stapled DMG.)*
- [ ] Publish 0.2.3.
- [ ] Put the iPhone app on TestFlight under Zihao's team: app icon, everything App Review needs, upload, and on the owner's phone. Then a list of what the owner does in App Store Connect, with links.
  - *(Icon, privacy manifest, encryption exemption and `make ios-upload` are in. Build 0.2.3 (591) is uploaded and processing; the owner adds themselves to internal testing and installs it from TestFlight.)*
  - *(Build 600, with the keyboard fix and swipe-to-close tabs, is uploaded. `make ios-upload` now signs in with the App Store Connect API key from ~/.appstoreconnect/gasp.env.)*
- [x] iPhone: swipe to delete on the Tabs page. *(Swipe a card sideways on the tab overview to close it; a short swipe springs back.)*
- [x] iPhone: the keyboard should never be open while the sidebar is open. *(Opening the sidebar puts the keyboard away, its search field only focuses itself with a hardware keyboard attached, and the note can't take focus behind it.)*
- [x] Error notices are ugly and useless, e.g. 'Couldn't open “CR5”: stream did not contain valid UTF-8'. Say what happened and what to do, in people's words. *(A short headline says what failed and a quieter line says why and what to do; the system's wording goes to the log. A note that isn't text already opens in its default app in 0.2.3.)*
- [ ] Make the App Store page ready for submission ("Gasp: Markdown Notes") and generate every asset it needs.
  - *(Five 6.9" screenshots and the full listing text are made (apps/ios/store); the owner pastes them into App Store Connect.)*
- [x] The build folder grew to 40 GB+. Make it small and keep it small. *(43 GB → 6 GB. Debug builds keep only line tables, and `make tidy` keeps the newest five builds of each crate; it runs after `make build` and `make dmg` and once a day from the auto-pull job.)*
- [x] Release notes stay short.

## iPhone first run (from the owner trying TestFlight)

- [x] "Tick this box": only the second box can be ticked; tapping the first puts the cursor between the brackets. *(Every task box in every note now ticks on tap without moving the cursor.)*
- [x] Don't pop the keyboard up automatically during the welcome flow.
- [ ] The first screen should feel like the website's homepage: the liquid shader instead of the jumping whale, which looks dated and weird.
- [ ] Starting from an existing GitHub vault doesn't work: "GitHub sign-in isn't available in this build yet."
- [ ] iCloud setup asks people to make a folder in iCloud Drive themselves; many can't. Gasp should make it.

## Website, later rounds

- [ ] The whale flickers as it jumps in and out of the water.
- [ ] The landing page above the fold still isn't right. Rework it together.
  - The four abstract sketches (Surface, Breath, Instant, Spout) were generic SaaS with no tie to the product. Keep what the old hero had: the note lines (the product) and the whale (the motif). No "big text left, picture right" layout.
  - The next three (ruled page, the icon life-size, the whale parting the text) were the old hero rearranged, and not interesting. The background should be interesting, interactive and novel, like lusion.co.
  - *(Built: a 3D sea of word-capsules on simulated water with the real humpback under it, ripples from the pointer, and a breach where you click. It's on a preview link, waiting for the owner's look.)*
  - The site needs more wow-factor, like landonorris.com's helmet. Idea floated: a maximalist animation below the fold that says "it will make you gasp". Unsure whether an assault on the senses is right.
  - [ ] Build the scroll sequence to lusion.co's standard: dive under the words, the whale swims toward you, breaches through, and the words land as the letters of "Gasp" under "It will make you". *(Agreed 2026-10-02.)*
  - [ ] Use shaders and pointer-driven interaction throughout. The text itself should feel liquid.
  - *(Built: the scroll story and a pointer-stirred liquid layer that bends the scene and the display type. It's on a preview link, waiting for the owner's look.)*
  - The scroll story and the 3D sea missed. *(Reverted to what's live on 2026-10-02.)*
- [x] Remove the breach from the hero, which reads as sloppy. The whale keeps swimming under the lines.
- [x] Make "bubbly" glyphs the site's motif. They should be fluid and dynamic, with the whale's ripple shader, and react to the cursor up close.
  - Chosen: the wordmark and section headings are live glyphs, and a light drift of loose glyphs rises between sections. Body text stays still.
  - Chosen: glyphs lean toward the cursor from a distance and part around it up close.
  - Chosen: soft and wobbly, squishing and jiggling like jelly when pushed.
  - *(Built: the breach is gone and the whale only swims. Headings and the wordmark are soft glyphs, and Markdown marks rise through every page. Live on gaspmd.com since 2026-10-02.)*
  - [x] The rising marks should scroll with the page, not float in a fixed layer.
  - [x] Put the marks throughout the whole page, and use the soft-glyph blobbing on more of the page's text.

- [x] Remove the line under the request field about keeping what's typed. The privacy page says it instead.
- [x] Remove "Show the settings it wrote", which shifted the page.
- [x] Replace the live AI demo with real renders of different behaviours, not just colours.
- [x] Log what people asked the demo, viewable on the stats page. *(Retired with the demo.)*
- [x] Folding: say how to fold in the demo window, not only with a shortcut the site can't use.
- [x] Accept colour names like "orange".
- [x] A changelog page on gaspmd.com that shows every GitHub release's notes. *(Live at gaspmd.com/changelog since 2026-10-03, linked from the footer.)*
