import Foundation
import Observation

/// The vault in iCloud Drive: iCloud does the syncing, and this watches
/// it. It tells the browser which notes another device changed, starts
/// downloading the files iCloud hasn't brought down yet, and finds the
/// copies iCloud leaves when two devices changed a note at once, so one
/// indicator and one sheet can show where things stand.
@Observable
final class ICloudCenter {
    /// Nil when the open vault isn't in iCloud.
    private(set) var folder: URL?
    /// Vault-relative paths of files still coming down from iCloud.
    private(set) var downloading: [String] = []
    /// Notes changed on two devices at once, each beside its copy.
    private(set) var copies: [ICloudCopyPair] = []

    /// Notes another device changed, for open notes to reload.
    @ObservationIgnored var onNotesChanged: (([String]) -> Void)?

    @ObservationIgnored private var presenter: VaultFolderPresenter?
    @ObservationIgnored private let queue = DispatchQueue(label: "com.borisnezlobin.gasp.icloud", qos: .utility)

    var isAttached: Bool { folder != nil }

    /// Watches `folder` from now on, or nothing when it's nil.
    func attach(to folder: URL?) {
        guard folder != self.folder else { return }
        presenter?.stop()
        presenter = nil
        VaultFileCoordination.active = nil
        downloading = []
        copies = []
        self.folder = folder
        guard let folder else { return }
        let presenter = VaultFolderPresenter(folder: folder) { [weak self] paths in self?.filesChanged(paths) }
        self.presenter = presenter
        VaultFileCoordination.active = VaultFileCoordination(root: folder, presenter: presenter)
        rescan()
    }

    /// Looks the vault over again: on attaching, on coming to the front,
    /// and after iCloud changed something.
    func rescan() {
        guard let folder else { return }
        let coordination = VaultFileCoordination.active
        queue.async { [weak self] in
            let scan = ICloudScan.run(root: folder, coordination: coordination)
            DispatchQueue.main.async {
                guard let self, self.folder == folder else { return }
                self.publish(scan)
            }
        }
    }

    private func filesChanged(_ paths: [String]) {
        onNotesChanged?(paths)
        rescan()
    }

    private func publish(_ scan: ICloudScan) {
        let known = Set(copies.map(\.copy))
        if scan.copies.contains(where: { !known.contains($0.copy) }) { Haptics.warning() }
        downloading = scan.downloading
        copies = scan.copies
        if !scan.madeCopies.isEmpty { onNotesChanged?(scan.madeCopies) }
    }
}

/// One look over the iCloud vault, made off the main thread.
struct ICloudScan {
    let downloading: [String]
    let copies: [ICloudCopyPair]
    /// Copies made just now from iCloud's own conflict versions.
    let madeCopies: [String]

    static func run(root: URL, coordination: VaultFileCoordination?) -> ICloudScan {
        let made = ICloudConflictVersions.keepAsCopies(in: root, coordination: coordination)
        let waiting = icloudWaitingDownloads(root: root.path)
        for path in waiting {
            try? FileManager.default.startDownloadingUbiquitousItem(at: root.appending(path: path))
        }
        return ICloudScan(downloading: waiting, copies: icloudCopies(root: root.path), madeCopies: made)
    }
}

/// iCloud's other way of keeping both sides of a clash: versions of a
/// note it marks as unresolved conflicts. Each becomes a copy beside the
/// note, named as iCloud names its own copies ("Plan 2.md"), so the same
/// sheet handles both.
enum ICloudConflictVersions {
    static func keepAsCopies(in root: URL, coordination: VaultFileCoordination?) -> [String] {
        conflictedNotes(in: root).flatMap { note in
            keepAsCopies(of: note, root: root, coordination: coordination)
        }
    }

    private static func conflictedNotes(in root: URL) -> [URL] {
        let key = URLResourceKey.ubiquitousItemHasUnresolvedConflictsKey
        guard let files = FileManager.default.enumerator(
            at: root, includingPropertiesForKeys: [key], options: [.skipsHiddenFiles]
        ) else { return [] }
        return files.compactMap { $0 as? URL }.filter { url in
            url.pathExtension == "md" && (try? url.resourceValues(forKeys: [key]))?.ubiquitousItemHasUnresolvedConflicts == true
        }
    }

    private static func keepAsCopies(of note: URL, root: URL, coordination: VaultFileCoordination?) -> [String] {
        let versions = NSFileVersion.unresolvedConflictVersionsOfItem(at: note) ?? []
        let made = versions.compactMap { version -> String? in
            let copy = unusedCopy(of: note)
            let path = VaultFolderPresenter.relativePath(of: copy, in: root)
            let copied = try? VaultFileCoordination.write(path, with: coordination) {
                try FileManager.default.copyItem(at: version.url, to: copy)
            }
            guard copied != nil else { return nil }
            version.isResolved = true
            return path
        }
        try? NSFileVersion.removeOtherVersionsOfItem(at: note)
        return made
    }

