import SwiftUI

/// Every setting in the vault's `.gasp/settings.toml`, grouped as the
/// schema groups them, each with the control its kind needs. Changes are
/// written back at once, and a value the app wouldn't load is refused
/// with the reason.
struct SettingsScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var items: [SettingItem] = []
    @State private var problem: String?

    private var tokens: Tokens { model.library.tokens }

    private var sections: [(title: String, items: [SettingItem])] {
        var order: [String] = []
        var bySection: [String: [SettingItem]] = [:]
        for item in items {
            if bySection[item.section] == nil { order.append(item.section) }
            bySection[item.section, default: []].append(item)
        }
        return order.map { ($0, bySection[$0] ?? []) }
    }

    var body: some View {
        NavigationStack {
            Form {
                if let problem {
                    Section { Text(problem).foregroundStyle(tokens.swiftUIColor(\.textStrong)) }
                }
                Section {
                    Button("Open another vault") {
                        dismiss()
                        model.workspace.prompt = .pickVault
                    }
                }
                ForEach(sections, id: \.title) { section in
                    Section(section.title) {
                        ForEach(section.items, id: \.key) { item in
                            SettingRow(item: item, tokens: tokens) { write(item.key, $0) }
                        }
                    }
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
            }
        }
        .font(Font(tokens.uiFont(size: tokens.bodySize)))
        .onAppear(perform: load)
    }

    private func load() {
        items = model.library.vault?.settings() ?? []
    }

    private func write(_ key: String, _ value: SettingValue) {
        do {
            try model.library.vault?.setSetting(key: key, value: value)
            problem = nil
            model.library.reloadConfig()
        } catch {
            problem = error.localizedDescription
        }
        load()
    }
}

/// One setting's control.
private struct SettingRow: View {
    let item: SettingItem
    let tokens: Tokens
    let write: (SettingValue) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
            control
            if !item.description.isEmpty {
                Text(item.description)
                    .font(Font(tokens.uiFont(size: tokens.smallSize)))
                    .foregroundStyle(tokens.swiftUIColor(\.textDetail))
            }
        }
    }

    @ViewBuilder private var control: some View {
        switch (item.control, item.value) {
        case (.switch, .bool(let value)):
            Toggle(item.title, isOn: Binding(get: { value }, set: { write(.bool(value: $0)) }))
        case (.choice(let options), .text(let value)):
            Picker(item.title, selection: Binding(get: { value }, set: { write(.text(value: $0)) })) {
                ForEach(options, id: \.self) { Text(Self.humanize($0)).tag($0) }
            }
        case (.integer, .integer(let value)):
            Stepper("\(item.title): \(value)", value: Binding(
                get: { Int(value) }, set: { write(.integer(value: Int64($0))) }
            ))
        case (.list, .list(let values)):
            ListField(title: item.title, values: values, tokens: tokens) { write(.list(values: $0)) }
        default:
            TextSettingField(title: item.title, value: Self.text(of: item.value)) { text in
                write(Self.parse(text, like: item.value))
            }
        }
    }

    /// `match-system` → `Match system`.
    private static func humanize(_ option: String) -> String {
        let spaced = option.replacingOccurrences(of: "-", with: " ")
        return spaced.prefix(1).uppercased() + spaced.dropFirst()
    }

    private static func text(of value: SettingValue) -> String {
        switch value {
        case .text(let text): text
        case .number(let number): String(number)
        case .integer(let number): String(number)
        default: ""
        }
    }

    private static func parse(_ text: String, like value: SettingValue) -> SettingValue {
        if case .number = value, let number = Double(text) { return .number(value: number) }
        return .text(value: text)
    }
}

/// A text setting, written when editing ends.
private struct TextSettingField: View {
    let title: String
    let value: String
    let write: (String) -> Void
    @State private var draft = ""

    var body: some View {
        LabeledContent(title) {
            TextField(title, text: $draft)
                .multilineTextAlignment(.trailing)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .onSubmit { write(draft) }
        }
        .onAppear { draft = value }
    }
}

/// A list setting, one item per line.
private struct ListField: View {
    let title: String
    let values: [String]
    let tokens: Tokens
    let write: ([String]) -> Void
    @State private var draft = ""
    @FocusState private var editing: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
            Text(title)
            TextEditor(text: $draft)
                .font(.system(.footnote, design: .monospaced))
                .frame(minHeight: 120)
                .focused($editing)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
        }
        .onAppear { draft = values.joined(separator: "\n") }
        .onChange(of: editing) { _, isEditing in
            guard !isEditing else { return }
            write(draft.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty })
        }
    }
}
