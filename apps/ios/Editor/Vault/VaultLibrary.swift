import Foundation
import Observation

/// The open vault: its folder, its notes and the theme it asks for. The
/// folder lives in the app's Documents, where sync will clone the real
/// vault later; for now the sample vault is copied there on first launch.
@Observable
final class VaultLibrary {
    private(set) var vault: VaultFolder?
    private(set) var notes: [NoteSummary] = []
    private(set) var tokens: Tokens
    private(set) var problem: String?

    init() {
        tokens = Tokens(theme: builtInTheme())
        open()
    }

    var name: String {
        vault?.name() ?? "Notes"
    }

    func refresh() {
        notes = vault?.notes() ?? []
    }

    private func open() {
        do {
            let folder = try VaultLocation.installSampleIfEmpty()
            let vault = try VaultFolder.open(path: folder.path)
            self.vault = vault
            tokens = Tokens(theme: vault.theme())
            refresh()
        } catch {
            problem = error.localizedDescription
        }
    }
}

enum VaultLocation {
    static var folder: URL {
        URL.documentsDirectory.appending(path: "Vault", directoryHint: .isDirectory)
    }

    /// Copies the bundled sample notes into the vault folder, unless it
    /// already exists.
    static func installSampleIfEmpty() throws -> URL {
        let manager = FileManager.default
        guard !manager.fileExists(atPath: folder.path) else { return folder }
        guard let sample = Bundle.main.url(forResource: "SampleVault", withExtension: nil) else {
            try manager.createDirectory(at: folder, withIntermediateDirectories: true)
            return folder
        }
        try manager.copyItem(at: sample, to: folder)
        return folder
    }
}
