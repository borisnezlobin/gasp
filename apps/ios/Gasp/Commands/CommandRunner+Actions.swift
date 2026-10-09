import UIKit

/// What the browser's commands do, when it's more than a line.
extension CommandRunner {
    // MARK: Notes

    /// Makes an "Untitled" note beside the one showing and opens it.
    func newNote() {
        guard let vault = library.vault else { return }
        let folder = session.map { ($0.path as NSString).deletingLastPathComponent } ?? ""
        do {
            let path = try vault.createNote(folder: folder, title: nil)
            library.edited()
            workspace.sidebarOpen = false
            if tabs.active.path == nil {
                tabs.open(path)
            } else {
                tabs.openInNewTab(path)
            }
            tabs.activeSession?.textView.becomeFirstResponder()
        } catch {
            workspace.tell(error.shownMessage)
        }
    }

    func openDailyNote() {
        guard let vault = library.vault else { return }
        do {
            let path = try vault.dailyNote()
            library.edited()
            tabs.open(path)
        } catch {
            workspace.tell(error.shownMessage)
        }
    }

    func rename(_ path: String, to title: String) {
        guard let vault = library.vault else { return }
        tabs.saveAllNotes()
        do {
            let renamed = try vault.renameNote(path: path, title: title)
            tabs.renamed(from: path, to: renamed)
            library.edited()
        } catch {
            workspace.tell(error.shownMessage)
        }
    }

    /// Moves the note into `folder`, following it in the tabs showing it.
    func move(_ path: String, to folder: String) {
        guard let vault = library.vault else { return }
        tabs.saveAllNotes()
        do {
            let moved = try vault.moveNote(path: path, folder: folder)
            tabs.renamed(from: path, to: moved)
            library.edited()
        } catch {
            workspace.tell(error.shownMessage)
        }
    }

    /// Moves a note or a folder picked up in the file tree into `folder`.
    func move(_ entry: TreeEntry, into folder: String) {
        switch entry {
        case .note(let path): move(path, to: folder)
        case .folder(let path): moveFolder(path, into: folder)
        }
    }

    /// Moves a folder and everything in it into `parent`, following its
    /// notes in the tabs showing them.
    func moveFolder(_ folder: String, into parent: String) {
        guard let vault = library.vault else { return }
        tabs.saveAllNotes()
        do {
            let moved = try vault.moveFolder(folder: folder, into: parent)
            tabs.folderMoved(from: folder, to: moved)
            library.edited()
        } catch {
            workspace.tell(error.shownMessage)
        }
    }

    /// Makes a folder called `name` in `parent` and shows it in the file
    /// tree, ready for notes to be dragged into it.
    func makeFolder(named name: String, in parent: String) {
        let trimmed = name.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty, createFolder(named: trimmed, in: parent) != nil else { return }
        workspace.openSidebar(.files)
    }

    /// Makes a folder called `name` in `parent` and answers its path.
    func createFolder(named name: String, in parent: String) -> String? {
        guard let vault = library.vault else { return nil }
        do {
            let folder = try vault.createFolder(parent: parent, name: name)
            library.edited()
            return folder
        } catch {
            workspace.tell(error.shownMessage)
            return nil
        }
    }

    func delete(_ path: String) {
        guard let vault = library.vault else { return }
        tabs.saveAllNotes()
        do {
            try vault.trashNote(path: path)
            tabs.removed(path)
            library.edited()
        } catch {
            workspace.tell(error.shownMessage)
        }
    }

    /// Puts a kept version back, keeping the text it replaces first so the
    /// restore can be undone too.
    func restore(_ text: String) {
        withSession { session in
            _ = session.vault.recordSnapshot(path: session.path, text: session.document.text(), now: true)
            session.selectAll()
            session.insert(text)
        }
    }

    func insertTemplate(_ name: String) {
        withSession { session in
            do {
                session.insert(try session.vault.templateText(name: name, notePath: session.path))
            } catch {
                workspace.tell(error.shownMessage)
            }
        }
    }

    /// Opens the picker for `source`; the picked image lands at the cursor.
    func pickImage(from source: ImageSource) {
        withSession { _ in workspace.sheet = source.sheet }
    }

    /// Saves a picked image beside the note and embeds it at the cursor.
    func insertImage(_ data: Data, extension fileExtension: String) {
        withSession { session in
            do {
                let embed = try session.vault.saveAttachment(
                    notePath: session.path, bytes: data, extension: fileExtension
                )
                session.insert(embed)
            } catch {
                workspace.tell(error.shownMessage)
            }
        }
    }

    // MARK: Links

    func followLinkAtCursor() {
        withSession { session in
            guard let target = session.document.linkAt(offset: session.selection.start) else {
                workspace.tell("Put the cursor on a link first.")
                return
            }
            follow(link: target, from: session)
        }
    }

    /// Turns the web address alone on the cursor's line into a card, with
    /// the page's title, description and image.
    func makeCard() {
        withSession { session in
            guard let address = session.document.cardAddress(offset: session.selection.start),
                  let url = URL(string: address) else {
                workspace.tell("Put the cursor on a line with only a web address.")
                return
            }
            workspace.tell("Making a card…")
            Task { @MainActor in
                guard let html = await Self.page(at: url) else {
                    workspace.tell("Couldn't reach the page.")
                    return
                }
                session.apply(session.document.makeCard(url: address, html: html))
            }
        }
    }

    /// The start of the page at `url`, where its title and image are.
    private static func page(at url: URL) async -> String? {
        guard let (data, _) = try? await URLSession.shared.data(from: url) else { return nil }
        return String(bytes: data.prefix(512 * 1024), encoding: .utf8)
    }

    // MARK: View

    func cycleSymbols() {
        guard let vault = library.vault else { return }
        let names: [SymbolVisibility: String] = [
            .alwaysShown: "Markdown symbols always show.",
            .aroundCursor: "Markdown symbols show around the cursor.",
            .alwaysHidden: "Markdown symbols are hidden."
        ]
        workspace.tell(names[vault.cycleSymbols()] ?? "")
        refreshSessions()
    }

    func toggleSentenceHighlighting() {
        guard let vault = library.vault else { return }
        do {
            let enabled = try vault.toggleSentenceHighlighting()
            workspace.tell(enabled ? "Sentences are tinted by length." : "Sentence tints are off.")
            library.reloadConfig()
            refreshSessions()
        } catch {
            workspace.tell(error.shownMessage)
        }
    }

    func lookUp() {
        withSession { session in
            guard let term = session.lookUpTerm() else {
                workspace.tell("Select a word to look up.")
                return
            }
            workspace.sheet = .lookUp(term)
        }
    }
}
