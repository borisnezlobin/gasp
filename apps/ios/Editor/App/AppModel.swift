import Foundation
import Observation

/// The app's state: the vault, the browser around it, and the command
/// runner that joins them. Opening another vault starts a fresh browser.
@Observable
final class AppModel {
    let library = VaultLibrary()
    let workspace = Workspace()
    private(set) var tabs: TabStore
    private(set) var runner: CommandRunner

    init() {
        let tabs = TabStore(library: library)
        self.tabs = tabs
        runner = CommandRunner(library: library, tabs: tabs, workspace: workspace)
    }

    func switchVault(to folder: URL) {
        tabs.saveAllNotes()
        library.open(folder: folder)
        let tabs = TabStore(library: library)
        self.tabs = tabs
        runner = CommandRunner(library: library, tabs: tabs, workspace: workspace)
    }
}
