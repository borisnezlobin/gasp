import Foundation

/// Writes a note back through the core a moment after the last change, off
/// the main thread, so typing never waits on the disk. Called on the main
/// thread.
final class NoteSaver {
    private let vault: VaultFolder
    var path: String
    private let disk = DispatchQueue(label: "com.borisnezlobin.editor.save")
    private var unsavedText: String?
    private var afterSave: (() -> Void)?
    private var timer: DispatchWorkItem?
    private static let quietPeriod: TimeInterval = 0.8

    init(vault: VaultFolder, path: String) {
        self.vault = vault
        self.path = path
    }

    /// Saves `text` once typing stops, then runs `afterSave` on the main
    /// thread.
    func schedule(_ text: String, afterSave: @escaping () -> Void) {
        unsavedText = text
        self.afterSave = afterSave
        timer?.cancel()
        let timer = DispatchWorkItem { [weak self] in self?.flush() }
        self.timer = timer
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.quietPeriod, execute: timer)
    }

    /// Saves anything still waiting, now.
    func flush() {
        timer?.cancel()
        guard let text = unsavedText else { return }
        unsavedText = nil
        let (vault, path, afterSave) = (vault, path, afterSave)
        disk.async {
            do {
                try vault.saveNote(path: path, text: text)
                DispatchQueue.main.async { afterSave?() }
            } catch {
                NSLog("Couldn't save \(path): \(error.localizedDescription)")
            }
        }
    }
}
