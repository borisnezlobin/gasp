import Foundation
import Observation

/// One tab: a note, or the start page a new tab opens on, and where it's
/// been, for back and forward.
struct BrowserTab: Identifiable, Equatable {
    let id = UUID()
    var path: String?
    var back: [String] = []
    var forward: [String] = []
}

/// What a tab's page shows.
enum TabContent {
    case note(EditingController)
    /// An encrypted note there's no key for yet.
    case locked(path: String, note: LockedNote)
    /// A note that can't be read, such as one that was moved away.
    case missing
    case start
}

/// The open tabs, like Safari's: which is showing, the ones closed
/// recently, and one editing session per open note so switching back
/// keeps its cursor, scroll and undo. The open notes are saved to the
/// vault's `.gasp/device.toml`, which never syncs.
@Observable
final class TabStore {
    private(set) var tabs: [BrowserTab] = []
    private(set) var activeIndex = 0
    /// Bumped when notes open in tabs were read again from disk, so their
    /// pages are rebuilt.
    private(set) var generation = 0
    private var closed: [BrowserTab] = []
    @ObservationIgnored private var sessions: [UUID: EditingController] = [:]
    /// Where the cursor was in a note sync replaced, for the session that
    /// reads it again.
    @ObservationIgnored private var cursorsToKeep: [UUID: Int] = [:]
    @ObservationIgnored private let library: VaultLibrary
    @ObservationIgnored var makeSession: ((String, String) -> EditingController?)?
    /// Set while encrypted notes are being locked, so their pages show
    /// locked before the lock lands on disk.
    @ObservationIgnored private var locking = false
    /// The locked notes tabs show, so a page asks the disk only once.
    @ObservationIgnored private var lockedPages: [UUID: (path: String, note: LockedNote)] = [:]

    init(library: VaultLibrary) {
        self.library = library
        restore()
    }

    var active: BrowserTab {
        tabs[activeIndex]
    }

    var activeSession: EditingController? {
        session(for: active)
    }

    /// Every note being edited in a tab.
    var openSessions: [EditingController] {
        Array(sessions.values)
    }

    func session(for tab: BrowserTab) -> EditingController? {
        guard case .note(let session) = content(for: tab) else { return nil }
        return session
    }

    func content(for tab: BrowserTab) -> TabContent {
        guard let path = tab.path else { return .start }
        if let session = sessions[tab.id], session.path == path { return .note(session) }
        if let locked = lockedPages[tab.id], locked.path == path { return .locked(path: path, note: locked.note) }
        guard let opened = opened(path) else { return .missing }
        switch opened {
        case .plain(let text), .unlocked(let text):
            return startSession(for: tab, path: path, text: text).map(TabContent.note) ?? .missing
        case .locked(let note):
            if !locking { lockedPages[tab.id] = (path, note) }
            return .locked(path: path, note: note)
        }
    }

    private func opened(_ path: String) -> OpenedNote? {
        guard let vault = library.vault else { return nil }
        if locking, vault.isNoteUnlocked(path: path) {
            return .locked(note: LockedNote(hasOwnPassword: false, keyId: nil))
        }
        return try? VaultFileCoordination.read(path, { try vault.openNote(path: path) })
    }

    private func startSession(for tab: BrowserTab, path: String, text: String) -> EditingController? {
        guard let session = makeSession?(path, text) else { return nil }
        sessions[tab.id]?.saveNow()
        sessions[tab.id] = session
        if let cursor = cursorsToKeep.removeValue(forKey: tab.id) { session.placeCursor(at: cursor) }
        return session
    }

    func title(of tab: BrowserTab) -> String {
        guard let path = tab.path else { return "New tab" }
        return library.notes.first { $0.path == path }?.title
            ?? URL(fileURLWithPath: path).deletingPathExtension().lastPathComponent
    }

    // MARK: Opening

    /// Shows `path` in the active tab, remembering where the tab was.
    func open(_ path: String) {
        var tab = active
        guard tab.path != path else { return }
        if let current = tab.path { tab.back.append(current) }
        tab.forward = []
        tab.path = path
        tabs[activeIndex] = tab
        save()
    }

    /// Opens `path` in a tab of its own, or shows the tab it's already in.
    func openInNewTab(_ path: String) {
        if let index = tabs.firstIndex(where: { $0.path == path }) {
            select(index)
            return
        }
        tabs.insert(BrowserTab(path: path), at: activeIndex + 1)
        activeIndex += 1
        save()
    }

    func newTab() {
        tabs.append(BrowserTab(path: nil))
        activeIndex = tabs.count - 1
        save()
    }

    func goBack() {
        var tab = active
        guard let previous = tab.back.popLast() else { return }
        if let current = tab.path { tab.forward.append(current) }
        tab.path = previous
        tabs[activeIndex] = tab
        save()
    }

    func goForward() {
        var tab = active
        guard let next = tab.forward.popLast() else { return }
        if let current = tab.path { tab.back.append(current) }
        tab.path = next
        tabs[activeIndex] = tab
        save()
    }

    // MARK: Switching and closing

    func select(_ index: Int) {
        guard tabs.indices.contains(index) else { return }
        activeIndex = index
        save()
    }

    func selectNext() {
        select((activeIndex + 1) % tabs.count)
    }

    func selectPrevious() {
        select((activeIndex - 1 + tabs.count) % tabs.count)
    }

