import SwiftUI

/// Settings: a short list of groups after the Mac app's settings pages,
/// each opening a screen of its own. `-settingsScrollTo <key>` at launch
/// opens the group holding that setting (or the group with that id) and
/// scrolls to it, for screenshots.
struct SettingsScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var store = SettingsStore()
    @State private var path: [SettingsGroup] = []

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        NavigationStack(path: $path) {
            groupList
                .textCase(nil)
                .navigationTitle("Settings")
                .navigationBarTitleDisplayMode(.inline)
                .navigationDestination(for: SettingsGroup.self) { SettingsGroupScreen(group: $0, store: store) }
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
                }
        }
        .font(Font(tokens.uiFont(size: tokens.bodySize)))
        .onAppear(perform: open)
        .onChange(of: model.library.configGeneration) { store.load(model) }
    }

    private var groupList: some View {
        List {
            if let problem = store.problem {
                Section { Text(problem).foregroundStyle(tokens.swiftUIColor(\.textStrong)) }
            }
            ForEach(SettingsShelf.allCases) { shelf in
                Section(shelf.rawValue) {
                    ForEach(shelf.groups.filter(store.shows)) { group in
                        NavigationLink(value: group) { SettingsGroupLabel(group: group, tokens: tokens) }
                    }
                }
            }
            Section {
                Button {
                    dismiss()
                    model.welcome.replay()
                } label: {
                    Label("Show the welcome tour", systemImage: "sparkles")
                }
            }
        }
    }

    private func open() {
        store.load(model)
        guard path.isEmpty, let group = store.launchGroup else { return }
        path = [group]
    }
}

/// A group's row on the top level: its symbol in the theme's icon colour
/// and its name.
private struct SettingsGroupLabel: View {
    let group: SettingsGroup
    let tokens: Tokens

    var body: some View {
        Label {
            Text(group.title).foregroundStyle(tokens.swiftUIColor(\.textStrong))
        } icon: {
            Image(systemName: group.symbol).foregroundStyle(tokens.swiftUIColor(\.icon))
        }
    }
}

/// One group's screen: its own rows (sync, the vault, toolbars, the app
/// icon), then its schema settings by section.
struct SettingsGroupScreen: View {
    @Environment(AppModel.self) private var model
    let group: SettingsGroup
    let store: SettingsStore

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        ScrollViewReader { scroller in
            Form {
                if let problem = store.problem {
                    Section { Text(problem).foregroundStyle(tokens.swiftUIColor(\.textStrong)) }
                }
                leadingRows
                schemaSections
                trailingRows
            }
            .onAppear { scrollToLaunchSetting(scroller) }
        }
        .textCase(nil)
        .navigationTitle(group.title)
        .navigationBarTitleDisplayMode(.inline)
    }

    @ViewBuilder private var leadingRows: some View {
        switch group {
        case .general: VaultChoiceSection()
        case .sync: SyncSettingsSection { store.write($0, $1, model: model) }
        case .toolbars: ToolbarSettingsSection(titled: false)
        default: EmptyView()
        }
    }

    @ViewBuilder private var trailingRows: some View {
        if group == .appearance { AppIconSection() }
    }

    private var schemaSections: some View {
        let blocks = store.blocks(in: group)
        let titled = blocks.count > 1 || group.hasOwnRows
        return ForEach(blocks) { block in
            Section {
                ForEach(block.items, id: \.key) { item in
                    SettingRow(item: item, tokens: tokens) { store.write(item.key, $0, model: model) }
                        .id(item.key)
                }
            } header: {
                if titled && block.title != group.title { Text(SettingsBlock.header(for: block.title)) }
            }
        }
    }

    private func scrollToLaunchSetting(_ scroller: ScrollViewProxy) {
        guard let key = store.takeLaunchKey() else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { scroller.scrollTo(key, anchor: .center) }
    }
}
