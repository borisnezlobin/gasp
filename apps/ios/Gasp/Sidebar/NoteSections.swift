import SwiftUI

/// The showing note's headings, indented by level. A tap puts the cursor
/// on the heading.
struct OutlineList: View {
    @Environment(AppModel.self) private var model
    let tokens: Tokens

    var body: some View {
        let headings = model.tabs.activeSession?.document.outline() ?? []
        VStack(alignment: .leading, spacing: 0) {
            if headings.isEmpty {
                EmptyNote(text: "This note has no headings.", tokens: tokens)
            }
            ForEach(Array(headings.enumerated()), id: \.offset) { _, heading in
                Button { jump(to: heading) } label: {
                    Text(heading.title)
                        .font(Font(tokens.textFont(size: tokens.bodySize, bold: heading.level <= 2)))
                        .foregroundStyle(tokens.swiftUIColor(heading.level <= 2 ? \.textStrong : \.text))
                        .lineLimit(2)
                        .padding(.leading, CGFloat(Int(heading.level) - 1) * CGFloat(tokens.spacing.lg))
                        .frame(maxWidth: .infinity, minHeight: 40, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func jump(to heading: OutlineHeading) {
        model.workspace.sidebarOpen = false
        model.tabs.activeSession?.placeCursor(at: Int(heading.range.start))
    }
}

/// The notes that link to the showing note, and those it links to.
struct LinksList: View {
    @Environment(AppModel.self) private var model
    let tokens: Tokens

    var body: some View {
        if let path = model.tabs.active.path, let vault = model.library.vault {
            VStack(alignment: .leading, spacing: tokens.spacing.lg) {
                group("Linked from", notes: vault.backlinks(path: path), empty: "No notes link here.")
                group("Links to", notes: vault.outgoingLinks(path: path), empty: "This note links nowhere.")
            }
        } else {
            EmptyNote(text: "Open a note to see its links.", tokens: tokens)
        }
    }

    private func group(_ title: String, notes: [NoteSummary], empty: String) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title)
                .font(Font(tokens.uiFont(size: tokens.smallSize, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.textDetail))
            if notes.isEmpty {
                EmptyNote(text: empty, tokens: tokens)
            }
            ForEach(notes, id: \.path) { note in
                NoteRow(note: note, tokens: tokens) { model.runner.show(note.path) }
            }
        }
    }
}

/// Every tag in the vault with how many notes carry it. A tap searches
/// for them.
struct TagsList: View {
    @Environment(AppModel.self) private var model
    let tokens: Tokens

    var body: some View {
        let tags = model.library.vault?.tags() ?? []
        VStack(alignment: .leading, spacing: 0) {
            if tags.isEmpty {
                EmptyNote(text: "No notes have tags yet.", tokens: tokens)
            }
            ForEach(tags, id: \.name) { tag in
                Button { model.workspace.searchQuery = "tag:\(tag.name)" } label: {
                    HStack {
                        Text("#\(tag.name)")
                            .font(Font(tokens.textFont(size: tokens.bodySize)))
                            .foregroundStyle(tokens.swiftUIColor(\.link))
                        Spacer()
                        Text("\(tag.notes)")
                            .font(Font(tokens.uiFont(size: tokens.smallSize)))
                            .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                            .monospacedDigit()
                    }
                    .frame(minHeight: 40)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
    }
}

/// A quiet line saying a list is empty.
struct EmptyNote: View {
    let text: String
    let tokens: Tokens

    var body: some View {
        Text(text)
            .font(Font(tokens.uiFont(size: tokens.smallSize)))
            .foregroundStyle(tokens.swiftUIColor(\.textDetail))
            .padding(.vertical, tokens.spacing.md)
    }
}
