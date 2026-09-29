import SwiftUI

/// The phone's toolbars in Settings, the keyboard bar and the bottom bar,
/// each opening a page to change it. Changes go to the vault's
/// `.gasp/toolbars.toml`, which syncs to the desktop like any setting.
struct ToolbarSettingsSection: View {
    @Environment(AppModel.self) private var model
    /// Whether the section names itself; not in a sheet already titled
    /// Toolbars.
    var titled = true
    @State private var bars: [ToolbarSetup] = []

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        Section {
            ForEach(bars, id: \.id) { bar in
                NavigationLink { ToolbarEditor(id: bar.id) } label: { row(bar) }
            }
        } header: {
            if titled { Text("Toolbars") }
        }
        .onAppear(perform: load)
        .onChange(of: model.library.configGeneration) { load() }
    }

    private func row(_ bar: ToolbarSetup) -> some View {
        HStack(spacing: tokens.spacing.md) {
            Image(systemName: bar.isBrowserBar ? "dock.rectangle" : "keyboard")
                .foregroundStyle(tokens.swiftUIColor(\.icon))
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(bar.title)
                Text(Self.summary(of: bar))
                    .font(Font(tokens.uiFont(size: tokens.smallSize)))
                    .foregroundStyle(tokens.swiftUIColor(\.textDetail))
            }
        }
    }

    private func load() {
        bars = model.library.vault?.phoneToolbars() ?? []
    }

    /// "12 buttons", or that it's off.
    private static func summary(of bar: ToolbarSetup) -> String {
        guard bar.enabled || bar.isBrowserBar else { return "Turned off" }
        let buttons = bar.items.filter(\.isButton).count
        return buttons == 1 ? "1 button" : "\(buttons) buttons"
    }
}

/// Customize toolbars from the palette: the same rows in a sheet of their
/// own. `-toolbarPage <id>` at launch opens a bar's page in it straight
/// away, to look at it in a screenshot.
struct ToolbarSettingsSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var pages = UserDefaults.standard.string(forKey: "toolbarPage").map { [$0] } ?? []

    var body: some View {
        NavigationStack(path: $pages) {
            Form { ToolbarSettingsSection(titled: false) }
                .textCase(nil)
                .navigationDestination(for: String.self) { ToolbarEditor(id: $0) }
                .navigationTitle("Toolbars")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
                }
        }
        .font(Font(model.library.tokens.uiFont(size: model.library.tokens.bodySize)))
    }
}

/// One toolbar's page: whether it shows and how its buttons read (for the
/// keyboard bar), then its items to drag into order, delete or add to.
struct ToolbarEditor: View {
    @Environment(AppModel.self) private var model
    let id: String
    @State private var setup: ToolbarSetup?
    @State private var problem: String?
    @State private var picking = false
    @State private var confirmingReset = false

    private var tokens: Tokens { model.library.tokens }
    private static let browserBarNote = "Swipe the note title to move between tabs, or tap it to see them all."
    private static let keyboardBarNote = "The button that hides the keyboard always sits at the bar's right end, "
        + "and undo and redo float just above the bar."

    var body: some View {
        List {
            if let problem {
                Section { Text(problem).foregroundStyle(tokens.swiftUIColor(\.conflict)) }
            }
            if let setup {
                if !setup.isBrowserBar { lookSection(setup) }
                itemsSection(setup)
                if setup.changed { resetSection(setup) }
            }
        }
        .environment(\.editMode, .constant(.active))
        .textCase(nil)
        .navigationTitle(setup?.title ?? "")
        .navigationBarTitleDisplayMode(.inline)
        .sheet(isPresented: $picking) {
            if let setup {
                ToolbarItemPicker(choices: addable(to: setup), browserBar: setup.isBrowserBar, tokens: tokens) {
                    write(setup.items + [$0])
                }
            }
        }
        .onAppear(perform: load)
        .onChange(of: model.library.configGeneration) { load() }
    }

    private func lookSection(_ setup: ToolbarSetup) -> some View {
        Section {
            Toggle("Show above the keyboard", isOn: Binding(
                get: { setup.enabled },
                set: { enabled in change { try $0.setToolbarEnabled(id: id, enabled: enabled) } }
            ))
            Picker("Buttons show", selection: Binding(
                get: { setup.labels },
                set: { labels in change { try $0.setToolbarLabels(id: id, labels: labels) } }
            )) {
                Text("Icons").tag(ToolbarLabels.icons)
                Text("Icons and labels").tag(ToolbarLabels.iconsAndLabels)
                Text("Labels").tag(ToolbarLabels.labels)
            }
        }
    }

