import Foundation

/// How the app reaches GitHub for signing in: the transport the core's
/// requests go through, and this build's OAuth App.
enum GitHubConnection {
    /// This build's OAuth App, or nil when it has none and GitHub sign-in
    /// isn't available.
    static var clientId: String? {
        #if DEBUG
        if FakeGitHub.isOn { return FakeGitHub.clientId }
        #endif
        return githubClientId()
    }

    static func transport() -> HttpTransport {
        #if DEBUG
        if FakeGitHub.isOn { return FakeGitHub() }
        #endif
        return URLSessionTransport()
    }
}

/// Sends the core's requests with URLSession. The core calls it from a
/// background queue and waits for the answer.
final class URLSessionTransport: HttpTransport {
    private static let timeout: TimeInterval = 30

    func send(call: HttpCall) throws -> HttpReply {
        guard let url = URL(string: call.url) else {
            throw TransportError.Unreachable(message: "\(call.url) isn't an address.")
        }
        let answer = Self.answer(to: Self.request(for: call, at: url))
        switch answer {
        case .success(let reply): return reply
        case .failure(let error): throw TransportError.Unreachable(message: error.localizedDescription)
        }
    }

    private static func request(for call: HttpCall, at url: URL) -> URLRequest {
        var request = URLRequest(url: url, timeoutInterval: timeout)
        request.httpMethod = call.method
        for header in call.headers {
            request.setValue(header.value, forHTTPHeaderField: header.name)
        }
        request.httpBody = call.body?.data(using: .utf8)
        return request
    }

    private static func answer(to request: URLRequest) -> Result<HttpReply, Error> {
        let done = DispatchSemaphore(value: 0)
        var answer: Result<HttpReply, Error> = .failure(URLError(.timedOut))
        URLSession.shared.dataTask(with: request) { data, response, error in
            defer { done.signal() }
            if let error {
                answer = .failure(error)
                return
            }
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            let body = data.flatMap { String(data: $0, encoding: .utf8) } ?? ""
            answer = .success(HttpReply(status: UInt16(clamping: status), body: body))
        }.resume()
        done.wait()
        return answer
    }
}

#if DEBUG
/// `-fakeGitHub YES` answers as GitHub would, so the sign-in screens can
/// be seen without an OAuth App: the code WDJB-MJHT, approved on the
/// third poll, the account "you" with a few repositories, and making a
/// repository. `-fakeGitHubRemote <url>` is the address its repositories
/// clone from, such as a bare repository reached as `file://`.
final class FakeGitHub: HttpTransport, @unchecked Sendable {
    static let clientId = "fake-client"
    private static let pendingPolls = 2
    private let lock = NSLock()
    private var polls = 0

    static var isOn: Bool { UserDefaults.standard.bool(forKey: "fakeGitHub") }

    /// `-fakeGitHubMakeRepository YES` takes the new repository as soon as
    /// the repositories show, to see setting up through to its end.
    static var makesRepositoryAtOnce: Bool {
        isOn && UserDefaults.standard.bool(forKey: "fakeGitHubMakeRepository")
    }

    private static var remote: String {
        UserDefaults.standard.string(forKey: "fakeGitHubRemote") ?? "https://github.com/you/notes.git"
    }

    func send(call: HttpCall) throws -> HttpReply {
        Thread.sleep(forTimeInterval: 0.4)
        let body = answer(to: call)
        return HttpReply(status: call.method == "POST" && call.url.hasSuffix("/user/repos") ? 201 : 200, body: body)
    }

    private func answer(to call: HttpCall) -> String {
        let url = call.url
        if url.hasSuffix("/login/device/code") { return Self.deviceCode }
        if url.hasSuffix("/login/oauth/access_token") { return tokenAnswer() }
        if url.hasSuffix("/user") { return #"{"login":"you"}"# }
        if url.hasSuffix("/user/repos") { return Self.repository(named: "notes", private: true) }
        return "[" + Self.existing.map { Self.repository(named: $0.name, private: $0.private) }.joined(separator: ",") + "]"
    }

    private func tokenAnswer() -> String {
        lock.lock()
        defer { lock.unlock() }
        polls += 1
        return polls > Self.pendingPolls ? #"{"access_token":"gho_fake","token_type":"bearer"}"# :
            #"{"error":"authorization_pending"}"#
    }

    private static let deviceCode = #"""
    {"device_code":"fake-device","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}
    """#

    private static let existing: [(name: String, private: Bool)] = [
        ("journal", true), ("website", false), ("recipes", true), ("dotfiles", false)
    ]

    private static func repository(named name: String, private isPrivate: Bool) -> String {
        let cloneUrl = name == "notes" ? remote : "https://github.com/you/\(name).git"
        return #"{"full_name":"you/\#(name)","name":"\#(name)","clone_url":"\#(cloneUrl)","private":\#(isPrivate),"default_branch":"main"}"#
    }
}
#endif
