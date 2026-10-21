import SwiftUI

/// Setting the vault's first password, or typing it when the vault is
/// locked, before encrypting a note with it.
struct VaultPasswordSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let followUp: VaultPasswordFollowUp
    @State private var password = ""
    @State private var confirmation = ""
    @State private var problem: String?
    @State private var working = false

    private var tokens: Tokens { model.library.tokens }
    private var creating: Bool { model.library.vault?.encryptionState().hasVaultPassword != true }

    private var actionTitle: String {
        switch (creating, followUp) {
        case (true, .encrypt): "Set password and encrypt"
        case (true, .useForNote): "Set vault password"
        case (false, .encrypt): "Unlock and encrypt"
        case (false, .useForNote): "Unlock vault"
        }
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    NewPasswordFields(
                        password: $password, confirmation: creating ? $confirmation : nil,
                        prompt: "Vault password"
                    )
                } footer: {
                    SheetFooter(
                        problem: problem,
                        note: creating
                            ? "It unlocks every encrypted note in this vault. If you forget it, "
                                + "no one can get those notes back."
                            : nil,
                        tokens: tokens
                    )
                }
                Section {
                    SheetActionButton(title: actionTitle, busy: working, tokens: tokens, action: submit)
                        .disabled(password.isEmpty || working)
                }
            }
            .textCase(nil)
            .navigationTitle(creating ? "Set a vault password" : "Unlock the vault")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
        }
        .font(Font(tokens.uiFont(size: tokens.bodySize)))
        .interactiveDismissDisabled(working)
    }

    private func submit() {
        guard let vault = model.library.vault, !password.isEmpty else { return }
        if creating && password != confirmation {
            problem = "The two passwords don't match."
            return
        }
        working = true
        problem = nil
        let (password, creating) = (password, creating)
        DispatchQueue.global(qos: .userInitiated).async {
            let result = Result {
                creating ? try vault.setVaultPassword(password: password) : try vault.unlockVault(password: password)
            }
            DispatchQueue.main.async { finish(result) }
        }
    }

    private func finish(_ result: Result<Void, Error>) {
        working = false
        switch result {
        case .success:
            if EncryptionPreferences(vault: model.library.vault).keepsKeys, let key = model.library.vault?.vaultKey() {
                KeptKeys.keep(key)
            }
            dismiss()
            model.runner.followUp(followUp)
        case .failure(let error):
            problem = error.shownMessage
        }
    }
}

/// A new vault password: the old one, then the new one twice. Every note
/// the vault password opens is rewrapped for the new one.
struct ChangeVaultPasswordSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var current = ""
    @State private var password = ""
    @State private var confirmation = ""
    @State private var problem: String?
    @State private var working = false

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    SecureField("Current password", text: $current)
                        .textContentType(.password)
                }
                Section {
                    NewPasswordFields(password: $password, confirmation: $confirmation, prompt: "New password")
                } footer: {
                    SheetFooter(
                        problem: problem,
                        note: "Your other devices ask for the new password the next time they sync.",
                        tokens: tokens
                    )
                }
                Section {
                    SheetActionButton(title: "Change password", busy: working, tokens: tokens, action: submit)
                        .disabled(current.isEmpty || password.isEmpty || working)
                }
            }
            .textCase(nil)
            .navigationTitle("Change vault password")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
        }
        .font(Font(tokens.uiFont(size: tokens.bodySize)))
        .interactiveDismissDisabled(working)
    }

    private func submit() {
        guard let vault = model.library.vault else { return }
        guard password == confirmation else {
            problem = "The two new passwords don't match."
            return
        }
        working = true
        problem = nil
        let oldKeyID = vault.vaultKey()?.id
        let (current, password) = (current, password)
        DispatchQueue.global(qos: .userInitiated).async {
            let result = Result { try vault.changeVaultPassword(old: current, new: password) }
            DispatchQueue.main.async { finish(result, replacing: oldKeyID) }
        }
    }

    private func finish(_ result: Result<UInt32, Error>, replacing oldKeyID: String?) {
        working = false
        switch result {
        case .success(let count):
            if let oldKeyID { KeptKeys.forget(id: oldKeyID) }
            if EncryptionPreferences(vault: model.library.vault).keepsKeys, let key = model.library.vault?.vaultKey() {
                KeptKeys.keep(key)
            }
            model.library.edited()
            model.workspace.tell(count == 1 ? "Password changed for 1 note." : "Password changed for \(count) notes.")
            dismiss()
        case .failure(let error):
            problem = error.shownMessage
        }
    }
}

