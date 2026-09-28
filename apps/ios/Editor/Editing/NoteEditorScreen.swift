import SwiftUI

/// One note, editable with live preview. Edits save back to the vault a
/// moment after typing stops, and again on leaving.
struct NoteEditorScreen: View {
    let note: NoteSummary
    var cursorLine: Int?
    @Environment(VaultLibrary.self) private var library
    @State private var loaded: Result<String, Error>?

    var body: some View {
        content
            .background(library.tokens.swiftUIColor(\.background))
            .navigationTitle(note.title)
            .navigationBarTitleDisplayMode(.inline)
            .task(id: note.path) { load() }
            .onDisappear { library.refresh() }
    }

    @ViewBuilder private var content: some View {
        switch loaded {
        case .success(let text):
            if let vault = library.vault {
                MarkdownEditor(
                    text: text,
                    saver: NoteSaver(vault: vault, path: note.path),
                    tokens: library.tokens,
                    cursorLine: cursorLine
                )
            }
        case .failure(let error):
            ContentUnavailableView(
                "Can't open this note",
                systemImage: "doc.questionmark",
                description: Text(error.localizedDescription)
            )
        case nil:
            ProgressView()
        }
    }

    private func load() {
        guard let vault = library.vault else { return }
        loaded = Result { try vault.readNote(path: note.path) }
    }
}
