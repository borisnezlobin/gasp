import BackgroundTasks
import Foundation
import Observation
import UIKit

/// Sync for the open vault. The core decides when and how (the desktop's
/// scheduler and merge policy); this runs its git work on a background
/// queue and tells the browser what changed. It syncs when the app
/// opens, comes back and goes away, on the desktop's idle schedule after
/// an edit, and now and then in the background.
@Observable
final class SyncCenter {
    static let refreshTaskIdentifier = "com.borisnezlobin.editor.sync"

    /// Nil when the open vault isn't the synced one.
    private(set) var overview: SyncOverview?
    private(set) var conflicts: [ConflictNote] = []
    private(set) var isRunning = false

    /// Notes a sync or a resolution changed on disk, for open notes to reload.
    @ObservationIgnored var onNotesChanged: (([String]) -> Void)?
    /// Saves open notes before a sync commits them.
    @ObservationIgnored var beforeSync: (() -> Void)?

    @ObservationIgnored private var engine: VaultSync?
    @ObservationIgnored private let queue = DispatchQueue(label: "com.borisnezlobin.editor.sync", qos: .utility)
    @ObservationIgnored private var dueTimer: Timer?
    @ObservationIgnored private var progressTimer: Timer?
    @ObservationIgnored private var attachedFolder: URL?

    private static var deviceName: String {
        UIDevice.current.model.lowercased().replacingOccurrences(of: " ", with: "-")
    }

    var isSynced: Bool {
        guard let overview else { return false }
        return overview.phase != .hidden
    }

    // MARK: Attaching

    /// Syncs `folder` from now on, or nothing when it's nil.
    func attach(to folder: URL?) {
        guard folder != attachedFolder else { return }
        attachedFolder = folder
        dueTimer?.invalidate()
        engine = nil
        overview = nil
        conflicts = []
        guard let folder else { return }
        queue.async { [weak self] in
            let engine = VaultSync.open(folder: folder.path, device: Self.deviceName)
            DispatchQueue.main.async {
                guard let self, self.attachedFolder == folder else { return }
                self.engine = engine
                self.publish(engine.overview(), conflicts: engine.conflicts())
                self.syncForAppEvent()
            }
        }
    }

    // MARK: What starts a sync

    /// The app opened or came back to the front.
    func syncForAppEvent() {
        guard overview?.autoSync ?? false else { return }
        syncNow()
    }

    /// `sync.now`, the details sheet's button, and the app's events.
    func syncNow() {
        run { $0.syncNow() }
    }

    /// A note was saved; a sync follows once edits stop, as on the desktop.
    func noteEdited() {
        guard let engine else { return }
        queue.async { [weak self] in
            engine.edited()
            DispatchQueue.main.async { self?.scheduleNextSync() }
        }
    }

    /// The app went to the background: saves, then syncs while iOS allows,
    /// and asks for a background refresh later.
    func syncForBackground() {
        scheduleBackgroundRefresh()
        guard overview?.autoSync ?? false else { return }
        let time = BackgroundTime(name: "Sync")
        run({ $0.syncNow() }, then: time.end)
    }

    /// A background refresh from `BGAppRefreshTask`.
    @MainActor func syncInBackgroundRefresh() async {
        scheduleBackgroundRefresh()
        guard overview?.autoSync ?? false else { return }
        await withCheckedContinuation { continuation in
            run({ $0.syncNow() }, then: { continuation.resume() })
        }
    }

    private func scheduleBackgroundRefresh() {
        let minutes = max(Double(overview?.intervalMinutes ?? 15), 15)
        let request = BGAppRefreshTaskRequest(identifier: Self.refreshTaskIdentifier)
        request.earliestBeginDate = Date(timeIntervalSinceNow: minutes * 60)
        try? BGTaskScheduler.shared.submit(request)
    }

    // MARK: Conflicts

