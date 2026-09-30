import Foundation
import Observation
import SwiftUI

/// The app's state: the vault, the browser around it, sync, and the
/// command runner that joins them. Opening another vault starts a fresh
/// browser.
@Observable
final class AppModel {
    let library: VaultLibrary
    let workspace = Workspace()
    let sync = SyncCenter()
    let icloud = ICloudCenter()
    let welcome: WelcomeFlow
    private(set) var tabs: TabStore
    private(set) var runner: CommandRunner

    init() {
        #if DEBUG
        VaultLocation.useICloudVaultFromArguments()
        #endif
        let welcomeIsDue = WelcomeFlow.isDue(vaultIsSetUp: VaultLocation.isSetUp)
        library = VaultLibrary(opensVault: !welcomeIsDue)
        welcome = WelcomeFlow(isFirstRun: welcomeIsDue)
        let tabs = TabStore(library: library)
        self.tabs = tabs
        runner = CommandRunner(library: library, tabs: tabs, workspace: workspace, sync: sync)
        sync.beforeSync = { [weak self] in self?.tabs.saveAllNotes() }
        sync.onNotesChanged = { [weak self] paths in self?.notesChangedOnDisk(paths) }
        icloud.onNotesChanged = { [weak self] paths in self?.notesChangedOnDisk(paths) }
        NotificationCenter.default.addObserver(forName: .vaultEdited, object: nil, queue: .main) { [weak self] _ in
            self?.sync.noteEdited()
        }
        attachSync()
    }

    func switchVault(to folder: URL) {
        reopen { library.open(folder: folder) }
    }

    func switchVault(to kind: VaultKind) {
        reopen { library.open(kind) }
    }

    /// Makes an empty vault on this iPhone and opens it.
    func openNewVault() throws {
        _ = try VaultLocation.makeNewVault()
        switchVault(to: .local)
    }

    /// Writes the sample notes into a new vault, opens it and shows its
    /// first note.
    func openSampleVault() throws {
        _ = try VaultLocation.makeSampleVault()
        switchVault(to: .local)
        tabs.open(VaultLocation.sampleFirstNote)
    }

    /// Opens the clone sync just made.
    func openSyncedVault(at folder: URL) {
        VaultLocation.rememberSynced(folder)
        switchVault(to: .synced)
    }

    /// Opens the Gasp folder in iCloud Drive as the vault.
    func openICloudVault(at folder: URL) {
        reopen { library.open(icloudFolder: folder) }
    }

    /// Whether the open vault already syncs, with git or through iCloud.
    var vaultSyncs: Bool {
        library.kind == .synced || library.kind == .icloud
    }

    /// The app came to the front, or went away.
    func sceneChanged(to phase: ScenePhase) {
        switch phase {
        case .active:
            sync.syncForAppEvent()
            icloud.rescan()
            UsagePing.sendIfDue { [weak self] in self?.allowsUsagePing ?? false }
        case .background: sync.syncForBackground()
        default: break
        }
    }

    /// The vault's `telemetry.enabled`, or its default while no vault is open.
    private var allowsUsagePing: Bool {
        let setting = library.vault?.settings().first { $0.key == UsagePing.settingKey }
        guard case .bool(let enabled) = setting?.value else { return true }
        return enabled
    }

    private func reopen(_ open: () -> Void) {
        tabs.saveAllNotes()
        open()
        let tabs = TabStore(library: library)
        self.tabs = tabs
        runner = CommandRunner(library: library, tabs: tabs, workspace: workspace, sync: sync)
        attachSync()
    }

    /// Sync runs on the clone it set up, never on the sample notes or a
    /// folder another app may be syncing. iCloud syncs its own folder, so
    /// the iCloud vault gets a watcher instead.
    private func attachSync() {
        sync.attach(to: library.kind == .synced ? library.folder : nil)
        icloud.attach(to: library.kind == .icloud ? library.folder : nil)
    }

    private func notesChangedOnDisk(_ paths: [String]) {
        library.vault?.filesChanged(paths: paths)
        library.refresh()
        if paths.contains(where: { $0.hasPrefix(configFolder() + "/") }) {
            library.readConfigFromDisk()
        }
        tabs.reload(paths)
    }
}
