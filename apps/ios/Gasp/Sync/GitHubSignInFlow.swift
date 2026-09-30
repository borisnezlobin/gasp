import Foundation
import Observation

/// Signing in with GitHub and choosing a repository, one step at a time:
/// the short code to approve, the person's repositories, setting up, and
/// what setting up did.
@MainActor
@Observable
final class GitHubSignInFlow {
    enum Stage {
        case asking
        case code(userCode: String, page: URL?)
        case failed(String)
        case repositories(GitHubAccount, token: String)
        case working(String)
        case done(GitHubVaultSetUp.Finished)
    }

    private(set) var stage: Stage = .asking

    /// The folder set up to sync, once it is.
    var finishedFolder: URL? {
        if case .done(let finished) = stage { return finished.folder }
        return nil
    }
    @ObservationIgnored private let transport = GitHubConnection.transport()

    /// Asks GitHub for a code, then asks again at the intervals GitHub
    /// gives until the person approves it. Cancelling the task (closing
    /// the sheet) stops the polling.
    func signIn() async {
        stage = .asking
        guard let clientId = GitHubConnection.clientId else {
            stage = .failed(Self.unavailable)
            return
        }
        let transport = transport
        do {
            let signIn = try await offMainThread { try GitHubSignIn.start(transport: transport, clientId: clientId) }
            stage = .code(userCode: signIn.userCode(), page: URL(string: signIn.verificationPage()))
            await poll(signIn)
        } catch {
            stage = .failed(error.shownMessage)
        }
    }

    static let unavailable = "GitHub sign-in isn't available in this build yet."

    private func poll(_ signIn: GitHubSignIn) async {
        let started = Date.now
        var wait = signIn.firstWaitSeconds()
        while !Task.isCancelled {
            try? await Task.sleep(for: .seconds(wait))
            guard !Task.isCancelled else { return }
            let waited = Date.now.timeIntervalSince(started)
            let step = (try? await offMainThread { signIn.poll(waitedSeconds: waited) }) ?? .waiting(nextPollSeconds: wait)
            switch step {
            case .waiting(let next): wait = next
            case .signedIn(let token):
                await readAccount(token: token)
                return
            case .failed(let message):
                stage = .failed(message)
                return
            }
        }
    }

    private func readAccount(token: String) async {
        stage = .working("Reading your repositories")
        let transport = transport
        do {
            let account = try await offMainThread { try githubAccount(transport: transport, token: token) }
            stage = .repositories(account, token: token)
        } catch {
            stage = .failed(error.shownMessage)
        }
    }

    /// Sets the vault up with a repository the person already has.
    func use(_ repository: GitHubRepository, token: String) async {
        stage = .working("Setting up \(repository.fullName)")
        await finish { try GitHubVaultSetUp.use(repository, token: token) }
    }

    /// Makes a private repository and sets the vault up with it, taking
    /// the phone's own vault along when that's what's open.
    func makeRepository(account: GitHubAccount, token: String, ownVault: URL?) async {
        stage = .working("Making your repository")
        let transport = transport
        await finish {
            NoteSaver.waitForWrites()
            return try GitHubVaultSetUp.makeNew(
                token: token, known: account.repositories, transport: transport, ownVault: ownVault
            )
        }
    }

    private func finish(_ setUp: @escaping () throws -> GitHubVaultSetUp.Finished) async {
        do {
            stage = .done(try await offMainThread(setUp))
        } catch {
            stage = .failed(error.shownMessage)
        }
    }
}

extension GitHubVaultSetUp.Finished {
    /// What setting up did, as the desktop says it.
    var sentence: String {
        guard summary.broughtIn > 0 else {
            return "Your notes are in \(repository) now, and they sync on their own from here."
        }
        return "\(summary.broughtIn) \(summary.broughtIn == 1 ? "file" : "files") came in from \(repository) "
            + "and \(summary.sent) went out."
    }
}
