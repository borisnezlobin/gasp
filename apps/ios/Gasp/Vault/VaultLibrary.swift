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
    /// A vault made on this iPhone, in the app's Documents: a new one, or
    /// the sample notes. It was called the sample before there was a
    /// choice, and keeps that name in the settings it's stored in.
    case local = "sample"
    /// A folder picked in Files.
    case picked
    /// The Gasp folder in iCloud Drive, which iCloud keeps the same on
    /// every device.
    case icloud
}

/// The open vault: its folder, its notes, the theme and commands it asks
/// for. The folder is the synced clone of the notes repository once sync
/// is set up, a vault made on this iPhone, or a folder picked with "Open
/// another vault".
@Observable
final class VaultLibrary {
    private(set) var vault: VaultFolder?
    private(set) var kind: VaultKind = .local
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
    /// Light, dark or the phone's own, from `appearance.theme`.
    private(set) var appearance: Appearance = .system
    private(set) var problem: String?
    /// Bumped whenever the vault's config is read again, so views that
    /// depend on it redraw.
    private(set) var configGeneration = 0

    /// Opens the vault chosen last, unless `opensVault` is false because
    /// none has been chosen yet and the welcome tour will ask.
    init(opensVault: Bool = true) {
        tokens = Tokens.forReader(theme: builtInTheme())
        useDataFolder(path: VaultLocation.supportFolder.path)
        if opensVault { open(VaultLocation.current()) }
        NotificationCenter.default.addObserver(
            forName: UIContentSizeCategory.didChangeNotification, object: nil, queue: .main
        ) { [weak self] _ in self?.readConfig() }
    }

    var name: String {
        vault?.name() ?? "Notes"
    }

    /// Whether the open vault is one made on this iPhone, not the sample
    /// notes: the notes setting up sync can bring along.
    var isThisPhonesOwn: Bool {
        guard kind == .local, let folder else { return false }
        return !folder.lastPathComponent.hasPrefix(VaultLocation.sampleVaultName)
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

    /// Opens the Gasp folder in iCloud Drive as the vault from now on.
    func open(icloudFolder url: URL) {
        VaultLocation.rememberICloud(url)
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
        appearance = vault.appearance()
        configGeneration += 1
    }
}

enum VaultLocation {
    private static let bookmarkKey = "vault.bookmark"
    private static let icloudBookmarkKey = "vault.icloud-bookmark"
    private static let choiceKey = "vault.choice"
    /// The synced clone's path inside `Documents`, which keeps working when
    /// an update moves the app's container.
    private static let syncedKey = "sync.folder"
    /// The vault made on this iPhone, also inside `Documents`. Before it
    /// was stored, it was always `Vault`.
    private static let localKey = "vault.local"

    /// The sample vault's folder name; a number follows when it's taken.
    static let sampleVaultName = "Gasp sample"
    /// The note the sample vault opens on.
    static let sampleFirstNote = "Start here.md"
    /// A new vault's folder name.
    static let newVaultName = "Notes"

    static var localFolder: URL {
        let relative = UserDefaults.standard.string(forKey: localKey) ?? "Vault"
        return URL.documentsDirectory.appending(path: relative, directoryHint: .isDirectory)
    }

    /// Where the app keeps what never goes in a vault, such as snapshots.
    static var supportFolder: URL {
        let folder = URL.applicationSupportDirectory
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }

    /// Whether a vault was ever chosen or made on this iPhone, so launching
    /// can open it instead of asking.
    static var isSetUp: Bool {
        let defaults = UserDefaults.standard
        return defaults.string(forKey: choiceKey) != nil || syncedFolder != nil
            || defaults.data(forKey: bookmarkKey) != nil
            || defaults.data(forKey: icloudBookmarkKey) != nil
            || FileManager.default.fileExists(atPath: localFolder.path)
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
        return unusedFolder(in: parent, named: base)
    }

    static func rememberSynced(_ folder: URL) {
        UserDefaults.standard.set(relativeToDocuments(folder), forKey: syncedKey)
        choice = .synced
    }

    /// Makes an empty vault in Documents and chooses it.
    static func makeNewVault() throws -> URL {
        let folder = unusedFolder(in: URL.documentsDirectory, named: newVaultName)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        chooseLocal(folder)
        return folder
    }

