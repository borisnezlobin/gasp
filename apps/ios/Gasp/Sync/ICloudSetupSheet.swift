import SwiftUI
import UniformTypeIdentifiers

/// Syncing with iCloud: one tap and Gasp makes its own folder in iCloud
/// Drive, brings this iPhone's notes along (copied, the originals left
/// where they are) and opens the folder as the vault. With no notes to
/// ask about, it starts as soon as it shows. Opening another folder in
/// iCloud Drive stays as a quieter way in.
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
        .task { if !canBringNotes { await syncWithICloud() } }
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
                text: "Gasp keeps your notes in a Gasp folder in iCloud Drive. "
                    + "Your Mac and other devices pick them up from there.",
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
        .safeAreaInset(edge: .bottom) { footer }
    }

    private var footer: some View {
        WelcomeFooter(
            primary: moving ? "Setting up iCloud" : "Sync with iCloud",
            tokens: tokens, action: startSyncing
        ) {
            if moving {
                ProgressView()
            } else {
                WelcomeQuietButton(title: "Open a folder", symbol: nil, tokens: tokens) { picking = true }
            }
        }
        .disabled(moving)
        .padding(.bottom, tokens.spacing.lg)
    }

    private func startSyncing() {
        Task { await syncWithICloud() }
    }

    /// Makes Gasp's folder in iCloud Drive if it isn't there yet, and
    /// moves in.
    private func syncWithICloud() async {
        guard !moving else { return }
        moving = true
        problem = nil
        do {
            guard let folder = try await ICloudContainer.documentsFolder() else {
                problem = ICloudContainer.turnedOff
                moving = false
                return
            }
            await move(into: folder)
        } catch {
            problem = error.shownMessage
            moving = false
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
        moving = true
        Task { await move(into: folder) }
    }

    private func move(into folder: URL) async {
        let ownVault = canBringNotes && bringsNotes ? model.library.folder : nil
        model.tabs.saveAllNotes()
        do {
            let moved = try await offMainThread {
                NoteSaver.waitForWrites()
                return try ownVault.map { try ICloudVaultMove.bring($0, into: folder) }
            }
            model.openICloudVault(at: folder)
            finished = ICloudSetupResult(folder: ICloudContainer.shownName(of: folder), moved: moved)
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
