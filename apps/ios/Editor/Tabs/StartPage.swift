import SwiftUI

/// What a new tab shows: a search across the vault, and the notes changed
/// most recently, with a new note and today's note a tap away.
struct StartPage: View {
    @Environment(AppModel.self) private var model
    @State private var query = ""
    @FocusState private var searching: Bool

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: tokens.spacing.xl) {
                SearchField(query: $query, prompt: "Search notes", tokens: tokens)
                    .focused($searching)
                if query.isEmpty {
                    actions
                    RecentNotes(tokens: tokens) { open($0) }
                } else {
                    SearchResults(query: query, tokens: tokens) { path, offset in model.runner.show(path, at: offset) }
                }
            }
            .padding(.horizontal, tokens.spacing.xl)
            .padding(.top, tokens.spacing.xl)
            .padding(.bottom, TabBarView.clearance)
        }
        .scrollDismissesKeyboard(.interactively)
    }

    private var actions: some View {
        HStack(spacing: tokens.spacing.md) {
            ActionChip(title: "New note", symbol: "square.and.pencil", tokens: tokens) {
                model.runner.run("note.new")
            }
            ActionChip(title: "Today", symbol: "calendar", tokens: tokens) {
                model.runner.run("daily.open")
            }
        }
    }

    private func open(_ path: String) {
        model.runner.show(path)
    }
}

/// The notes changed most recently.
private struct RecentNotes: View {
    @Environment(AppModel.self) private var model
    let tokens: Tokens
    let open: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Recent")
                .font(Font(tokens.uiFont(size: tokens.smallSize, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                .padding(.bottom, tokens.spacing.sm)
            ForEach(model.library.notes.prefix(20), id: \.path) { note in
                NoteRow(note: note, tokens: tokens) { open(note.path) }
            }
        }
    }
}

/// A button with an icon and a few words, on a quiet fill.
struct ActionChip: View {
    let title: String
    let symbol: String
    let tokens: Tokens
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label(title, systemImage: symbol)
                .font(Font(tokens.uiFont(size: tokens.smallSize)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                .padding(.horizontal, tokens.spacing.lg)
                .frame(minHeight: 40)
                .background(Capsule().fill(tokens.swiftUIColor(\.fill)))
        }
        .buttonStyle(.plain)
    }
}

/// A note in a list: its title, and its folder when it's in one.
struct NoteRow: View {
    let note: NoteSummary
    let tokens: Tokens
    var isCurrent = false
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            VStack(alignment: .leading, spacing: tokens.spacing.xs) {
                Text(note.title)
                    .font(Font(tokens.textFont(size: tokens.bodySize, bold: isCurrent)))
                    .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                    .lineLimit(2)
                if !note.folder.isEmpty {
                    Text(note.folder)
                        .font(Font(tokens.uiFont(size: tokens.smallSize)))
                        .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                        .lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.vertical, tokens.spacing.md)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// A search box on a quiet fill.
struct SearchField: View {
    @Binding var query: String
    let prompt: String
    let tokens: Tokens

    var body: some View {
        HStack(spacing: tokens.spacing.md) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(tokens.swiftUIColor(\.icon))
            TextField(prompt, text: $query)
                .font(Font(tokens.uiFont(size: tokens.bodySize)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.search)
            if !query.isEmpty {
                Button { query = "" } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(tokens.swiftUIColor(\.textFaint))
                }
                .accessibilityLabel("Clear the search")
            }
        }
        .padding(.horizontal, tokens.spacing.lg)
        .frame(minHeight: 44)
        .background(
            RoundedRectangle(cornerRadius: CGFloat(tokens.spacing.radiusLg))
                .fill(tokens.swiftUIColor(\.fill))
        )
    }
}