    /// Writes the bundled sample notes into a new vault in Documents and
    /// chooses it.
    static func makeSampleVault() throws -> URL {
        let folder = unusedFolder(in: URL.documentsDirectory, named: sampleVaultName)
        if let sample = Bundle.main.url(forResource: "SampleVault", withExtension: nil) {
            try FileManager.default.copyItem(at: sample, to: folder)
        } else {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        }
        chooseLocal(folder)
        return folder
    }

    private static func chooseLocal(_ folder: URL) {
        UserDefaults.standard.set(relativeToDocuments(folder), forKey: localKey)
        choice = .local
    }

    private static func relativeToDocuments(_ folder: URL) -> String {
        let documents = URL.documentsDirectory.standardizedFileURL.path
        let path = folder.standardizedFileURL.path
        return path.hasPrefix(documents) ? String(path.dropFirst(documents.count + 1)) : path
    }

    /// `parent/name`, or `parent/name 2` and so on when that's taken.
    static func unusedFolder(in parent: URL, named name: String) -> URL {
        var folder = parent.appending(path: name, directoryHint: .isDirectory)
        var number = 2
        while FileManager.default.fileExists(atPath: folder.path) {
            folder = parent.appending(path: "\(name) \(number)", directoryHint: .isDirectory)
            number += 1
        }
        return folder
    }

    /// Which vault opens: the one last chosen, or the synced notes when
    /// there are some, or the iCloud folder, or a picked folder, or the one
    /// made on this iPhone.
    static var choice: VaultKind {
        get {
            if let stored = UserDefaults.standard.string(forKey: choiceKey).flatMap(VaultKind.init) {
                return stored
            }
            if syncedFolder != nil { return .synced }
            if UserDefaults.standard.data(forKey: icloudBookmarkKey) != nil { return .icloud }
            return UserDefaults.standard.data(forKey: bookmarkKey) != nil ? .picked : .local
        }
        set { UserDefaults.standard.set(newValue.rawValue, forKey: choiceKey) }
    }

    /// The chosen vault if it can still be reached, otherwise the one made
    /// on this iPhone.
    static func current() -> Result<(VaultKind, URL), Error> {
        switch choice {
        case .synced:
            if let folder = syncedFolder { return .success((.synced, folder)) }
        case .picked:
            if let folder = pickedFolder() { return .success((.picked, folder)) }
        case .icloud:
            if let folder = icloudFolder { return .success((.icloud, folder)) }
        case .local:
            break
        }
        return Result { (.local, try existingLocalFolder()) }
    }

    static func remember(_ url: URL) {
        bookmark(url, under: bookmarkKey)
    }

    #if DEBUG
    /// `-icloudVault <path>` opens a folder as the iCloud vault, for
    /// screenshots in the simulator, where nothing can be picked.
    static func useICloudVaultFromArguments() {
        guard let path = UserDefaults.standard.string(forKey: "icloudVault") else { return }
        let folder = URL(fileURLWithPath: path, isDirectory: true)
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        rememberICloud(folder)
    }
    #endif

    /// Keeps the Gasp folder picked in iCloud Drive and chooses it.
    static func rememberICloud(_ url: URL) {
        bookmark(url, under: icloudBookmarkKey)
        choice = .icloud
    }

    static var hasPickedFolder: Bool {
        pickedFolder() != nil
    }

    /// The vault in iCloud Drive: the Gasp folder, picked once in Files
    /// and kept with a bookmark. The one place that knows how the phone
    /// reaches it.
    static var icloudFolder: URL? {
        bookmarkedFolder(under: icloudBookmarkKey)
    }

    private static func bookmark(_ url: URL, under key: String) {
        guard url.startAccessingSecurityScopedResource() || url.isFileURL,
              let bookmark = try? url.bookmarkData() else { return }
        UserDefaults.standard.set(bookmark, forKey: key)
    }

    private static func pickedFolder() -> URL? {
        bookmarkedFolder(under: bookmarkKey)
    }

    private static func bookmarkedFolder(under key: String) -> URL? {
        guard let data = UserDefaults.standard.data(forKey: key) else { return nil }
        var stale = false
        guard let url = try? URL(resolvingBookmarkData: data, bookmarkDataIsStale: &stale) else { return nil }
        _ = url.startAccessingSecurityScopedResource()
        if stale { bookmark(url, under: key) }
        return FileManager.default.fileExists(atPath: url.path) ? url : nil
    }

    /// The vault made on this iPhone, made again empty if it was deleted.
    private static func existingLocalFolder() throws -> URL {
        let folder = localFolder
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }
}
