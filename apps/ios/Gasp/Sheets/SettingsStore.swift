import Foundation
import Observation

/// The vault's schema settings for the Settings screens, and writing one
/// back. A value the app wouldn't load is refused, and the reason is kept
/// in `problem` until the next write goes through.
@Observable final class SettingsStore {
    private(set) var items: [SettingItem] = []
    private(set) var problem: String?
    /// `-settingsScrollTo <key>` from the command line, until a screen
    /// has scrolled to it.
    private var launchKey = UserDefaults.standard.string(forKey: "settingsScrollTo")

    /// The group `-settingsScrollTo` names, by its id or a setting in it.
    var launchGroup: SettingsGroup? {
        guard let key = launchKey else { return nil }
        if let group = SettingsGroup(rawValue: key) { return group }
        if key.hasPrefix("sync.") { return .sync }
        return items.first { $0.key == key }.map(SettingsGroup.of)
    }

    func load(_ model: AppModel) {
        let kind = model.library.kind
        items = (model.library.vault?.settings() ?? []).filter { Self.showsInList($0.key, kind: kind) }
    }

    func write(_ key: String, _ value: SettingValue, model: AppModel) {
        do {
            try model.library.vault?.setSetting(key: key, value: value)
            problem = nil
            model.library.reloadConfig()
            if key.hasPrefix("sync.") { model.sync.reloadSettings() }
        } catch {
            problem = error.shownMessage
        }
        load(model)
    }

    /// The setting key to scroll to once, then never again.
    func takeLaunchKey() -> String? {
        defer { launchKey = nil }
        return launchKey
    }

    func shows(_ group: SettingsGroup) -> Bool {
        group.hasOwnRows || items.contains { SettingsGroup.of($0) == group }
    }

    /// The group's settings by schema section, in the schema's order.
    func blocks(in group: SettingsGroup) -> [SettingsBlock] {
        var order: [String] = []
        var bySection: [String: [SettingItem]] = [:]
        for item in items where SettingsGroup.of(item) == group {
            if bySection[item.section] == nil { order.append(item.section) }
            bySection[item.section, default: []].append(item)
        }
        return order.map { SettingsBlock(title: $0, items: bySection[$0] ?? []) }
    }

    /// The sync screen shows the repository, branches and interval itself;
    /// the rest of sync's settings only matter in the synced vault.
    private static func showsInList(_ key: String, kind: VaultKind) -> Bool {
        guard key.hasPrefix("sync.") else { return true }
        let ownRows = ["sync.branch", "sync.legacy-branch", "sync.interval-minutes"]
        return kind == .synced && !ownRows.contains(key)
    }
}
