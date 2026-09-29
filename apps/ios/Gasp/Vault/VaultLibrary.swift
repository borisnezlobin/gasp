import Foundation
import Observation

/// The open vault: its folder, its notes, the theme and commands it asks
/// for. The folder is the app's own `Documents/Vault` (where sync will
/// clone the real vault later, and where the sample notes go on first
/// launch) or a folder picked with "Open another vault".
@Observable
final class VaultLibrary {
    private(set) var vault: VaultFolder?
    private(set) var notes: [NoteSummary] = []
    private(set) var folders: [String] = []
    private(set) var tokens: Tokens
    private(set) var commands: [CommandInfo] = []
    private(set) var toolbar: [CommandInfo] = []
    private(set) var keyBindings: [KeyBinding] = []
    private(set) var problem: String?
    /// Bumped whenever the vault's config is read again, so views that
    /// depend on it redraw.
    private(set) var configGeneration = 0

    init() {
        tokens = Tokens(theme: builtInTheme())
        useDataFolder(path: VaultLocation.supportFolder.path)
        open(VaultLocation.current())
    }

    var name: String {
        vault?.name() ?? "Notes"
    }

    func refresh() {
        notes = vault?.notes() ?? []
        folders = vault?.folders() ?? []
    }

    func command(_ id: String) -> CommandInfo? {
        commands.first { $0.id == id }
    }

    /// Reads `.gasp/` again after a setting changed.
    func reloadConfig() {
        vault?.reloadConfig()
        readConfig()
    }

    /// Opens another folder as the vault, remembering it for next time.
    func open(folder url: URL) {
        VaultLocation.remember(url)
        open(VaultLocation.current())
    }

    private func open(_ location: Result<URL, Error>) {
        do {
            let folder = try location.get()
            let vault = try VaultFolder.open(path: folder.path)
            self.vault = vault
            problem = nil
            vault.pruneSnapshots()
            readConfig()
            refresh()
        } catch {
            problem = error.localizedDescription
        }
    }

    private func readConfig() {
        guard let vault else { return }
        tokens = Tokens(theme: vault.theme())
        commands = vault.commands()
        toolbar = vault.toolbar()
        keyBindings = vault.keyBindings()
        configGeneration += 1
    }
}

enum VaultLocation {
    private static let bookmarkKey = "vault.bookmark"

    static var sampleFolder: URL {
        URL.documentsDirectory.appending(path: "Vault", directoryHint: .isDirectory)
    }

    /// Where the app keeps what never goes in a vault, such as snapshots.
    static var supportFolder: URL {
        let folder = URL.applicationSupportDirectory
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }

    /// The picked folder if there is one and it can still be reached,
    /// otherwise the app's own vault.
    static func current() -> Result<URL, Error> {
        if let picked = pickedFolder() {
            return .success(picked)
        }
        return Result { try installSampleIfEmpty() }
    }

    static func remember(_ url: URL) {
        guard url.startAccessingSecurityScopedResource() || url.isFileURL,
              let bookmark = try? url.bookmarkData() else { return }
        UserDefaults.standard.set(bookmark, forKey: bookmarkKey)
    }

    private static func pickedFolder() -> URL? {
        guard let data = UserDefaults.standard.data(forKey: bookmarkKey) else { return nil }
        var stale = false
        guard let url = try? URL(resolvingBookmarkData: data, bookmarkDataIsStale: &stale) else { return nil }
        _ = url.startAccessingSecurityScopedResource()
        return FileManager.default.fileExists(atPath: url.path) ? url : nil
    }

    /// Copies the bundled sample notes into the app's vault folder, unless
    /// it already exists.
    private static func installSampleIfEmpty() throws -> URL {
        let manager = FileManager.default
        let folder = sampleFolder
        guard !manager.fileExists(atPath: folder.path) else { return folder }
        guard let sample = Bundle.main.url(forResource: "SampleVault", withExtension: nil) else {
            try manager.createDirectory(at: folder, withIntermediateDirectories: true)
            return folder
        }
        try manager.copyItem(at: sample, to: folder)
        return folder
    }
}
