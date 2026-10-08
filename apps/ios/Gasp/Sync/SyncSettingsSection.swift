import SwiftUI

/// Sync on the settings screen. For the synced vault: the repository, the
/// branches, how often to check, and the token. For the iCloud vault:
/// where it is and how it stands. Otherwise, the ways to start syncing.
struct SyncSettingsSection: View {
    @Environment(AppModel.self) private var model
    /// Writes a setting the way every other row does.
    let write: (String, SettingValue) -> Void

    var body: some View {
        if model.library.kind == .synced, let overview = model.sync.overview {
            SyncedVaultRows(overview: overview, write: write)
        } else if model.library.kind == .icloud {
            ICloudVaultRows()
        } else {
            NotSyncingRows()
        }
    }
}

/// Where the iCloud vault is, and a row for how it stands that opens the
/// details.
private struct ICloudVaultRows: View {
    @Environment(AppModel.self) private var model

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        Section("Sync") {
            LabeledContent("Where", value: model.icloud.place)
            Button { model.workspace.sheet = .icloudDetails } label: {
                HStack(spacing: tokens.spacing.md) {
                    SyncGlyph(look: SyncLook(icloud: model.icloud), tokens: tokens)
                        .frame(width: 28)
                    Text(model.icloud.headline)
                        .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                    Spacer()
                    Image(systemName: "chevron.right")
                        .font(tokens.symbolFont(0.75, weight: .semibold))
                        .foregroundStyle(tokens.swiftUIColor(\.textFaint))
                }
            }
        }
    }
}

/// The ways to start syncing a vault that doesn't: iCloud, GitHub, and the
/// address and token form; and the synced notes when this phone has them.
private struct NotSyncingRows: View {
    @Environment(AppModel.self) private var model

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        Section("Sync") {
            Button { model.workspace.sheet = .icloudSetup } label: {
                Label("Sync with iCloud", systemImage: "icloud")
            }
            Button { model.workspace.sheet = .githubSignIn } label: {
                Label("Sign in with GitHub", systemImage: "arrow.triangle.branch")
            }
            .disabled(GitHubConnection.clientId == nil)
            Button { model.workspace.sheet = .syncSetup(SyncSetupDraft()) } label: {
                Text("Use a repository address and token")
                    .font(Font(tokens.uiFont(size: tokens.smallSize)))
                    .foregroundStyle(tokens.swiftUIColor(\.textDetail))
            }
            if VaultLocation.syncedFolder != nil {
                Button("Open your synced notes") { model.switchVault(to: .synced) }
            }
        }
    }
}

private struct SyncedVaultRows: View {
    @Environment(AppModel.self) private var model
    let overview: SyncOverview
    let write: (String, SettingValue) -> Void
    @State private var repository = ""
    @State private var branch = ""
    @State private var legacyBranch = ""
    @State private var problem: String?

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        Section {
            SettingTextRow(title: "Repository", prompt: "github.com/you/notes", text: $repository) {
                attempt { try model.sync.setRepository(repository) }
            }
            SettingTextRow(title: "Branch", prompt: "master", text: $branch) {
                write("sync.branch", .text(value: branch))
            }
            SettingTextRow(title: "Legacy branch", prompt: "None", text: $legacyBranch) {
                write("sync.legacy-branch", .text(value: legacyBranch))
            }
            Stepper(value: intervalBinding, in: 1...120) {
                Text("Check every \(overview.intervalMinutes) \(overview.intervalMinutes == 1 ? "minute" : "minutes")")
            }
            SyncAccountRow(overview: overview) { problem = $0 }
        } header: {
            Text("Sync")
        } footer: {
            if let problem {
                ProblemText(message: problem, tokens: tokens)
            } else {
                Text(
                    "Every sync also merges the legacy branch in one way, for tools that still send to it. "
                        + "Leave it empty to stop."
                )
            }
        }
        .onAppear(perform: fill)
        .onChange(of: overview) { fill() }
    }

    private var intervalBinding: Binding<Int> {
        Binding(
            get: { Int(overview.intervalMinutes) },
            set: { write("sync.interval-minutes", .integer(value: Int64($0))) }
        )
    }

    private func fill() {
        repository = overview.repository ?? ""
        branch = overview.branch
        legacyBranch = overview.legacyBranch
    }

    private func attempt(_ change: () throws -> Void) {
        do {
            try change()
            problem = nil
        } catch {
            problem = error.shownMessage
        }
    }
}

