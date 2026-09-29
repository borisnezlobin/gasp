import UIKit

/// The browser's own commands, grouped by what they act on.
extension CommandRunner {
    static let tabHandlers: [String: Handler] = [
        "tab.new": { $0.tabs.newTab() },
        "tab.close": { $0.tabs.close($0.tabs.active.id) },
        "tab.reopen": { $0.tabs.reopenClosed() },
        "tab.next": { $0.tabs.selectNext() },
        "tab.previous": { $0.tabs.selectPrevious() },
        "tab.close-others": { $0.tabs.closeOthers() },
        "tab.close-right": { $0.tabs.closeToTheRight() },
        "history.back": { $0.tabs.goBack() },
        "history.forward": { $0.tabs.goForward() }
    ]

    static let navigationHandlers: [String: Handler] = [
        "palette.open": { $0.workspace.sheet = .palette },
        "search.open": { $0.workspace.openSearch() },
        "switcher.open": { $0.workspace.openSearch() },
        "outline.jump-to-heading": { $0.workspace.openSidebar(.outline) },
        "sidebar.files.toggle": { $0.workspace.sidebarOpen.toggle() },
        "sidebar.files.show": { $0.workspace.openSidebar(.files) },
        "sidebar.files.hide": { $0.workspace.sidebarOpen = false },
        "file-tree.focus": { $0.workspace.openSidebar(.files) },
        "file-tree.reveal-active": { $0.workspace.openSidebar(.files) },
        "sidebar.right.toggle": { $0.toggleSidebar(.links) },
        "sidebar.right.focus": { $0.workspace.openSidebar(.links) },
        "sidebar.backlinks": { $0.workspace.openSidebar(.links) },
        "sidebar.outgoing-links": { $0.workspace.openSidebar(.links) },
        "sidebar.outline": { $0.workspace.openSidebar(.outline) },
        "sidebar.tags": { $0.workspace.openSidebar(.tags) },
        "link.follow": { $0.followLinkAtCursor() },
        "link.make-card": { $0.makeCard() }
    ]

    static let noteHandlers: [String: Handler] = [
        "note.new": { $0.newNote() },
        "daily.open": { $0.openDailyNote() },
        "note.rename": { runner in runner.onNotePath { runner.workspace.prompt = .rename(path: $0) } },
        "note.delete": { runner in runner.onNotePath { runner.workspace.prompt = .delete(path: $0) } },
        "note.recover": { runner in runner.onNotePath { runner.workspace.sheet = .recovery(path: $0) } },
        "app.export": { runner in runner.onNotePath { runner.workspace.prompt = .export(path: $0) } },
        "app.print": { $0.printNote() },
        "template.insert": { runner in runner.withSession { _ in runner.workspace.sheet = .templates } },
        "note.import-image": { runner in runner.withSession { _ in runner.workspace.sheet = .photos } },
        "settings.open": { $0.workspace.sheet = .settings },
        "vault.open": { $0.workspace.prompt = .pickVault }
    ]

    static let viewHandlers: [String: Handler] = [
        "view.zoom-in": { runner in runner.workspace.zoomIn(); runner.refreshSessions() },
        "view.zoom-out": { runner in runner.workspace.zoomOut(); runner.refreshSessions() },
        "view.zoom-reset": { runner in runner.workspace.zoom = 1; runner.refreshSessions() },
        "view.toggle-readable-width": { runner in
            runner.workspace.readableWidth.toggle()
            runner.refreshSessions()
        },
        "markdown.cycle-symbols": { $0.cycleSymbols() },
        "fold.toggle": { runner in
            runner.withSession { session in
                if !session.toggleFoldAtCursor() { runner.workspace.tell("There's no heading here to fold.") }
            }
        },
        "fold.all": { $0.withSession { $0.foldAllHeadings() } },
        "fold.unfold-all": { $0.withSession { $0.unfoldAllHeadings() } },
        "prose.toggle-sentence-highlighting": { $0.toggleSentenceHighlighting() }
    ]

    static let textHandlers: [String: Handler] = [
        "keyboard.hide": { $0.session?.textView.resignFirstResponder() },
        "edit.undo": { $0.withSession { $0.undo() } },
        "edit.redo": { $0.withSession { $0.redo() } },
        "edit.copy": { $0.withSession { $0.copySelectionOrLine() } },
        "edit.cut": { $0.withSession { $0.cutSelectionOrLine() } },
        "edit.paste": { $0.withSession { $0.paste() } },
        "edit.paste-plain": { $0.withSession { $0.pastePlain() } },
        "select.all": { $0.withSession { $0.selectAll() } },
        "edit.look-up": { $0.lookUp() },
        "find.open": { $0.withSession { $0.showFind(replacing: false) } },
        "find.replace": { $0.withSession { $0.showFind(replacing: true) } },
        "find.next": { $0.withSession { $0.findNext() } },
        "find.previous": { $0.withSession { $0.findPrevious() } }
    ]

    /// Runs `body` with the path of the note showing, or says there's none.
    func onNotePath(_ body: (String) -> Void) {
        withSession { body($0.path) }
    }

    private func toggleSidebar(_ section: SidebarSection) {
        let showing = workspace.sidebarOpen && workspace.sidebarSection == section
        showing ? (workspace.sidebarOpen = false) : workspace.openSidebar(section)
    }
}

/// Sync's commands: syncing now (or setting sync up when it isn't yet),
/// and the resolver.
extension CommandRunner {
    static let syncHandlers: [String: Handler] = [
        "sync.now": { $0.syncNow() },
        "sync.resolve-conflicts": { $0.resolveConflicts() }
    ]

    private func syncNow() {
        if sync.isSynced {
            sync.syncNow()
        } else {
            workspace.sheet = .syncSetup(SyncSetupDraft())
        }
    }

    private func resolveConflicts() {
        if sync.conflicts.isEmpty {
            workspace.tell("No notes are waiting for you.")
        } else {
            workspace.sheet = .resolver
        }
    }
}
