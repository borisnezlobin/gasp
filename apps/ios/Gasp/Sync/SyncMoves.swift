import Foundation

/// Runs `work` on a background queue and hands back what it returns or
/// throws, for the core's slow calls: the network, git and copying files.
func offMainThread<Value>(_ work: @escaping () throws -> Value) async throws -> Value {
    try await withCheckedThrowingContinuation { continuation in
        DispatchQueue.global(qos: .userInitiated).async {
            continuation.resume(with: Result { try work() })
        }
    }
}

/// Setting a vault up to sync with a GitHub repository chosen after
/// signing in. Every step runs off the main thread.
enum GitHubVaultSetUp {
    struct Finished {
        let folder: URL
        let repository: String
        let summary: InPlaceSetupSummary
    }

    /// A repository the person already has, into a new empty folder.
    static func use(_ repository: GitHubRepository, token: String) throws -> Finished {
        let folder = VaultLocation.newSyncedFolder(for: repository.name)
        do {
            return try setUp(repository, token: token, in: folder)
        } catch {
            try? FileManager.default.removeItem(at: folder)
            throw error
        }
    }

    /// A new private repository. The phone's own vault, when it's open,
    /// moves into the synced folder and goes up with it; it moves back
    /// if setting up fails.
    static func makeNew(
        token: String, known: [GitHubRepository], transport: HttpTransport, ownVault: URL?
    ) throws -> Finished {
        let repository = try githubMakeNotesRepository(transport: transport, token: token, known: known)
        guard let ownVault else { return try use(repository, token: token) }
        let folder = VaultLocation.newSyncedFolder(for: repository.name)
        try FileManager.default.createDirectory(
            at: folder.deletingLastPathComponent(), withIntermediateDirectories: true
        )
        try FileManager.default.moveItem(at: ownVault, to: folder)
        do {
            return try setUp(repository, token: token, in: folder)
        } catch {
            try? FileManager.default.moveItem(at: folder, to: ownVault)
            throw error
        }
    }

    private static func setUp(_ repository: GitHubRepository, token: String, in folder: URL) throws -> Finished {
        let setup = InPlaceSyncSetup(
            url: repository.cloneUrl, branch: repository.branch, token: token,
            folder: folder.path, device: SyncCenter.deviceName
        )
        let summary = try setUpSyncInPlace(setup: setup)
        return Finished(folder: folder, repository: repository.fullName, summary: summary)
    }
}

/// Moving this iPhone's notes into the Gasp folder in iCloud Drive: a
/// copy, checked and coordinated with iCloud, that leaves the original
/// where it is.
enum ICloudVaultMove {
    static func bring(_ vault: URL, into icloudFolder: URL) throws -> ICloudMove {
        var refusal: NSError?
        var result: Result<ICloudMove, Error> = .failure(CocoaError(.fileWriteUnknown))
        NSFileCoordinator(filePresenter: nil).coordinate(
            writingItemAt: icloudFolder, options: [], error: &refusal
        ) { folder in
            result = Result {
                try icloudMoveVault(from: vault.path, to: folder.path, device: SyncCenter.deviceName)
            }
        }
        if let refusal { throw refusal }
        return try result.get()
    }
}