/// The token: signed in with a way to sign out, or a field to sign in.
private struct SyncAccountRow: View {
    @Environment(AppModel.self) private var model
    let overview: SyncOverview
    let report: (String?) -> Void
    @State private var token = ""
    @State private var confirmingSignOut = false

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        if overview.signedIn {
            HStack {
                Label("Signed in with a token", systemImage: "key.fill")
                    .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                Spacer()
                Button("Sign out", role: .destructive) { confirmingSignOut = true }
            }
            .confirmationDialog(
                "Sign out of sync on this iPhone?", isPresented: $confirmingSignOut, titleVisibility: .visible
            ) {
                Button("Sign out", role: .destructive) { attempt { try model.sync.signOut() } }
            } message: {
                Text("Your notes stay on this iPhone. They sync again once you paste a token.")
            }
        } else if overview.takesToken {
            HStack(spacing: tokens.spacing.md) {
                SecureField("Paste a GitHub token", text: $token)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                PasteButton(payloadType: String.self) { token = $0.first ?? "" }
                    .labelStyle(.iconOnly)
                    .buttonBorderShape(.capsule)
                Button("Sign in") { attempt { try model.sync.signIn(token: token) } }
                    .disabled(token.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
    }

    private func attempt(_ change: () throws -> Void) {
        do {
            try change()
            token = ""
            report(nil)
        } catch {
            report(error.shownMessage)
        }
    }
}

/// A text setting written when editing ends.
private struct SettingTextRow: View {
    let title: String
    let prompt: String
    @Binding var text: String
    let submit: () -> Void

    var body: some View {
        LabeledContent(title) {
            TextField(prompt, text: $text)
                .multilineTextAlignment(.trailing)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .onSubmit(submit)
        }
    }
}

/// The vaults this phone can open, the open one checked.
struct VaultChoiceSection: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        Section("Vault") {
            if let synced = VaultLocation.syncedFolder {
                VaultRow(title: synced.lastPathComponent, detail: "Synced", kind: .synced)
            }
            if let icloud = VaultLocation.icloudFolder {
                VaultRow(title: ICloudContainer.shownName(of: icloud), detail: "In iCloud Drive", kind: .icloud)
            }
            VaultRow(title: VaultLocation.localFolder.lastPathComponent, detail: "On this iPhone only", kind: .local)
            if VaultLocation.hasPickedFolder {
                VaultRow(title: "Folder from Files", detail: "Picked in Files", kind: .picked)
            }
            Button("Open another folder") {
                dismiss()
                model.workspace.prompt = .pickVault
            }
        }
    }
}

private struct VaultRow: View {
    @Environment(AppModel.self) private var model
    let title: String
    let detail: String
    let kind: VaultKind

    private var tokens: Tokens { model.library.tokens }
    private var isOpen: Bool { model.library.kind == kind }

    var body: some View {
        Button { if !isOpen { model.switchVault(to: kind) } } label: {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                        .font(Font(tokens.uiFont(size: tokens.bodySize, bold: isOpen)))
                        .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                    Text(detail)
                        .font(Font(tokens.uiFont(size: tokens.smallSize)))
                        .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                }
                Spacer()
                if isOpen {
                    Image(systemName: "checkmark").foregroundStyle(tokens.swiftUIColor(\.accent))
                }
            }
        }
        .accessibilityAddTraits(isOpen ? .isSelected : [])
    }
}
