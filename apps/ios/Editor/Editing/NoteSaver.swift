import Foundation

/// Writes a note back through the core a moment after the last change, off
/// the main thread, so typing never waits on the disk. Called on the main
/// thread.
final class NoteSaver {
    private let vault: VaultFolder
    private let path: String
    private let disk = DispatchQueue(label: "com.borisnezlobin.editor.save")
    private var unsavedText: String?
    private var timer: DispatchWorkItem?
    private static let quietPeriod: TimeInterval = 0.8

    init(vault: VaultFolder, path: String) {
        self.vault = vault
        self.path = path
    }

    func schedule(_ text: String) {
        unsavedText = text
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
        let vault = vault
        let path = path
        disk.async {
            do {
                try vault.saveNote(path: path, text: text)
            } catch {
                NSLog("Couldn't save \(path): \(error.localizedDescription)")
            }
        }
    }
}
