/// Where Insert image takes a picture from. Its button on the keyboard
/// bar opens these as a menu; from the palette a dialog asks. Each runs
/// as its own id through the command runner, like any button would.
enum ImageSource: String, CaseIterable, Identifiable {
    case photoLibrary = "note.import-image.photos"
    case files = "note.import-image.files"

    static let command = "note.import-image"

    var id: String { rawValue }

    var title: String {
        switch self {
        case .photoLibrary: "Photo library"
        case .files: "Files"
        }
    }

    var symbol: String {
        switch self {
        case .photoLibrary: "photo.on.rectangle"
        case .files: "folder"
        }
    }

    var sheet: WorkspaceSheet {
        switch self {
        case .photoLibrary: .photos
        case .files: .imageFiles
        }
    }
}
