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
    @State private var showing = false

    private var tokens: Tokens { model.library.tokens }
    private var creating: Bool { model.library.vault?.encryptionState().hasVaultPassword != true }

    private var workingTitle: String {
        creating ? "Setting password…" : "Unlocking…"
    }

    private var actionTitle: String {
        switch (creating, followUp) {
        case (true, .encrypt): "Set password and lock"
        case (true, .useForNote): "Set vault password"
        case (false, .encrypt): "Enter password and lock"
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
                    SheetActionButton(
                        title: actionTitle, busyTitle: workingTitle, busy: working, tokens: tokens, action: submit
                    )
                    .disabled(password.isEmpty || working)
                }
            }
            .textCase(nil)
            .navigationTitle(creating ? "Set a vault password" : "Unlock the vault")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() }.disabled(working) }
            }
        }
        .font(Font(tokens.uiFont(size: tokens.bodySize)))
        .interactiveDismissDisabled(working)
        .onAppear { showing = true }
        .onDisappear { showing = false }
    }

    private func submit() {
        guard let vault = model.library.vault, !password.isEmpty, !working else { return }
        if creating && password != confirmation {
            problem = "The two passwords don't match."
            return
        }
        working = true
        problem = nil
        let (password, creating) = (password, creating)
        Task { @MainActor in
            let result = await Result.offMain {
                creating ? try vault.setVaultPassword(password: password) : try vault.unlockVault(password: password)
            }
            working = false
            finish(result)
        }
    }

    /// Keeps the vault's key behind Face ID once it's in hand, then carries
    /// on with what the sheet was opened for, unless it was closed meanwhile.
    private func finish(_ result: Result<Void, Error>) {
        switch result {
        case .success:
            keepVaultKey(model.library.vault)
            guard showing else { return }
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
    @State private var showing = false

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
                    SheetActionButton(
                        title: "Change password", busyTitle: "Changing password…", busy: working,
                        tokens: tokens, action: submit
                    )
                    .disabled(current.isEmpty || password.isEmpty || working)
                }
            }
            .textCase(nil)
            .navigationTitle("Change vault password")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() }.disabled(working) }
            }
        }
        .font(Font(tokens.uiFont(size: tokens.bodySize)))
        .interactiveDismissDisabled(working)
        .onAppear { showing = true }
        .onDisappear { showing = false }
    }

    private func submit() {
        guard let vault = model.library.vault, !working else { return }
        guard password == confirmation else {
            problem = "The two new passwords don't match."
            return
        }
        working = true
        problem = nil
        let oldKeyID = vault.vaultKey()?.id
        let (current, password) = (current, password)
        Task { @MainActor in
            let result = await Result.offMain { try vault.changeVaultPassword(old: current, new: password) }
            working = false
            finish(result, replacing: oldKeyID)
        }
    }

    /// Swaps the key kept behind Face ID and says how many notes changed.
    /// Open notes saved while it ran are already sealed again with the new key.
    private func finish(_ result: Result<UInt32, Error>, replacing oldKeyID: String?) {
        switch result {
        case .success(let count):
            if let oldKeyID { KeptKeys.forget(id: oldKeyID) }
            keepVaultKey(model.library.vault)
            model.library.edited()
            model.workspace.tell(count == 1 ? "Password changed for 1 note." : "Password changed for \(count) notes.")
            if showing { dismiss() }
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
    @State private var settingPassword = false

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
                SheetActionButton(
                    title: "Set note password", busyTitle: "Setting password…", busy: settingPassword,
                    bold: false, tokens: tokens, action: setOwnPassword
                )
                .disabled(password.isEmpty || settingPassword)
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
        guard !settingPassword else { return }
        guard password == confirmation else {
            problem = "The two passwords don't match."
            return
        }
        problem = nil
        settingPassword = true
        model.runner.setNotePassword(password, for: path, whenDone: { settingPassword = false }, done: { _ in
            hasOwnPassword = true
            choosingPassword = false
            password = ""
            confirmation = ""
            model.workspace.tell("The note has its own password now.")
        })
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

/// A sheet's action, filling its row. While it runs, a spinner and the
/// busy title take the title's place, and the row keeps its size.
private struct SheetActionButton: View {
    let title: String
    let busyTitle: String
    let busy: Bool
    var bold = true
    let tokens: Tokens
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            ZStack {
                Text(title)
                    .opacity(busy ? 0 : 1)
                    .accessibilityHidden(busy)
                HStack(spacing: tokens.spacing.md) {
                    ProgressView()
                    Text(busyTitle)
                }
                .opacity(busy ? 1 : 0)
                .accessibilityHidden(!busy)
            }
            .font(Font(tokens.uiFont(size: tokens.bodySize, bold: bold)))
            .frame(maxWidth: .infinity, minHeight: 44)
        }
    }
}

/// Keeps the vault's key behind Face ID, when the vault's settings ask
/// for it.
private func keepVaultKey(_ vault: VaultFolder?) {
    guard EncryptionPreferences(vault: vault).keepsKeys, let key = vault?.vaultKey() else { return }
    KeptKeys.keep(key)
}

extension Result where Failure == Error {
    /// What `work` answers or throws, run off the main thread.
    static func offMain(_ work: @escaping () throws -> Success) async -> Result<Success, Error> {
        do {
            return .success(try await offMainThread(work))
        } catch {
            return .failure(error)
        }
    }
}
