import SwiftUI

/// A note's kept versions, newest first. Open one to read it and put it
/// back; the text it replaces is kept too, so a restore can be undone.
struct RecoverySheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let path: String

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        NavigationStack {
            let snapshots = model.library.vault?.snapshots(path: path) ?? []
            List(snapshots, id: \.id) { snapshot in
                NavigationLink {
                    SnapshotPreview(path: path, snapshot: snapshot) { text in
                        dismiss()
                        model.runner.restore(text)
                    }
                } label: {
                    Text(Self.when(snapshot))
                        .font(Font(tokens.uiFont(size: tokens.bodySize)))
                        .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                }
            }
            .overlay {
                if snapshots.isEmpty {
                    ContentUnavailableView(
                        "No earlier versions",
                        systemImage: isEncrypted ? "lock" : "clock.arrow.circlepath",
                        description: Text(emptyExplanation)
                    )
                }
            }
            .navigationTitle("Earlier versions")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } }
            }
        }
    }

    private var isEncrypted: Bool {
        model.library.vault?.isNoteUnlocked(path: path) ?? false
    }

    private var emptyExplanation: String {
        isEncrypted
            ? "Encrypted notes don't keep earlier versions, since each one would hold the note unencrypted."
            : "A version is kept every few minutes while you edit."
    }

    static func when(_ snapshot: SnapshotInfo) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(snapshot.taken))
        let relative = date.formatted(.relative(presentation: .named))
        return relative + ", " + date.formatted(date: .abbreviated, time: .shortened)
    }
}

private struct SnapshotPreview: View {
    @Environment(AppModel.self) private var model
    let path: String
    let snapshot: SnapshotInfo
    let restore: (String) -> Void

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        let text = (try? model.library.vault?.snapshotText(path: path, id: snapshot.id)) ?? ""
        ScrollView {
            Text(text)
                .font(Font(tokens.textFont(size: tokens.smallSize)))
                .foregroundStyle(tokens.swiftUIColor(\.text))
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(tokens.spacing.xl)
                .textSelection(.enabled)
        }
        .navigationTitle(RecoverySheet.when(snapshot))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .confirmationAction) { Button("Restore") { restore(text) } }
        }
    }
}
