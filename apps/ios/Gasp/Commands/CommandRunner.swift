import UIKit

/// Runs every command in the shared registry on the phone. Commands that
/// edit the note go to the core through the active editing session; the
/// rest act on the browser: tabs, the sidebar, notes and the view. The
/// keyboard's bar, the palette and hardware keys all come here.
final class CommandRunner: EditingHost {
    let library: VaultLibrary
    let tabs: TabStore
    let workspace: Workspace
    /// The text read from the vault's images and PDFs, for search.
    let imageText: ImageTextReader?

    typealias Handler = (CommandRunner) -> Void

    /// Every command the browser handles itself, by id.
    private static let handlers: [String: Handler] = [
        tabHandlers, navigationHandlers, noteHandlers, viewHandlers, textHandlers
    ].reduce(into: [:]) { all, group in all.merge(group) { first, _ in first } }

    init(library: VaultLibrary, tabs: TabStore, workspace: Workspace) {
        self.library = library
        self.tabs = tabs
        self.workspace = workspace
        imageText = library.vault.map(ImageTextReader.init)
        imageText?.start()
        tabs.makeSession = { [weak self] path, text in self?.makeSession(path: path, text: text) }
    }

    var toolbar: [CommandInfo] {
        library.toolbar
    }

    var keyBindings: [KeyBinding] {
        library.keyBindings
    }

    var session: EditingController? {
        tabs.activeSession
    }

    /// Whether `id` does something here, for the palette.
    func handles(_ id: String) -> Bool {
        Self.handlers[id] != nil || session?.document.handles(id: id) == true
            || Self.tabNumber(id) != nil
    }

    func run(_ id: String) {
        if let handler = Self.handlers[id] {
            handler(self)
        } else if let number = Self.tabNumber(id) {
            tabs.select(number - 1)
        } else if let session {
            runOnNote(id, in: session)
        } else {
            workspace.tell("Open a note first.")
        }
    }

    private func runOnNote(_ id: String, in session: EditingController) {
        switch session.runNoteCommand(id) {
        case .notice(let message):
            workspace.tell(message)
        case .copy:
            workspace.tell("Copied.")
        case nil, .notANoteCommand:
            workspace.tell("That isn't something a note can do.")
        default:
            break
        }
    }

    /// `tab.go-3` → 3.
    private static func tabNumber(_ id: String) -> Int? {
        guard id.hasPrefix("tab.go-") else { return nil }
        return Int(id.dropFirst("tab.go-".count))
    }

    /// Runs `body` with the active session, or says there's no note open.
    func withSession(_ body: (EditingController) -> Void) {
        guard let session else {
            workspace.tell("Open a note first.")
            return
        }
        body(session)
    }

    /// Shows a note, in the active tab or a new one, with the cursor at
    /// `offset` (UTF-16) when there is one.
    func show(_ path: String, at offset: UInt32? = nil, inNewTab: Bool = false) {
        workspace.sidebarOpen = false
        if inNewTab {
            tabs.openInNewTab(path)
        } else {
            tabs.open(path)
        }
        if let offset { tabs.activeSession?.placeCursor(at: Int(offset)) }
    }

    // MARK: Sessions

    private func makeSession(path: String, text: String) -> EditingController? {
        guard let vault = library.vault else { return nil }
        let session = EditingController(
            path: path, text: text, vault: vault, tokens: scaledTokens, host: self
        )
        session.setReadableWidth(workspace.readableWidth)
        return session
    }

    var scaledTokens: Tokens {
        library.tokens.scaled(by: workspace.zoom)
    }

    /// Redraws every open note after the zoom, the theme or the symbols
    /// changed.
    func refreshSessions() {
        for session in tabs.openSessions {
            session.use(scaledTokens)
            session.setReadableWidth(workspace.readableWidth)
            session.showToolbar(library.toolbar)
            session.redrawAll()
        }
    }

    // MARK: EditingHost

    func follow(link target: String, from session: EditingController) {
        do {
            let destination = try session.vault.resolveLink(fromPath: session.path, target: target)
            open(destination, from: session)
        } catch {
            workspace.tell(error.localizedDescription)
        }
    }

    private func open(_ destination: LinkDestination, from session: EditingController) {
        switch destination {
        case .web(let url):
            if let url = URL(string: url) { UIApplication.shared.open(url) }
        case .note(let path, let heading):
            library.refresh()
            tabs.open(path)
            if let heading { tabs.activeSession?.showHeading(heading) }
        case .heading(let heading):
            session.showHeading(heading)
        }
    }
}
