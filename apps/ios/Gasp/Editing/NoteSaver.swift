import Foundation

/// Writes a note back through the core a moment after the last change, off
/// the main thread, so typing never waits on the disk. In the iCloud vault
/// the write is coordinated with iCloud. Called on the main thread.
final class NoteSaver {
    private let vault: VaultFolder
    var path: String
    private static let disk = DispatchQueue(label: "com.borisnezlobin.gasp.save")
    /// Reads the text to save, once there's text waiting.
    private var unsavedText: (() -> String)?
    private var afterSave: ((String) -> Void)?
    private var timer: DispatchWorkItem?
    private static let quietPeriod: TimeInterval = 0.8

    init(vault: VaultFolder, path: String) {
        self.vault = vault
        self.path = path
    }

    /// Saves the text `read` gives once typing stops, then runs
    /// `afterSave` with it on the main thread. The text is read only then,
    /// so typing never copies the note.
    func schedule(_ read: @escaping () -> String, afterSave: @escaping (String) -> Void) {
        unsavedText = read
        self.afterSave = afterSave
        timer?.cancel()
        let timer = DispatchWorkItem { [weak self] in self?.flush() }
        self.timer = timer
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.quietPeriod, execute: timer)
    }

    /// Waits until every save handed to the disk so far is written, so a
    /// sync commits it. Never call it on the main thread.
    static func waitForWrites() {
        Self.disk.sync {}
    }

    /// Forgets text waiting to be saved, for a note sync is replacing.
    func discardUnsaved() {
        timer?.cancel()
        unsavedText = nil
    }

    /// Saves anything still waiting, now.
    func flush() {
        timer?.cancel()
        guard let read = unsavedText else { return }
        unsavedText = nil
        let text = read()
        let (vault, path, afterSave) = (vault, path, afterSave)
        let coordination = VaultFileCoordination.active
        Self.disk.async {
            do {
                try VaultFileCoordination.write(path, with: coordination) {
                    try vault.saveNote(path: path, text: text)
                }
                DispatchQueue.main.async {
                    afterSave?(text)
                    NotificationCenter.default.post(name: .vaultEdited, object: nil)
                }
            } catch {
                NSLog("Couldn't save \(path): \(error.localizedDescription)")
            }
        }
    }
}
