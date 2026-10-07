import GameController
import Observation
import UIKit

/// What the sidebar shows below its search field.
enum SidebarSection: String, CaseIterable, Identifiable {
    case files = "Files"
    case outline = "Outline"
    case links = "Links"
    case tags = "Tags"

    var id: String { rawValue }

    var symbol: String {
        switch self {
        case .files: "folder"
        case .outline: "list.bullet.indent"
        case .links: "link"
        case .tags: "number"
        }
    }
}

/// A sheet over the browser.
enum WorkspaceSheet: Identifiable {
    case palette
    case settings
    case toolbars
    case templates
    case recovery(path: String)
    case share(URL)
    case lookUp(String)
    case photos
    case syncSetup(SyncSetupDraft)
    case syncDetails
    case resolver
    /// iCloud or GitHub, for a vault that doesn't sync yet.
    case syncChooser
    case icloudSetup
    case githubSignIn
    case icloudDetails

    var id: String {
        switch self {
        case .syncSetup: "sync setup"
        case .syncDetails: "sync details"
        case .resolver: "resolver"
        case .syncChooser: "sync chooser"
        case .icloudSetup: "icloud setup"
        case .githubSignIn: "github sign-in"
        case .icloudDetails: "icloud details"
        case .palette: "palette"
        case .settings: "settings"
        case .toolbars: "toolbars"
        case .templates: "templates"
        case .recovery(let path): "recovery \(path)"
        case .share(let url): "share \(url.path)"
        case .lookUp(let term): "look up \(term)"
        case .photos: "photos"
        }
    }
}

/// A question the browser asks before acting.
enum WorkspacePrompt: Identifiable {
    case rename(path: String)
    case delete(path: String)
    case export(path: String)
    case pickVault
    /// A name for a new folder in `parent`, `""` for the top of the vault.
    case newFolder(parent: String)

    var id: String {
        switch self {
        case .rename(let path): "rename \(path)"
        case .delete(let path): "delete \(path)"
        case .export(let path): "export \(path)"
        case .pickVault: "pick vault"
        case .newFolder(let parent): "new folder in \(parent)"
        }
    }
}

/// The browser's state beyond its tabs: the sidebar, the tab overview,
/// sheets, prompts and short notices, and this device's text size and
/// line length.
@Observable
final class Workspace {
    /// Opening the sidebar puts the keyboard away; closing it leaves the
    /// keyboard down until the note is tapped.
    var sidebarOpen = false {
        didSet { if sidebarOpen && !oldValue { Self.dismissKeyboard() } }
    }
    var sidebarSection: SidebarSection = .files
    var searchQuery = ""
    /// Set to put the cursor in the sidebar's search field, which takes it
    /// and clears this.
    var searchFocusPending = false
    var overviewOpen = false
    var sheet: WorkspaceSheet?
    var prompt: WorkspacePrompt?
    private(set) var notice: String?
    var keyboardShown = false
    /// The note or folder being dragged in the file tree.
    var treeDrag: TreeEntry?

    var zoom: Double {
        didSet { UserDefaults.standard.set(zoom, forKey: Self.zoomKey) }
    }

    var readableWidth: Bool {
        didSet { UserDefaults.standard.set(readableWidth, forKey: Self.readableWidthKey) }
    }

    private static let zoomKey = "view.zoom"
    private static let readableWidthKey = "view.readable-width"
    static let zoomSteps: [Double] = [0.8, 0.9, 1, 1.1, 1.25, 1.4, 1.6]

    init() {
        let defaults = UserDefaults.standard
        zoom = defaults.object(forKey: Self.zoomKey) as? Double ?? 1
        readableWidth = defaults.object(forKey: Self.readableWidthKey) as? Bool ?? true
    }

    func openSidebar(_ section: SidebarSection) {
        sidebarSection = section
        sidebarOpen = true
    }

    /// Opens the file tree on the note at `path`, for dragging it onto a
    /// folder.
    func revealForMoving(_ path: String) {
        searchQuery = ""
        openSidebar(.files)
        tell("Hold the note, then drag it onto a folder.")
    }

    /// Opens the sidebar, with the cursor in its search field only when a
    /// hardware keyboard is attached, so no software keyboard covers the
    /// files.
    func openSearch() {
        sidebarOpen = true
        searchFocusPending = GCKeyboard.coalesced != nil
    }

    private static func dismissKeyboard() {
        UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
    }

    /// Shows `message` for a moment.
    func tell(_ message: String) {
        notice = message
        let shown = message
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.5) { [weak self] in
            if self?.notice == shown { self?.notice = nil }
        }
    }

    func zoomIn() {
        zoom = Self.zoomSteps.first { $0 > zoom + 0.001 } ?? zoom
    }

    func zoomOut() {
        zoom = Self.zoomSteps.last { $0 < zoom - 0.001 } ?? zoom
    }
}
