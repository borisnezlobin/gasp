import Foundation

extension Error {
    /// What went wrong, as a sentence to show. The core's errors describe
    /// themselves as Swift source by default, which no one should read.
    var shownMessage: String {
        guard let error = self as? VaultError else { return localizedDescription }
        let message = switch error {
        case .NoFolder(let path): "There's no folder at \(path)."
        case .NotANote(let path): "\(path) isn't a note in this vault."
        case .Refused(let message), .Io(let message): message
        }
        return message.first.map { $0.uppercased() + message.dropFirst() } ?? message
    }
}