    /// Settles one note, then syncs it. Throws the reason when the core
    /// refuses, such as the note having changed since it was shown.
    @MainActor func resolve(_ note: ConflictNote, choices: [PlaceChoice]) async throws {
        guard let engine else { return }
        beforeSync?()
        let outcome: SyncOutcome = try await withCheckedThrowingContinuation { continuation in
            queue.async {
                NoteSaver.waitForWrites()
                let result = Result {
                    try engine.resolve(path: note.path, version: note.version, choices: choices)
                }
                continuation.resume(with: result)
            }
        }
        onNotesChanged?(outcome.received + [note.path])
        publish(engine.overview(), conflicts: await perform { $0.conflicts() } ?? [])
        scheduleNextSync()
    }

    // MARK: Account and settings

    func signIn(token: String) throws {
        try engine?.signIn(token: token)
        refresh()
        syncNow()
    }

    func signOut() throws {
        try engine?.signOut()
        refresh()
    }

    func setRepository(_ typed: String) throws {
        try engine?.setRepository(typed: typed)
        refresh()
    }

    /// Follows the sync settings after one changed.
    func reloadSettings() {
        engine?.reloadSettings()
        refresh()
        scheduleNextSync()
    }

    func refresh() {
        guard let engine else { return }
        overview = engine.overview()
    }

    // MARK: Running

    private func run(_ work: @escaping (VaultSync) -> SyncOutcome, then done: (() -> Void)? = nil) {
        guard let engine else {
            done?()
            return
        }
        beforeSync?()
        startProgress()
        queue.async { [weak self] in
            NoteSaver.waitForWrites()
            let outcome = work(engine)
            let overview = engine.overview()
            let conflicts = engine.conflicts()
            DispatchQueue.main.async {
                self?.finish(outcome, overview: overview, conflicts: conflicts)
                done?()
            }
        }
    }

    private func finish(_ outcome: SyncOutcome, overview: SyncOverview, conflicts: [ConflictNote]) {
        stopProgress()
        publish(overview, conflicts: conflicts)
        let waiting = conflicts.map(\.path)
        if !outcome.received.isEmpty || !waiting.isEmpty {
            onNotesChanged?(outcome.received + waiting)
        }
        scheduleNextSync()
    }

    private func publish(_ overview: SyncOverview, conflicts: [ConflictNote]) {
        self.overview = overview
        self.conflicts = conflicts
    }

    private func perform<Value>(_ work: @escaping (VaultSync) -> Value) async -> Value? {
        guard let engine else { return nil }
        return await withCheckedContinuation { continuation in
            queue.async { continuation.resume(returning: work(engine)) }
        }
    }

    /// Reads the overview while a sync runs, so the indicator names the step.
    private func startProgress() {
        isRunning = true
        progressTimer?.invalidate()
        progressTimer = Timer.scheduledTimer(withTimeInterval: 0.3, repeats: true) { [weak self] _ in
            self?.refresh()
        }
        refresh()
    }

    private func stopProgress() {
        isRunning = false
        progressTimer?.invalidate()
        progressTimer = nil
    }

    /// Wakes when the scheduler next has something due: edits settling,
    /// the interval passing or a retry.
    private func scheduleNextSync() {
        dueTimer?.invalidate()
        guard let engine else { return }
        queue.async { [weak self] in
            guard let seconds = engine.secondsUntilDue() else { return }
            DispatchQueue.main.async {
                guard let self, self.engine === engine else { return }
                self.dueTimer?.invalidate()
                let delay = max(seconds, 0.5)
                self.dueTimer = Timer.scheduledTimer(withTimeInterval: delay, repeats: false) { [weak self] _ in
                    self?.run { $0.syncIfDue() }
                }
            }
        }
    }
}

/// Time iOS gives the app to finish a sync after it goes to the
/// background, ended once the sync is done or the time runs out.
private final class BackgroundTime {
    private var identifier = UIBackgroundTaskIdentifier.invalid

    init(name: String) {
        identifier = UIApplication.shared.beginBackgroundTask(withName: name) { [weak self] in self?.end() }
    }

    func end() {
        guard identifier != .invalid else { return }
        UIApplication.shared.endBackgroundTask(identifier)
        identifier = .invalid
    }
}