    /// `Plan 2.md` beside `Plan.md`, or the next number that's free.
    static func unusedCopy(of note: URL) -> URL {
        let folder = note.deletingLastPathComponent()
        let name = note.deletingPathExtension().lastPathComponent
        var number = 2
        var copy = folder.appending(path: "\(name) \(number).md")
        while FileManager.default.fileExists(atPath: copy.path) {
            number += 1
            copy = folder.appending(path: "\(name) \(number).md")
        }
        return copy
    }
}

/// Writes and reads of notes in the iCloud vault go through a file
/// coordinator, so iCloud never uploads half a note or swaps one out
/// while it's being read. The vault's own presenter is named as the
/// writer, so a save doesn't come back as a change from elsewhere.
final class VaultFileCoordination {
    /// The iCloud vault's, while one is open. Read on the main thread.
    static var active: VaultFileCoordination?

    let root: URL
    let presenter: NSFilePresenter

    init(root: URL, presenter: NSFilePresenter) {
        self.root = root
        self.presenter = presenter
    }

    /// Runs `body` as a coordinated write of `path` when `coordination`
    /// is there, and plainly otherwise.
    static func write<Value>(
        _ path: String, with coordination: VaultFileCoordination?, _ body: () throws -> Value
    ) throws -> Value {
        guard let coordination else { return try body() }
        return try coordination.coordinate(path, writing: true, body)
    }

    /// Runs `body` as a coordinated read of `path` in the open iCloud
    /// vault, which first brings the file down if iCloud hasn't yet.
    static func read<Value>(_ path: String, _ body: () throws -> Value) throws -> Value {
        guard let coordination = active else { return try body() }
        return try coordination.coordinate(path, writing: false, body)
    }

    private func coordinate<Value>(_ path: String, writing: Bool, _ body: () throws -> Value) throws -> Value {
        let url = root.appending(path: path)
        let coordinator = NSFileCoordinator(filePresenter: presenter)
        var refusal: NSError?
        var result: Result<Value, Error>?
        let run: (URL) -> Void = { _ in result = Result { try body() } }
        if writing {
            coordinator.coordinate(writingItemAt: url, options: .forReplacing, error: &refusal, byAccessor: run)
        } else {
            coordinator.coordinate(readingItemAt: url, options: [], error: &refusal, byAccessor: run)
        }
        if let refusal { throw refusal }
        guard let result else { throw CocoaError(.fileWriteUnknown) }
        return try result.get()
    }
}

/// Hears from iCloud when files in the vault change, appear, move or go,
/// and hands on their vault-relative paths once they settle for a moment.
final class VaultFolderPresenter: NSObject, NSFilePresenter {
    let presentedItemURL: URL?
    let presentedItemOperationQueue: OperationQueue = {
        let queue = OperationQueue()
        queue.maxConcurrentOperationCount = 1
        return queue
    }()

    private let root: URL
    private let changed: ([String]) -> Void
    private var pending = Set<String>()
    private var settle: DispatchWorkItem?
    private static let settleTime: TimeInterval = 0.5

    init(folder: URL, changed: @escaping ([String]) -> Void) {
        root = folder
        presentedItemURL = folder
        self.changed = changed
        super.init()
        NSFileCoordinator.addFilePresenter(self)
    }

    func stop() {
        NSFileCoordinator.removeFilePresenter(self)
    }

    func presentedSubitemDidChange(at url: URL) { note(url) }
    func presentedSubitemDidAppear(at url: URL) { note(url) }
    func presentedSubitem(at url: URL, didGain version: NSFileVersion) { note(url) }
    func presentedSubitem(at url: URL, didResolve version: NSFileVersion) { note(url) }

    func presentedSubitem(at oldURL: URL, didMoveTo newURL: URL) {
        note(oldURL)
        note(newURL)
    }

    func accommodatePresentedSubitemDeletion(at url: URL, completionHandler: @escaping (Error?) -> Void) {
        note(url)
        completionHandler(nil)
    }

    /// `Folder/Plan.md` for a file in the vault; a placeholder iCloud
    /// keeps for a file not downloaded yet (`.Plan.md.icloud`) counts as
    /// the file it stands for.
    static func relativePath(of url: URL, in root: URL) -> String {
        let base = root.resolvingSymlinksInPath().standardizedFileURL.path
        let full = url.resolvingSymlinksInPath().standardizedFileURL.path
        let relative = full.hasPrefix(base + "/") ? String(full.dropFirst(base.count + 1)) : url.lastPathComponent
        return standInFor(relative)
    }

    private static func standInFor(_ path: String) -> String {
        var parts = path.split(separator: "/", omittingEmptySubsequences: false).map(String.init)
        guard let last = parts.last, last.hasPrefix("."), last.hasSuffix(".icloud") else { return path }
        parts[parts.count - 1] = String(last.dropFirst().dropLast(".icloud".count))
        return parts.joined(separator: "/")
    }

    private func note(_ url: URL) {
        let path = Self.relativePath(of: url, in: root)
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            pending.insert(path)
            settle?.cancel()
            let settle = DispatchWorkItem { [weak self] in self?.handOn() }
            self.settle = settle
            DispatchQueue.main.asyncAfter(deadline: .now() + Self.settleTime, execute: settle)
        }
    }

    private func handOn() {
        let paths = Array(pending)
        pending.removeAll()
        guard !paths.isEmpty else { return }
        changed(paths)
    }
}
