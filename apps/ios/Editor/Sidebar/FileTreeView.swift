import SwiftUI

/// The vault's folders and notes as a tree, folders first. The note
/// showing is picked out; press and hold a note to open it in a new tab,
/// rename it or move it to the trash.
struct FileTreeView: View {
    @Environment(AppModel.self) private var model
    let tokens: Tokens

    var body: some View {
        FolderContents(folder: "", depth: 0, tokens: tokens)
    }
}

private struct FolderContents: View {
    @Environment(AppModel.self) private var model
    let folder: String
    let depth: Int
    let tokens: Tokens

    private var subfolders: [String] {
        model.library.folders.filter { ($0 as NSString).deletingLastPathComponent == folder }
    }

    private var notes: [NoteSummary] {
        model.library.notes
            .filter { $0.folder == folder }
            .sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(subfolders, id: \.self) { subfolder in
                FolderRow(folder: subfolder, depth: depth, tokens: tokens)
            }
            ForEach(notes, id: \.path) { note in
                TreeNoteRow(note: note, depth: depth, tokens: tokens)
            }
        }
    }
}

private struct FolderRow: View {
    @Environment(AppModel.self) private var model
    let folder: String
    let depth: Int
    let tokens: Tokens
    @State private var expanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button { withAnimation(.snappy) { expanded.toggle() } } label: {
                TreeLabel(
                    title: (folder as NSString).lastPathComponent,
                    symbol: expanded ? "folder.fill" : "folder",
                    depth: depth,
                    tokens: tokens,
                    isCurrent: false
                )
            }
            .buttonStyle(.plain)
            .accessibilityValue(expanded ? "Open" : "Closed")
            if expanded {
                FolderContents(folder: folder, depth: depth + 1, tokens: tokens)
            }
        }
        .onAppear { expanded = containsActiveNote }
    }

    private var containsActiveNote: Bool {
        model.tabs.active.path?.hasPrefix(folder + "/") == true
    }
}

private struct TreeNoteRow: View {
    @Environment(AppModel.self) private var model
    let note: NoteSummary
    let depth: Int
    let tokens: Tokens

    var body: some View {
        let isCurrent = model.tabs.active.path == note.path
        Button { model.runner.show(note.path) } label: {
            TreeLabel(title: note.title, symbol: "doc.text", depth: depth, tokens: tokens, isCurrent: isCurrent)
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button("Open in new tab", systemImage: "plus.square.on.square") {
                model.runner.show(note.path, inNewTab: true)
            }
            Button("Rename", systemImage: "character.cursor.ibeam") {
                model.workspace.prompt = .rename(path: note.path)
            }
            Button("Move to trash", systemImage: "trash", role: .destructive) {
                model.workspace.prompt = .delete(path: note.path)
            }
        }
        .accessibilityAddTraits(isCurrent ? .isSelected : [])
    }
}

/// A row of the tree: indented by its depth, filled when it's the note
/// showing.
private struct TreeLabel: View {
    let title: String
    let symbol: String
    let depth: Int
    let tokens: Tokens
    let isCurrent: Bool

    var body: some View {
        HStack(spacing: tokens.spacing.md) {
            Image(systemName: symbol)
                .font(.system(size: 14))
                .foregroundStyle(tokens.swiftUIColor(\.icon))
                .frame(width: 20)
            Text(title)
                .font(Font(tokens.textFont(size: tokens.bodySize, bold: isCurrent)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                .lineLimit(1)
            Spacer(minLength: 0)
        }
        .padding(.leading, CGFloat(depth) * CGFloat(tokens.spacing.xl) + CGFloat(tokens.spacing.sm))
        .frame(minHeight: 40)
        .background(
            RoundedRectangle(cornerRadius: CGFloat(tokens.spacing.radiusMd))
                .fill(isCurrent ? tokens.swiftUIColor(\.fillStrong) : .clear)
        )
        .contentShape(Rectangle())
    }
}