    private func itemsSection(_ setup: ToolbarSetup) -> some View {
        Section {
            ForEach(Array(setup.items.enumerated()), id: \.offset) { _, item in
                ToolbarItemRow(item: item, browserBar: setup.isBrowserBar, tokens: tokens)
                    .deleteDisabled(setup.isBrowserBar && item.kind == .spacer)
            }
            .onMove { from, destination in
                var items = setup.items
                items.move(fromOffsets: from, toOffset: destination)
                write(items)
            }
            .onDelete { offsets in
                var items = setup.items
                items.remove(atOffsets: offsets)
                write(items)
            }
            Button { picking = true } label: {
                Label("Add a button", systemImage: "plus")
                    .foregroundStyle(tokens.swiftUIColor(\.textStrong))
            }
        } footer: {
            Text(setup.isBrowserBar ? Self.browserBarNote : Self.keyboardBarNote)
        }
    }

    private func resetSection(_ setup: ToolbarSetup) -> some View {
        Section {
            Button("Put back the built-in \(setup.title.lowercased())", role: .destructive) {
                confirmingReset = true
            }
            .confirmationDialog(
                "Put this bar back as it came?", isPresented: $confirmingReset, titleVisibility: .visible
            ) {
                Button("Put it back", role: .destructive) { change { try $0.resetToolbar(id: id) } }
            }
        }
    }

    /// What the picker offers: commands and the sync status not on the bar
    /// yet, and lines, gaps and menus, which can repeat.
    private func addable(to setup: ToolbarSetup) -> [ToolbarItemChoice] {
        let present = Set(setup.items.filter(\.isOnce).map(\.item))
        let choices = model.library.vault?.toolbarChoices(id: id) ?? []
        return choices.filter { !$0.isOnce || !present.contains($0.item) }
    }

    private func load() {
        setup = model.library.vault?.phoneToolbars().first { $0.id == id }
    }

    private func write(_ items: [ToolbarItemChoice]) {
        setup?.items = items
        change { try $0.setToolbarItems(id: id, items: items.map(\.item)) }
    }

    /// Writes a change through the core, then reads the toolbars again;
    /// a change the app wouldn't load is refused with the reason.
    private func change(_ write: (VaultFolder) throws -> Void) {
        guard let vault = model.library.vault else { return }
        do {
            try write(vault)
            problem = nil
            model.library.reloadConfig()
        } catch {
            problem = error.shownMessage
        }
        load()
    }
}

/// An item on a toolbar's page or in the picker: its symbol and name.
private struct ToolbarItemRow: View {
    let item: ToolbarItemChoice
    let browserBar: Bool
    let tokens: Tokens

    var body: some View {
        HStack(spacing: tokens.spacing.md) {
            Image(systemName: item.symbol(browserBar: browserBar))
                .foregroundStyle(tokens.swiftUIColor(\.icon))
                .frame(width: 28)
            Text(item.title)
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
        }
    }
}

/// Every command the phone runs and the rest a bar can hold, by group,
/// with a search field. Picking one adds it at the bar's end.
private struct ToolbarItemPicker: View {
    let choices: [ToolbarItemChoice]
    let browserBar: Bool
    let tokens: Tokens
    let pick: (ToolbarItemChoice) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""

    private var groups: [(title: String, items: [ToolbarItemChoice])] {
        let words = query.lowercased().split(separator: " ")
        let shown = choices.filter { choice in
            let haystack = "\(choice.title) \(choice.category)".lowercased()
            return words.allSatisfy { haystack.contains($0) }
        }
        var order: [String] = []
        var byGroup: [String: [ToolbarItemChoice]] = [:]
        for choice in shown {
            if byGroup[choice.category] == nil { order.append(choice.category) }
            byGroup[choice.category, default: []].append(choice)
        }
        return order.map { ($0, byGroup[$0] ?? []) }
    }

    var body: some View {
        NavigationStack {
            List {
                ForEach(groups, id: \.title) { group in
                    Section(group.title) {
                        ForEach(group.items, id: \.item) { choice in
                            Button {
                                pick(choice)
                                dismiss()
                            } label: {
                                ToolbarItemRow(item: choice, browserBar: browserBar, tokens: tokens)
                            }
                        }
                    }
                }
            }
            .textCase(nil)
            .overlay { if groups.isEmpty { ContentUnavailableView.search(text: query) } }
            .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Find a command")
            .navigationTitle("Add a button")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
        }
    }
}

extension ToolbarItemChoice {
    /// A button on the bar, rather than a line or a gap.
    var isButton: Bool {
        switch kind {
        case .command, .menu, .widget: true
        case .separator, .spacer: false
        }
    }

    /// Something a bar holds once: a command or the sync status.
    var isOnce: Bool {
        switch kind {
        case .command, .widget: true
        case .separator, .spacer, .menu: false
        }
    }

    func symbol(browserBar: Bool) -> String {
        switch kind {
        case .command(let id): CommandSymbols.name(for: id)
        case .separator: "poweron"
        case .spacer: browserBar ? "textformat" : "arrow.left.and.right"
        case .menu: "ellipsis.circle"
        case .widget: "checkmark.icloud"
        }
    }
}
