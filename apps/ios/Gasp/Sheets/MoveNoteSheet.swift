import SwiftUI

/// The vault's folders, to move a note into one. The folder it's in now is
/// checked and can't be picked; links to the note follow it when the vault
/// updates links on renames.
struct MoveNoteSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let path: String
    @State private var query = ""

    private var tokens: Tokens { model.library.tokens }

    private var current: String {
        (path as NSString).deletingLastPathComponent
    }

    /// The top of the vault, then every folder, filtered by the search.
    private var folders: [String] {
        let all = [""] + model.library.folders.sorted { $0.localizedStandardCompare($1) == .orderedAscending }
        guard !query.isEmpty else { return all }
        return all.filter { $0.localizedCaseInsensitiveContains(query) }
    }

    var body: some View {
        NavigationStack {
            List(folders, id: \.self) { folder in
                row(folder)
            }
            .searchable(text: $query, prompt: "Find a folder")
            .navigationTitle("Move “\(model.tabs.title(of: BrowserTab(path: path)))”")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
        }
        .presentationDetents([.medium, .large])
    }

    private func row(_ folder: String) -> some View {
        let isCurrent = folder == current
        return Button {
            dismiss()
            model.runner.move(path, to: folder)
        } label: {
            HStack(spacing: tokens.spacing.md) {
                Image(systemName: folder.isEmpty ? "tray.full" : "folder")
                    .font(tokens.symbolFont(0.875))
                    .foregroundStyle(tokens.swiftUIColor(\.icon))
                Text(folder.isEmpty ? "Top of the vault" : folder)
                    .font(Font(tokens.textFont(size: tokens.bodySize)))
                    .foregroundStyle(tokens.swiftUIColor(isCurrent ? \.textMuted : \.textStrong))
                Spacer(minLength: 0)
                Image(systemName: "checkmark")
                    .font(tokens.symbolFont(0.875, weight: .semibold))
                    .foregroundStyle(tokens.swiftUIColor(\.accent))
                    .opacity(isCurrent ? 1 : 0)
            }
        }
        .disabled(isCurrent)
        .accessibilityAddTraits(isCurrent ? .isSelected : [])
    }
}
