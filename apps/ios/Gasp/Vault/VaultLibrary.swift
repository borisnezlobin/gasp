import Foundation
import Observation
import UIKit

extension Notification.Name {
    /// This phone changed a note or the vault's config, which sync sends on.
    static let vaultEdited = Notification.Name("com.borisnezlobin.gasp.vault-edited")
}

/// Where the open vault comes from.
enum VaultKind: String {
    /// The notes repository sync cloned into the app.
    case synced
    /// The sample notes that ship with the app, for when nothing syncs.
    case sample
    /// A folder picked in Files.
    case picked
}

/// The open vault: its folder, its notes, the theme and commands it asks
/// for. The folder is the synced clone of the notes repository once sync
/// is set up, the bundled sample notes until then, or a folder picked
/// with "Open another vault".
@Observable
final class VaultLibrary {
    private(set) var vault: VaultFolder?
    private(set) var kind: VaultKind = .sample
    private(set) var folder: URL?
    private(set) var notes: [NoteSummary] = []
    private(set) var folders: [String] = []
    private(set) var tokens: Tokens
    private(set) var commands: [CommandInfo] = []
    /// The bar above the software keyboard: the `keyboard` toolbar in
    /// toolbars.toml.
    private(set) var keyboardToolbar = PhoneToolbar(enabled: true, labels: .icons, entries: [])
    /// The bar at the bottom of the screen: the `browser-bar` toolbar.
    private(set) var browserBar = PhoneToolbar(enabled: true, labels: .icons, entries: [.spacer])
    private(set) var keyBindings: [KeyBinding] = []
    private(set) var problem: String?
    /// Bumped whenever the vault's config is read again, so views that
    /// depend on it redraw.
    private(set) var configGeneration = 0

    init() {
        tokens = Tokens.forReader(theme: builtInTheme())
        useDataFolder(path: VaultLocation.supportFolder.path)
        open(VaultLocation.current())
        NotificationCenter.default.addObserver(
            forName: UIContentSizeCategory.didChangeNotification, object: nil, queue: .main
        ) { [weak self] _ in self?.readConfig() }
    }

    var name: String {
        vault?.name() ?? "Notes"
    }

    func refresh() {
        notes = vault?.notes() ?? []
        folders = vault?.folders() ?? []
    }

    /// Lists the notes again after this phone changed the vault, and tells sync.
    func edited() {
        refresh()
        NotificationCenter.default.post(name: .vaultEdited, object: nil)
    }

    func command(_ id: String) -> CommandInfo? {
        commands.first { $0.id == id }
    }

    /// Reads `.gasp/` again after a setting changed on this phone.
    func reloadConfig() {
        readConfigFromDisk()
        NotificationCenter.default.post(name: .vaultEdited, object: nil)
    }

    /// Reads `.gasp/` again after sync brought in another device's change.
    func readConfigFromDisk() {
        vault?.reloadConfig()
        readConfig()
    }

    /// Opens another folder as the vault, remembering it for next time.
    func open(folder url: URL) {
        VaultLocation.remember(url)
        VaultLocation.choice = .picked
        open(VaultLocation.current())
    }

    /// Opens the synced notes, the sample notes or the picked folder.
    func open(_ kind: VaultKind) {
        VaultLocation.choice = kind
        open(VaultLocation.current())
    }

    private func open(_ location: Result<(VaultKind, URL), Error>) {
        do {
            let (kind, folder) = try location.get()
            let vault = try VaultFolder.open(path: folder.path)
            self.vault = vault
            self.kind = kind
            self.folder = folder
            problem = nil
            vault.pruneSnapshots()
            readConfig()
            refresh()
        } catch {
            problem = error.shownMessage
        }
    }

    private func readConfig() {
        guard let vault else { return }
        tokens = Tokens.forReader(theme: vault.theme())
        commands = vault.commands()
        keyboardToolbar = vault.keyboardToolbar()
        browserBar = vault.browserBar()
        keyBindings = vault.keyBindings()
        configGeneration += 1
    }
}

enum VaultLocation {
    private static let bookmarkKey = "vault.bookmark"
    private static let choiceKey = "vault.choice"
    /// The synced clone's path inside `Documents`, which keeps working when
    /// an update moves the app's container.
    private static let syncedKey = "sync.folder"

    static var sampleFolder: URL {
        URL.documentsDirectory.appending(path: "Vault", directoryHint: .isDirectory)
    }

    /// Where the app keeps what never goes in a vault, such as snapshots.
    static var supportFolder: URL {
        let folder = URL.applicationSupportDirectory
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }

    /// The clone sync set up, if there is one on this phone.
    static var syncedFolder: URL? {
        guard let relative = UserDefaults.standard.string(forKey: syncedKey) else { return nil }
        let folder = URL.documentsDirectory.appending(path: relative, directoryHint: .isDirectory)
        return FileManager.default.fileExists(atPath: folder.path) ? folder : nil
    }

    /// A folder for a new clone of `repository`, named after it.
    static func newSyncedFolder(for repository: String) -> URL {
        let parent = URL.documentsDirectory.appending(path: "Synced", directoryHint: .isDirectory)
        let last = repository.split(separator: "/").last.map(String.init) ?? "Notes"
        let base = last.hasSuffix(".git") ? String(last.dropLast(4)) : last
        var folder = parent.appending(path: base, directoryHint: .isDirectory)
        var number = 2
        while FileManager.default.fileExists(atPath: folder.path) {
            folder = parent.appending(path: "\(base) \(number)", directoryHint: .isDirectory)
            number += 1
        }
        return folder
    }

    static func rememberSynced(_ folder: URL) {
        let documents = URL.documentsDirectory.standardizedFileURL.path
        let path = folder.standardizedFileURL.path
        let relative = path.hasPrefix(documents) ? String(path.dropFirst(documents.count + 1)) : path
        UserDefaults.standard.set(relative, forKey: syncedKey)
        choice = .synced
    }

    /// Which vault opens: the one last chosen, or the synced notes when
    /// there are some, or a picked folder, or the sample notes.
    static var choice: VaultKind {
        get {
            if let stored = UserDefaults.standard.string(forKey: choiceKey).flatMap(VaultKind.init) {
                return stored
            }
            if syncedFolder != nil { return .synced }
            return UserDefaults.standard.data(forKey: bookmarkKey) != nil ? .picked : .sample
        }
        set { UserDefaults.standard.set(newValue.rawValue, forKey: choiceKey) }
    }

    /// The chosen vault if it can still be reached, otherwise the sample notes.
    static func current() -> Result<(VaultKind, URL), Error> {
        switch choice {
        case .synced:
            if let folder = syncedFolder { return .success((.synced, folder)) }
        case .picked:
            if let folder = pickedFolder() { return .success((.picked, folder)) }
        case .sample:
            break
        }
        return Result { (.sample, try installSampleIfEmpty()) }
    }

    static func remember(_ url: URL) {
        guard url.startAccessingSecurityScopedResource() || url.isFileURL,
              let bookmark = try? url.bookmarkData() else { return }
        UserDefaults.standard.set(bookmark, forKey: bookmarkKey)
    }

    static var hasPickedFolder: Bool {
        pickedFolder() != nil
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
