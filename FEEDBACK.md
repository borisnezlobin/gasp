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
- [x] A website at gaspmd.com, deployed on Vercel. *(Live on Vercel; waiting on two DNS records at Porkbun.)*
- [x] Count downloads and daily use, with a plain "send anonymous usage data" setting that's on by default and easy to turn off. *(Needs the Upstash store connected in Vercel to start counting.)*

## Sync (from friends trying it)

- [x] Sync commits no longer fill the GitHub contribution graph.
- [x] Changing the branch in settings moves sync to that branch, and a vault stuck on the old one gets a "Switch to" button.
- [ ] Sync for people with no GitHub account: iCloud first, one button. *(In progress.)*
- [ ] GitHub as the second choice: sign in with GitHub, and Gasp makes the private repository. *(In progress.)*

## Titles

- [x] Rename a note by editing its title on the Mac (a click on the title now reaches it).
- [x] A long title wraps instead of being cut off, on the Mac and the iPhone.
