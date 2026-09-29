import SwiftUI

/// What the setup screen starts with.
struct SyncSetupDraft: Equatable {
    var repository = ""
    var branch = "master"
    var token = ""
    /// Starts cloning as soon as the screen shows. Only launch arguments
    /// set it, for tests.
    var startsAtOnce = false
}

/// Setting up sync: the notes repository, its branch and a GitHub token.
/// It clones the notes into the app, keeps the token in the Keychain and
/// opens the clone as the vault.
struct SyncSetupView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State var draft: SyncSetupDraft
    @State private var cloning = false
    @State private var problem: String?

    static let newTokenLink = URL(string: "https://github.com/settings/personal-access-tokens/new")!

    private var tokens: Tokens { model.library.tokens }

    private var canClone: Bool {
        !cloning && !draft.repository.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some View {
        NavigationStack {
            Form {
                Section("Repository") {
                    SetupField(prompt: "github.com/you/notes", text: $draft.repository)
                        .keyboardType(.URL)
                        .accessibilityLabel("Repository")
                }
                Section {
                    SetupField(prompt: "master", text: $draft.branch)
                        .accessibilityLabel("Branch")
                } header: {
                    Text("Branch")
                } footer: {
                    Text(
                        "This iPhone commits to it and sends it. "
                            + "Anything your other tools send to main is merged in too."
                    )
                }
                tokenSection
                if let problem {
                    Section { ProblemText(message: problem, tokens: tokens) }
                }
                Section { cloneButton }
            }
            .textCase(nil)
            .disabled(cloning)
            .navigationTitle("Sync with GitHub")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() }.disabled(cloning) }
            }
        }
        .font(Font(tokens.uiFont(size: tokens.bodySize)))
        .interactiveDismissDisabled(cloning)
        .task { if draft.startsAtOnce { clone() } }
    }

    private var tokenSection: some View {
        Section {
            HStack(spacing: tokens.spacing.md) {
                SecureField("Paste your token", text: $draft.token)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .textContentType(.password)
                PasteButton(payloadType: String.self) { strings in
                    draft.token = strings.first?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                }
                .labelStyle(.iconOnly)
                .buttonBorderShape(.capsule)
            }
        } header: {
            Text("GitHub token")
        } footer: {
            VStack(alignment: .leading, spacing: tokens.spacing.xs) {
                Text("Give it read and write access to this repository's contents. It stays in this iPhone's Keychain.")
                Link("Create a token", destination: Self.newTokenLink)
                    .font(Font(tokens.uiFont(size: tokens.smallSize, bold: true)))
            }
        }
    }

    private var cloneButton: some View {
        Button(action: clone) {
            HStack(spacing: tokens.spacing.md) {
                if cloning { ProgressView() }
                Text(cloning ? "Cloning your notes" : "Clone notes")
                    .font(Font(tokens.uiFont(size: tokens.bodySize, bold: true)))
            }
            .frame(maxWidth: .infinity, minHeight: 44)
        }
        .disabled(!canClone)
    }

    private func clone() {
        guard canClone else { return }
        let folder = VaultLocation.newSyncedFolder(for: draft.repository)
        let setup = SyncSetup(
            repository: draft.repository, branch: draft.branch, token: draft.token, folder: folder.path
        )
        cloning = true
        problem = nil
        DispatchQueue.global(qos: .userInitiated).async {
            let result = Result { try setUpSync(setup: setup) }
            DispatchQueue.main.async { finish(result, folder: folder) }
        }
    }

    private func finish(_ result: Result<Void, Error>, folder: URL) {
        cloning = false
        switch result {
        case .success:
            model.openSyncedVault(at: folder)
            dismiss()
        case .failure(let error):
            problem = error.shownMessage
        }
    }
}

/// A one-line field for addresses, branches and names: no capitals, no
/// corrections.
struct SetupField: View {
    let prompt: String
    @Binding var text: String

    var body: some View {
        TextField(prompt, text: $text)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
    }
}

/// Why something didn't work, with its icon, on the conflict colour.
struct ProblemText: View {
    let message: String
    let tokens: Tokens

    var body: some View {
        Label {
            Text(message).foregroundStyle(tokens.swiftUIColor(\.textStrong))
        } icon: {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(tokens.swiftUIColor(\.conflict))
        }
    }
}