    func close(_ id: UUID) {
        guard let index = tabs.firstIndex(where: { $0.id == id }) else { return }
        sessions.removeValue(forKey: id)?.saveNow()
        closed.append(tabs.remove(at: index))
        if tabs.isEmpty { tabs = [BrowserTab(path: nil)] }
        if index < activeIndex || activeIndex >= tabs.count { activeIndex = max(activeIndex - 1, 0) }
        save()
    }

    func closeOthers() {
        tabs.filter { $0.id != active.id }.forEach { close($0.id) }
    }

    func closeToTheRight() {
        tabs.suffix(from: activeIndex + 1).forEach { close($0.id) }
    }

    func reopenClosed() {
        guard let tab = closed.popLast() else { return }
        tabs.insert(tab, at: min(activeIndex + 1, tabs.count))
        activeIndex = tabs.firstIndex(of: tab) ?? activeIndex
        save()
    }

    /// Follows a note that was renamed or moved.
    func renamed(from old: String, to new: String) {
        tabs = tabs.map { tab in
            var tab = tab
            if tab.path == old { tab.path = new }
            tab.back = tab.back.map { $0 == old ? new : $0 }
            tab.forward = tab.forward.map { $0 == old ? new : $0 }
            return tab
        }
        sessions.values.filter { $0.path == old }.forEach { $0.path = new }
        save()
    }

    /// Follows the notes inside a folder that was moved or renamed.
    func folderMoved(from old: String, to new: String) {
        let follow = { (path: String) in VaultPaths.path(path, afterMovingFolder: old, to: new) }
        tabs = tabs.map { tab in
            var tab = tab
            tab.path = tab.path.map(follow)
            tab.back = tab.back.map(follow)
            tab.forward = tab.forward.map(follow)
            return tab
        }
        sessions.values.forEach { $0.path = follow($0.path) }
        save()
    }

    /// Closes every tab showing a note that's gone.
    func removed(_ path: String) {
        tabs.filter { $0.path == path }.forEach { close($0.id) }
    }

    func saveAllNotes() {
        sessions.values.forEach { $0.saveNow() }
    }

    /// Reads notes open in tabs again after sync changed them on disk. A
    /// tab whose text already matches the disk keeps its session.
    func reload(_ paths: [String]) {
        let changed = Set(paths)
        let replaced = sessions.filter { _, session in
            changed.contains(session.path) && yieldsToDisk(session)
        }
        guard !replaced.isEmpty else { return }
        for (id, session) in replaced {
            session.retire()
            cursorsToKeep[id] = session.textView.selectedRange.location
            sessions.removeValue(forKey: id)
        }
        generation += 1
    }

    /// Whether a session should give way to the note sync wrote. Edits not
    /// saved yet are folded into it first; when they clash with it on the
    /// same lines the session stays, and its edits save over the note.
    private func yieldsToDisk(_ session: EditingController) -> Bool {
        guard let vault = library.vault, let onDisk = textOnDisk(session.path, in: vault) else { return false }
        let text = session.document.text()
        guard onDisk != text else { return false }
        guard session.hasUnsavedEdits else { return true }
        guard let merged = mergeNoteEdits(base: session.savedText, edited: text, synced: onDisk),
              (try? VaultFileCoordination.write(session.path, with: VaultFileCoordination.active, {
                  try vault.saveNote(path: session.path, text: merged)
              })) != nil else { return false }
        NotificationCenter.default.post(name: .vaultEdited, object: nil)
        return true
    }

    // MARK: Encrypted notes

    /// Rebuilds the pages after a note was unlocked.
    func noteUnlocked() {
        lockedPages = [:]
        generation += 1
    }

    /// Takes every encrypted note out of its tab, which shows it locked
    /// from now on, and answers the text of those with edits not yet saved.
    func takeEncryptedSessions() -> [(path: String, text: String)] {
        locking = true
        let encrypted = sessions.filter { _, session in session.isEncrypted }
        var unsaved: [(path: String, text: String)] = []
        for (id, session) in encrypted {
            if session.hasUnsavedEdits { unsaved.append((session.path, session.document.text())) }
            session.retire()
            sessions.removeValue(forKey: id)
        }
        if !encrypted.isEmpty { generation += 1 }
        return unsaved
    }

    /// The notes have locked on disk too.
    func encryptedNotesLocked() {
        locking = false
        let activeShowsNoEditor = active.path != nil && sessions[active.id] == nil
        let showedLocked = !lockedPages.isEmpty || activeShowsNoEditor
        lockedPages = [:]
        if showedLocked { generation += 1 }
    }

    /// The note's text on disk, opened when it's encrypted; nil when it
    /// can't be read or opened, so the editor's text stays.
    private func textOnDisk(_ path: String, in vault: VaultFolder) -> String? {
        switch try? vault.openNote(path: path) {
        case .plain(let text), .unlocked(let text): text
        case .locked, nil: nil
        }
    }

    // MARK: Saving

    private func restore() {
        let saved = library.vault?.openTabs()
        let known = Set(library.notes.map(\.path))
        tabs = (saved?.paths ?? []).filter(known.contains).map { BrowserTab(path: $0) }
        if tabs.isEmpty { tabs = [BrowserTab(path: nil)] }
        activeIndex = min(Int(saved?.active ?? 0), tabs.count - 1)
    }

    private func save() {
        let noteTabs = tabs.enumerated().filter { $0.element.path != nil }
        let active = noteTabs.firstIndex { $0.offset == activeIndex }
        let open = OpenTabs(paths: noteTabs.compactMap(\.element.path), active: active.map(UInt32.init))
        try? library.vault?.saveOpenTabs(tabs: open)
    }
}
