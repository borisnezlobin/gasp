import Foundation

/// What to do once the vault password has been set or typed.
enum VaultPasswordFollowUp: Equatable {
    /// Encrypt the note at this path.
    case encrypt(path: String)
    /// Hand the note at this path back to the vault password.
    case useForNote(path: String)

    var path: String {
        switch self {
        case .encrypt(let path), .useForNote(let path): path
        }
    }
}

/// Encrypted notes: encrypting and decrypting the note showing, its
/// options, the vault password, and locking every note again.
extension CommandRunner {
    static let encryptionHandlers: [String: Handler] = [
        "encryption.encrypt-note": { $0.encryptNoteShowing() },
        "encryption.decrypt-note": { runner in
            runner.withUnlockedNote { runner.workspace.prompt = .decrypt(path: $0.path) }
        },
        "encryption.lock": { runner in
            runner.lockEncryptedNotes()
            runner.workspace.tell("Encrypted notes are locked.")
        },
        "encryption.note-options": { runner in
            runner.withUnlockedNote { runner.workspace.sheet = .noteOptions(path: $0.path) }
        },
        "encryption.change-password": { $0.changeVaultPassword() }
    ]

    /// Runs `body` with the note showing when it's an unlocked encrypted
    /// note, or says why not.
    func withUnlockedNote(_ body: (EditingController) -> Void) {
        guard let session else {
            workspace.tell(tabs.active.path == nil ? "Open a note first." : "Unlock the note first.")
            return
        }
        guard session.isEncrypted else {
            workspace.tell("This note isn't encrypted.")
            return
        }
        body(session)
    }

    private func encryptNoteShowing() {
        withSession { session in
            guard !session.isEncrypted else {
                workspace.tell("This note is already encrypted.")
                return
            }
            guard let state = library.vault?.encryptionState() else { return }
            if state.vaultUnlocked {
                encrypt(session.path)
            } else {
                workspace.sheet = .vaultPassword(then: .encrypt(path: session.path))
            }
        }
    }

    private func changeVaultPassword() {
        guard library.vault?.encryptionState().hasVaultPassword == true else {
            workspace.tell("Encrypt a note first to set a vault password.")
            return
        }
        workspace.sheet = .changeVaultPassword
    }

    /// The editing session showing the note at `path`.
    func openSession(for path: String) -> EditingController? {
        tabs.openSessions.first { $0.path == path }
    }

    /// Carries on after the vault password was set or typed.
    func followUp(_ followUp: VaultPasswordFollowUp) {
        switch followUp {
        case .encrypt(let path):
            encrypt(path)
        case .useForNote(let path):
            setNotePassword(nil, for: path) { [weak self] _ in
                self?.workspace.tell("The note uses the vault password now.")
            }
        }
    }

    func encrypt(_ path: String) {
        guard let session = openSession(for: path) else { return }
        changeEncryption(of: session) { vault, path, text in
            try vault.encryptNote(path: path, text: text, ownPassword: nil)
        } then: { [weak self] _ in
            self?.workspace.tell("This note is encrypted.")
        }
    }

    func decrypt(_ path: String) {
        guard let session = openSession(for: path) else { return }
        changeEncryption(of: session) { vault, path, text in
            try vault.decryptNote(path: path, text: text)
        } then: { [weak self] _ in
            self?.workspace.tell("This note is plain text again.")
        }
    }

    /// Saves new options for the encrypted note at `path`; `done` gets
    /// its path, which changes when its name is hidden or shown.
    func setOptions(
        claude: NoteClaudeAccess, hidesName: Bool, for path: String, done: @escaping (String) -> Void
    ) {
        guard let session = openSession(for: path) else { return }
        changeEncryption(of: session) { vault, path, text in
            try vault.setEncryptedNoteOptions(path: path, text: text, claude: claude, hidesName: hidesName)
        } then: { done($0) }
    }

    /// Gives the note at `path` its own password, or with nil the vault's.
    /// `whenDone` runs once it has finished or failed.
    func setNotePassword(
        _ password: String?, for path: String, whenDone: @escaping () -> Void = {},
        done: @escaping (String) -> Void
    ) {
        guard let session = openSession(for: path) else {
            whenDone()
            return
        }
        let keepsKeys = EncryptionPreferences(vault: library.vault).keepsKeys
        changeEncryption(of: session) { vault, path, text in
            try vault.setNotePassword(path: path, text: text, password: password)
            if keepsKeys, let key = vault.noteKey(path: path) { KeptKeys.keep(key) }
            return path
        } then: { done($0) } whenDone: { whenDone() }
    }

    /// Runs `change` on the session's note once its waiting saves are
    /// written, so none of them lands after it, then follows the note to
    /// the path `change` answers.
    private func changeEncryption(
        of session: EditingController,
        _ change: @escaping (VaultFolder, String, String) throws -> String,
        then finish: @escaping (String) -> Void,
        whenDone: @escaping () -> Void = {}
    ) {
        guard let vault = library.vault else { return whenDone() }
        let (path, text) = (session.path, session.document.text())
        session.saveNow()
        let coordination = VaultFileCoordination.active
        NoteSaver.afterWrites { [weak self] in
            let result = Result {
                try VaultFileCoordination.write(path, with: coordination) { try change(vault, path, text) }
            }
            DispatchQueue.main.async {
                whenDone()
                self?.finishChange(result, from: path, session: session, then: finish)
            }
        }
    }

    private func finishChange(
        _ result: Result<String, Error>, from path: String, session: EditingController,
        then finish: (String) -> Void
    ) {
        switch result {
        case .success(let newPath):
            if newPath != path { tabs.renamed(from: path, to: newPath) }
            session.showTitle()
            library.edited()
            finish(newPath)
        case .failure(let error):
            workspace.tell(error.shownMessage)
        }
    }

    /// Locks every encrypted note: their tabs show them locked at once,
    /// edits not yet saved are sealed and written, and only then are the
    /// keys forgotten, so no save can come after the lock.
    func lockEncryptedNotes() {
        guard let vault = library.vault else { return }
        if case .noteOptions = workspace.sheet { workspace.sheet = nil }
        let unsaved = tabs.takeEncryptedSessions()
        let coordination = VaultFileCoordination.active
        let (tabs, library) = (tabs, library)
        NoteSaver.afterWrites {
            for note in unsaved {
                _ = try? VaultFileCoordination.write(note.path, with: coordination) {
                    try vault.saveNote(path: note.path, text: note.text)
                }
            }
            vault.lockEncryptedNotes()
            DispatchQueue.main.async {
                tabs.encryptedNotesLocked()
                library.refreshOffMain()
            }
        }
    }

    /// A note was unlocked in its tab: it opens in the editor, and names
    /// hidden until now show.
    func noteUnlocked() {
        tabs.noteUnlocked()
        library.refreshOffMain()
    }
}
