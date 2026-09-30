import SwiftUI
import UniformTypeIdentifiers

/// Syncing with iCloud: the person picks the Gasp folder in iCloud Drive
/// once, this iPhone's own notes can come along (copied, the originals
/// left where they are), and the folder opens as the vault.
struct ICloudSetupSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var bringsNotes = true
    @State private var picking = false
    @State private var moving = false
    @State private var problem: String?
    @State private var finished: ICloudSetupResult?

    private var tokens: Tokens { model.library.tokens }

    /// Whether there are notes of this iPhone's own to bring along.
    private var canBringNotes: Bool {
        model.library.isThisPhonesOwn && !model.library.notes.isEmpty
    }

    var body: some View {
        NavigationStack {
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(tokens.swiftUIColor(\.background).ignoresSafeArea())
                .navigationTitle("Sync with iCloud")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        if finished == nil { Button("Cancel") { dismiss() }.disabled(moving) }
                    }
                }
        }
        .font(Font(tokens.uiFont(size: tokens.bodySize)))
        .interactiveDismissDisabled(moving)
        .fileImporter(isPresented: $picking, allowedContentTypes: [.folder], onCompletion: picked)
    }

    @ViewBuilder private var content: some View {
        if let finished {
            SetupDone(sentence: finished.sentence, detail: finished.detail, tokens: tokens) { dismiss() }
        } else {
            choosing
        }
    }

    private var choosing: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xl) {
            CloudDrawing(tokens: tokens) {
                Label(icloudFolderName(), systemImage: "folder.fill")
                    .font(Font(tokens.uiFont(size: tokens.bodySize, bold: true)))
                    .foregroundStyle(tokens.swiftUIColor(\.textStrong))
            }
            .frame(height: 120)
                .frame(maxWidth: .infinity)
                .padding(.top, tokens.spacing.xl)
            WelcomeSentence(
                text: "Choose the Gasp folder in iCloud Drive. If there isn't one yet, make a folder called Gasp there.",
                tokens: tokens
            )
            if canBringNotes {
                Toggle("Bring this iPhone's notes along", isOn: $bringsNotes)
                    .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                    .tint(tokens.swiftUIColor(\.accent))
                    .disabled(moving)
            }
            if let problem { ProblemText(message: problem, tokens: tokens) }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, WelcomeMetrics.margin)
        .safeAreaInset(edge: .bottom) {
            WelcomeFooter(
                primary: moving ? "Bringing your notes along" : "Choose the folder",
                tokens: tokens, action: { picking = true }
            ) {
                if moving { ProgressView() }
            }
            .disabled(moving)
            .padding(.bottom, tokens.spacing.lg)
        }
    }

    private func picked(_ result: Result<URL, Error>) {
        guard case .success(let folder) = result else { return }
        _ = folder.startAccessingSecurityScopedResource()
        guard isInIcloud(path: folder.path) else {
            problem = "That folder isn't in iCloud Drive. Choose one under iCloud Drive."
            return
        }
        problem = nil
        let ownVault = canBringNotes && bringsNotes ? model.library.folder : nil
        model.tabs.saveAllNotes()
        moving = true
        Task { await move(ownVault, into: folder) }
    }

    private func move(_ ownVault: URL?, into folder: URL) async {
        do {
            let moved = try await offMainThread {
                NoteSaver.waitForWrites()
                return try ownVault.map { try ICloudVaultMove.bring($0, into: folder) }
            }
            model.openICloudVault(at: folder)
            finished = ICloudSetupResult(folder: folder.lastPathComponent, moved: moved)
        } catch {
            problem = error.shownMessage
        }
        moving = false
    }
}

/// What setting up iCloud did, for its last screen.
struct ICloudSetupResult {
    let folder: String
    let moved: ICloudMove?

    var sentence: String {
        guard let moved, moved.notes > 0 else {
            return "This vault is in iCloud Drive now, in \(folder), and it syncs on its own from here."
        }
        let notes = moved.notes == 1 ? "1 note" : "\(moved.notes) notes"
        return "\(notes) came along into iCloud Drive, in \(folder). They sync on their own from here."
    }

    var detail: String? {
        guard let kept = moved?.keptBeside, !kept.isEmpty else { return nil }
        let (count, beside) = kept.count == 1
            ? ("1 note was", "its version from this iPhone sits beside it")
            : ("\(kept.count) notes were", "their versions from this iPhone sit beside them")
        return "\(count) already there with other text, so \(beside), ending in “(from \(SyncCenter.deviceName))”."
    }
}
