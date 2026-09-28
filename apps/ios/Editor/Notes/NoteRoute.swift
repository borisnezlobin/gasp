import Foundation

/// A note to show, and optionally the line to put the cursor on.
struct NoteRoute: Hashable {
    let note: NoteSummary
    var line: Int?

    /// Reads `editor://open?path=Folder/Note.md&line=12`, where `line`
    /// counts from 1 and may be left out.
    init?(url: URL, notes: [NoteSummary]) {
        guard url.scheme == "editor", url.host() == "open",
              let query = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems,
              let path = query.first(where: { $0.name == "path" })?.value,
              let note = notes.first(where: { $0.path == path })
        else { return nil }
        self.note = note
        line = query.first(where: { $0.name == "line" })?.value.flatMap(Int.init).map { max($0 - 1, 0) }
    }

    init(note: NoteSummary) {
        self.note = note
    }
}
