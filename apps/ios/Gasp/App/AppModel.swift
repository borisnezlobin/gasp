import Foundation
import Observation
import SwiftUI

/// The app's state: the vault, the browser around it, sync, and the
/// command runner that joins them. Opening another vault starts a fresh
/// browser.
@Observable
final class AppModel {
    let library = VaultLibrary()
    let workspace = Workspace()
    let sync = SyncCenter()
    private(set) var tabs: TabStore
    private(set) var runner: CommandRunner

    init() {
        let tabs = TabStore(library: library)
        self.tabs = tabs
        runner = CommandRunner(library: library, tabs: tabs, workspace: workspace, sync: sync)
        sync.beforeSync = { [weak self] in self?.tabs.saveAllNotes() }
        sync.onNotesChanged = { [weak self] paths in self?.notesChangedOnDisk(paths) }
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

    /// Opens the clone sync just made.
    func openSyncedVault(at folder: URL) {
        VaultLocation.rememberSynced(folder)
        switchVault(to: .synced)
    }

    /// The app came to the front, or went away.
    func sceneChanged(to phase: ScenePhase) {
        switch phase {
        case .active: sync.syncForAppEvent()
        case .background: sync.syncForBackground()
        default: break
        }
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
    /// folder another app may be syncing.
    private func attachSync() {
        sync.attach(to: library.kind == .synced ? library.folder : nil)
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
