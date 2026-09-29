import SwiftUI

/// Notes matching a search, best first, each with the lines that match.
/// It runs the desktop's search off the main thread as the query changes,
/// finding notes by the text in the images they embed too; `tag:name` or
/// `#name` finds tagged notes.
struct SearchResults: View {
    @Environment(AppModel.self) private var model
    let query: String
    let tokens: Tokens
    /// Opens a note, at the offset of the line picked when there is one.
    let open: (String, UInt32?) -> Void
    @State private var results: [SearchResult] = []
    @State private var searched = ""

    var body: some View {
        LazyVStack(alignment: .leading, spacing: tokens.spacing.sm) {
            ForEach(results, id: \.note.path) { result in
                ResultRow(result: result, tokens: tokens, open: open)
            }
            if results.isEmpty && searched == query {
                Text("Nothing matches.")
                    .font(Font(tokens.uiFont(size: tokens.bodySize)))
                    .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                    .padding(.vertical, tokens.spacing.lg)
            }
        }
        .task(id: query) { await search() }
    }

    private func search() async {
        try? await Task.sleep(for: .milliseconds(120))
        guard !Task.isCancelled, let vault = model.library.vault else { return }
        let query = self.query
        let images = model.runner.imageText?.texts
        let found = await Task.detached(priority: .userInitiated) {
            images.map { vault.searchEverything(query: query, images: $0) } ?? vault.search(query: query)
        }.value
        guard !Task.isCancelled else { return }
        results = Array(found.prefix(50))
        searched = query
    }
}

private struct ResultRow: View {
    let result: SearchResult
    let tokens: Tokens
    let open: (String, UInt32?) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
            NoteRow(note: result.note, tokens: tokens) { open(result.note.path, nil) }
            ForEach(Array(result.hits.prefix(3).enumerated()), id: \.offset) { _, hit in
                Button { open(result.note.path, hit.offset) } label: {
                    Text(excerpt(hit))
                        .font(Font(tokens.textFont(size: tokens.smallSize)))
                        .foregroundStyle(tokens.swiftUIColor(\.textMuted))
                        .lineLimit(2)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.leading, tokens.spacing.md)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.bottom, tokens.spacing.sm)
    }

    /// The line with its matches picked out.
    private func excerpt(_ hit: SearchHit) -> AttributedString {
        let text = NSMutableAttributedString(string: hit.excerpt)
        for range in hit.highlights where Int(range.end) <= text.length {
            text.addAttribute(.backgroundColor, value: tokens.color(\.searchMatch), range: range.nsRange)
        }
        return (try? AttributedString(text, including: \.uiKit)) ?? AttributedString(hit.excerpt)
    }
}
