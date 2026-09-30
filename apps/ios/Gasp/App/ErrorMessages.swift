import Foundation

extension Error {
    /// What went wrong, as a sentence to show. The core's errors describe
    /// themselves as Swift source by default, which no one should read.
    var shownMessage: String {
        let message = coreSentence ?? localizedDescription
        return message.first.map { $0.uppercased() + message.dropFirst() } ?? message
    }

    private var coreSentence: String? {
        if let error = self as? VaultError { return error.sentence }
        if case .Refused(let message)? = self as? GitHubProblem { return message }
        return nil
    }
}

private extension VaultError {
    var sentence: String {
        switch self {
        case .NoFolder(let path): "There's no folder at \(path)."
        case .NotANote(let path): "\(path) isn't a note in this vault."
        case .Refused(let message), .Io(let message): message
        }
    }
}
