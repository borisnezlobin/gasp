import SwiftUI

/// Every note in the vault, most recently changed first.
struct NoteListView: View {
    @Environment(VaultLibrary.self) private var library
    @State private var query = ""
    @State private var route: [NoteRoute] = []

    private var tokens: Tokens { library.tokens }

    private var shownNotes: [NoteSummary] {
        guard !query.isEmpty else { return library.notes }
        return library.notes.filter { note in
            note.title.localizedCaseInsensitiveContains(query)
                || note.folder.localizedCaseInsensitiveContains(query)
        }
    }

    var body: some View {
        NavigationStack(path: $route) {
            List(shownNotes, id: \.path) { note in
                NavigationLink(value: NoteRoute(note: note)) {
                    NoteRow(note: note, tokens: tokens)
                }
                .listRowBackground(tokens.swiftUIColor(\.background))
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .background(tokens.swiftUIColor(\.background))
            .overlay { emptyState }
            .navigationTitle(library.name)
            .navigationDestination(for: NoteRoute.self) { route in
                NoteEditorScreen(note: route.note, cursorLine: route.line)
            }
            .searchable(text: $query, prompt: "Search notes")
            .refreshable { library.refresh() }
        }
        .onOpenURL(perform: open)
        .task { openLaunchLink() }
    }

    private func open(_ url: URL) {
        guard let opened = NoteRoute(url: url, notes: library.notes) else { return }
        route = [opened]
    }

    /// `-open editor://open?path=…` on the command line opens a note at
    /// launch, as tests and `xcrun simctl launch` do.
    private func openLaunchLink() {
        guard let link = UserDefaults.standard.string(forKey: "open"), let url = URL(string: link) else { return }
        open(url)
    }

    @ViewBuilder private var emptyState: some View {
        if let problem = library.problem {
            ContentUnavailableView(
                "Can't open the vault",
                systemImage: "exclamationmark.triangle",
                description: Text(problem)
            )
        } else if library.notes.isEmpty {
            ContentUnavailableView(
                "No notes yet",
                systemImage: "doc.text",
                description: Text("Notes in the vault folder show up here.")
            )
        } else if shownNotes.isEmpty {
            ContentUnavailableView.search(text: query)
        }
    }
}

private struct NoteRow: View {
    let note: NoteSummary
    let tokens: Tokens

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
            Text(note.title)
                .font(Font(tokens.textFont(size: tokens.bodySize)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                .lineLimit(2)
            if !note.folder.isEmpty {
                Label(note.folder, systemImage: "folder")
                    .font(Font(tokens.uiFont(size: tokens.smallSize)))
                    .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                    .labelStyle(FolderLabelStyle(spacing: tokens.spacing.sm))
                    .lineLimit(1)
            }
        }
        .padding(.vertical, tokens.spacing.sm)
    }
}

private struct FolderLabelStyle: LabelStyle {
    let spacing: Double

    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: spacing) {
            configuration.icon.imageScale(.small)
            configuration.title
        }
    }
}
