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
            library.refresh()
            if tabs.active.path == nil {
                tabs.open(path)
            } else {
                tabs.openInNewTab(path)
            }
            tabs.activeSession?.textView.becomeFirstResponder()
        } catch {
            workspace.tell(error.localizedDescription)
        }
    }

    func openDailyNote() {
        guard let vault = library.vault else { return }
        do {
            let path = try vault.dailyNote()
            library.refresh()
            tabs.open(path)
        } catch {
            workspace.tell(error.localizedDescription)
        }
    }

    func rename(_ path: String, to title: String) {
        guard let vault = library.vault else { return }
        tabs.saveAllNotes()
        do {
            let renamed = try vault.renameNote(path: path, title: title)
            tabs.renamed(from: path, to: renamed)
            library.refresh()
        } catch {
            workspace.tell(error.localizedDescription)
        }
    }

    func delete(_ path: String) {
        guard let vault = library.vault else { return }
        tabs.saveAllNotes()
        do {
            try vault.trashNote(path: path)
            tabs.removed(path)
            library.refresh()
        } catch {
            workspace.tell(error.localizedDescription)
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
                workspace.tell(error.localizedDescription)
            }
        }
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
                workspace.tell(error.localizedDescription)
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
            workspace.tell(error.localizedDescription)
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