/// An unlocked encrypted note's options: whether its name shows, whether
/// Claude's connector may read it, and its own password. Changes save at
/// once.
struct NoteOptionsSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var path: String
    @State private var hidesName = false
    @State private var claude = NoteClaudeAccess.never
    @State private var hasOwnPassword = false
    @State private var choosingPassword = false
    @State private var password = ""
    @State private var confirmation = ""
    @State private var problem: String?

    init(path: String) {
        _path = State(initialValue: path)
    }

    private var tokens: Tokens { model.library.tokens }

    private static let claudeChoices: [(access: NoteClaudeAccess, title: String)] = [
        (.never, "Never"), (.whileUnlocked, "While it's unlocked"), (.always, "Always")
    ]

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Toggle("Hide its name", isOn: $hidesName)
                } footer: {
                    Text("Its file gets a stand-in name, so the title doesn't show in the file list or on GitHub.")
                }
                Section {
                    Picker("Claude can read it", selection: $claude) {
                        ForEach(Self.claudeChoices, id: \.title) { Text($0.title).tag($0.access) }
                    }
                } footer: {
                    Text("For the Gasp connector in Claude, which runs on your Mac.")
                }
                passwordSection
            }
            .textCase(nil)
            .navigationTitle("Encryption")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
            }
        }
        .font(Font(tokens.uiFont(size: tokens.bodySize)))
        .onAppear(perform: readOptions)
        .onChange(of: hidesName) { _, _ in saveOptions() }
        .onChange(of: claude) { _, _ in saveOptions() }
    }

    @ViewBuilder private var passwordSection: some View {
        Section {
            if choosingPassword {
                NewPasswordFields(password: $password, confirmation: $confirmation, prompt: "Note password")
                Button("Set note password", action: setOwnPassword)
                    .disabled(password.isEmpty)
            } else {
                Button(hasOwnPassword ? "Change its password" : "Give it its own password") {
                    choosingPassword = true
                }
                if hasOwnPassword {
                    Button("Use the vault password instead", action: useVaultPassword)
                }
            }
        } header: {
            Text("Password")
        } footer: {
            SheetFooter(
                problem: problem,
                note: hasOwnPassword ? "The vault password doesn't open this note." : nil,
                tokens: tokens
            )
        }
    }

    private func readOptions() {
        guard let options = model.library.vault?.encryptedNoteOptions(path: path) else { return }
        hidesName = options.hidesName
        claude = options.claude
        hasOwnPassword = options.hasOwnPassword
    }

    private func saveOptions() {
        guard let options = model.library.vault?.encryptedNoteOptions(path: path),
              options.hidesName != hidesName || options.claude != claude else { return }
        model.runner.setOptions(claude: claude, hidesName: hidesName, for: path) { newPath in
            path = newPath
        }
    }

    private func setOwnPassword() {
        guard password == confirmation else {
            problem = "The two passwords don't match."
            return
        }
        problem = nil
        model.runner.setNotePassword(password, for: path) { _ in
            hasOwnPassword = true
            choosingPassword = false
            password = ""
            confirmation = ""
            model.workspace.tell("The note has its own password now.")
        }
    }

    private func useVaultPassword() {
        guard model.library.vault?.encryptionState().vaultUnlocked == true else {
            model.workspace.sheet = .vaultPassword(then: .useForNote(path: path))
            return
        }
        model.runner.followUp(.useForNote(path: path))
        hasOwnPassword = false
    }
}

/// A password typed once, or twice when it's new and a typo would lock
/// the notes away.
private struct NewPasswordFields: View {
    @Binding var password: String
    var confirmation: Binding<String>?
    let prompt: String

    var body: some View {
        SecureField(prompt, text: $password)
            .textContentType(confirmation == nil ? .password : .newPassword)
        if let confirmation {
            SecureField("Type it again", text: confirmation)
                .textContentType(.newPassword)
        }
    }
}

/// A form section's footer: what went wrong when something did, or else
/// the note that explains the section.
private struct SheetFooter: View {
    let problem: String?
    let note: String?
    let tokens: Tokens

    var body: some View {
        if let problem {
            ProblemText(message: problem, tokens: tokens)
        } else if let note {
            Text(note)
        }
    }
}

/// A sheet's main action, filling its row, with a spinner while it runs.
private struct SheetActionButton: View {
    let title: String
    let busy: Bool
    let tokens: Tokens
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: tokens.spacing.md) {
                if busy { ProgressView() }
                Text(title)
                    .font(Font(tokens.uiFont(size: tokens.bodySize, bold: true)))
            }
            .frame(maxWidth: .infinity, minHeight: 44)
        }
    }
}
