import SwiftUI

/// One screen of Settings, after the Mac app's settings pages. The raw
/// value is the page's id on the Mac, so `-settingsScrollTo sync` opens
/// the same page there and here.
enum SettingsGroup: String, CaseIterable, Identifiable, Hashable {
    case general
    case sync
    case appearance
    case sidebar
    case toolbars
    case editor
    case files
    case dailyNotes = "daily-notes"
    case prose
    case snippets

    var id: String { rawValue }

    var title: String { Self.looks[self]?.title ?? rawValue }
    var symbol: String { Self.looks[self]?.symbol ?? "gearshape" }

    /// Groups that show rows of their own, so they stay even with no
    /// schema setting in them.
    var hasOwnRows: Bool {
        [.general, .sync, .appearance, .toolbars].contains(self)
    }

    /// The group a schema setting belongs in, by its section.
    static func of(_ item: SettingItem) -> SettingsGroup {
        keyGroups[item.key] ?? sectionGroups[item.section] ?? .general
    }

    private static let looks: [SettingsGroup: (title: String, symbol: String)] = [
        .general: ("General", "slider.horizontal.3"),
        .sync: ("Sync", "arrow.triangle.2.circlepath"),
        .appearance: ("Appearance", "paintpalette"),
        .sidebar: ("Sidebar", "sidebar.left"),
        .toolbars: ("Toolbars", "menubar.rectangle"),
        .editor: ("Editor", "pencil"),
        .files: ("Files and links", "folder"),
        .dailyNotes: ("Daily notes and templates", "calendar"),
        .prose: ("Prose", "text.alignleft"),
        .snippets: ("Snippets and replacements", "function")
    ]

    private static let sectionGroups: [String: SettingsGroup] = [
        "Sync": .sync,
        "Appearance": .appearance,
        "Sidebar": .sidebar,
        "Editor": .editor,
        "Markdown": .editor,
        "Files": .files,
        "Recovery": .files,
        "Daily notes": .dailyNotes,
        "Templates": .dailyNotes,
        "Prose": .prose,
        "Math": .snippets
    ]

    private static let keyGroups: [String: SettingsGroup] = [
        "editor.snippets": .snippets,
        "editor.replacements": .snippets
    ]
}

/// The titled blocks of rows on the top level of Settings.
enum SettingsShelf: String, CaseIterable, Identifiable {
    case app = "App"
    case writing = "Writing"

    var id: String { rawValue }

    var groups: [SettingsGroup] {
        switch self {
        case .app: [.general, .sync, .appearance, .sidebar, .toolbars]
        case .writing: [.editor, .files, .dailyNotes, .prose, .snippets]
        }
    }
}

/// Schema settings under one header inside a group's screen.
struct SettingsBlock: Identifiable {
    let title: String
    let items: [SettingItem]

    var id: String { title }

    /// How a schema section reads as a header, where its name alone
    /// wouldn't say what it is.
    static func header(for section: String) -> String {
        headers[section] ?? section
    }

    private static let headers = ["Mcp": "Agent apps"]
}
