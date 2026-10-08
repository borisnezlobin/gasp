import Foundation

/// Gasp's own iCloud Documents container, which iCloud Drive shows as a
/// folder named Gasp on the iPhone, the Mac and the web. Gasp makes the
/// folder itself, so nobody has to find iCloud Drive in Files first.
enum ICloudContainer {
    static let identifier = "iCloud.com.borisnezlobin.gasp"
    /// The container's folder in `Mobile Documents`, the same on every device.
    static let folderName = "iCloud~com~borisnezlobin~gasp"

    static let turnedOff = "Turn on iCloud Drive in Settings › your name › iCloud, then try again."

    /// The container's Documents folder, made if it's missing, or nil when
    /// iCloud Drive is off. Apple asks for the container to be looked up
    /// off the main thread, since the first lookup can take a while.
    static func documentsFolder() async throws -> URL? {
        try await offMainThread(makeDocumentsFolder)
    }

    private static func makeDocumentsFolder() throws -> URL? {
        let files = FileManager.default
        guard let container = files.url(forUbiquityContainerIdentifier: identifier) else { return nil }
        let documents = container.appending(path: "Documents", directoryHint: .isDirectory)
        try files.createDirectory(at: documents, withIntermediateDirectories: true)
        return documents
    }

    /// Whether `folder` is the container's Documents folder, which iCloud
    /// Drive shows as Gasp.
    static func isDocumentsFolder(_ folder: URL) -> Bool {
        folder.lastPathComponent == "Documents"
            && folder.deletingLastPathComponent().lastPathComponent == folderName
    }

    /// The name iCloud Drive shows for `folder`.
    static func shownName(of folder: URL) -> String {
        isDocumentsFolder(folder) ? icloudFolderName() : folder.lastPathComponent
    }
}
