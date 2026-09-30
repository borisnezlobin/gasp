import SwiftUI

/// The vault's folders as a tree, to move a note into one with a tap. The
/// folder it's in now is checked and can't be picked. "New folder" makes a
/// folder at the top of the vault and moves the note into it. Links to the
/// note follow it when the vault updates links on renames.
struct MoveNoteSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let path: String
    @State private var naming = false
    @State private var newName = ""
    @FocusState private var nameFocused: Bool

    private var tokens: Tokens { model.library.tokens }

    private var current: String {
        (path as NSString).deletingLastPathComponent
    }

    var body: some View {
        NavigationStack {
            List {
                newFolderRow
                FolderChoice(
                    title: model.library.name, symbol: "tray.full", depth: 0,
                    isCurrent: current.isEmpty, tokens: tokens
                ) { move(to: "") }
                ForEach(FolderOutline.rows(model.library.folders)) { row in
                    FolderChoice(
                        title: row.name, symbol: "folder", depth: row.depth + 1,
                        isCurrent: row.folder == current, tokens: tokens
                    ) { move(to: row.folder) }
                }
            }
            .navigationTitle("Move “\(model.tabs.title(of: BrowserTab(path: path)))”")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
        }
        .presentationDetents([.medium, .large])
    }

    /// "New folder", which turns into a field for its name in place.
    @ViewBuilder private var newFolderRow: some View {
        HStack(spacing: tokens.spacing.md) {
            Image(systemName: "folder.badge.plus")
                .font(tokens.symbolFont(0.875))
                .foregroundStyle(tokens.swiftUIColor(\.accent))
                .frame(width: 24)
            if naming {
                TextField("Folder name", text: $newName)
                    .font(Font(tokens.textFont(size: tokens.bodySize)))
                    .focused($nameFocused)
                    .submitLabel(.done)
                    .onSubmit(moveIntoNewFolder)
                Button("Move", action: moveIntoNewFolder)
                    .font(Font(tokens.uiFont(size: tokens.bodySize, bold: true)))
                    .disabled(newName.trimmingCharacters(in: .whitespaces).isEmpty)
            } else {
                Button {
                    naming = true
                    nameFocused = true
                } label: {
                    Text("New folder")
                        .font(Font(tokens.textFont(size: tokens.bodySize)))
                        .foregroundStyle(tokens.swiftUIColor(\.accent))
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .frame(minHeight: 32)
    }

    private func move(to folder: String) {
        dismiss()
        model.runner.move(path, to: folder)
    }

    private func moveIntoNewFolder() {
        let name = newName.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty, let folder = model.runner.createFolder(named: name, in: "") else { return }
        move(to: folder)
    }
}

/// One folder to move into, indented by its depth; the note's own folder
/// is checked and muted.
private struct FolderChoice: View {
    let title: String
    let symbol: String
    let depth: Int
    let isCurrent: Bool
    let tokens: Tokens
    let choose: () -> Void

    var body: some View {
        Button(action: choose) {
            HStack(spacing: tokens.spacing.md) {
                Image(systemName: symbol)
                    .font(tokens.symbolFont(0.875))
                    .foregroundStyle(tokens.swiftUIColor(\.icon))
                    .frame(width: 24)
                Text(title)
                    .font(Font(tokens.textFont(size: tokens.bodySize)))
                    .foregroundStyle(tokens.swiftUIColor(isCurrent ? \.textMuted : \.textStrong))
                    .lineLimit(1)
                Spacer(minLength: 0)
                Image(systemName: "checkmark")
                    .font(tokens.symbolFont(0.875, weight: .semibold))
                    .foregroundStyle(tokens.swiftUIColor(\.accent))
                    .opacity(isCurrent ? 1 : 0)
            }
            .padding(.leading, CGFloat(depth) * CGFloat(tokens.spacing.lg))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(isCurrent)
        .accessibilityAddTraits(isCurrent ? .isSelected : [])
    }
}
