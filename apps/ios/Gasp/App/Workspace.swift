import Foundation
import Observation

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

    var id: String {
        switch self {
        case .syncSetup: "sync setup"
        case .syncDetails: "sync details"
        case .resolver: "resolver"
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

    var id: String {
        switch self {
        case .rename(let path): "rename \(path)"
        case .delete(let path): "delete \(path)"
        case .export(let path): "export \(path)"
        case .pickVault: "pick vault"
        }
    }
}

/// The browser's state beyond its tabs: the sidebar, the tab overview,
/// sheets, prompts and short notices, and this device's text size and
/// line length.
@Observable
final class Workspace {
    var sidebarOpen = false
    var sidebarSection: SidebarSection = .files
    var searchQuery = ""
    /// Bumped to put the cursor in the sidebar's search field.
    var searchFocusRequest = 0
    var overviewOpen = false
    var sheet: WorkspaceSheet?
    var prompt: WorkspacePrompt?
    private(set) var notice: String?
    var keyboardShown = false

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

    func openSearch() {
        sidebarOpen = true
        searchFocusRequest += 1
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
