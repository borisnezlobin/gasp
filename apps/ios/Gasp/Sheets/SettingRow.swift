import SwiftUI

/// Every setting row's shape: the title with its description underneath
/// on the leading side, and the control centred on both at the trailing
/// side, so the description wraps before the control starts. At the
/// accessibility text sizes the control can move under the text.
///
/// When `labelsControl` is on, the text is hidden from VoiceOver and the
/// control reads as the title, with the description as its hint.
struct SettingRowLayout<Control: View>: View {
    let title: String
    let description: String
    let tokens: Tokens
    var stacksAtLargeSizes = true
    var labelsControl = true
    @ViewBuilder let control: () -> Control
    @Environment(\.dynamicTypeSize) private var typeSize

    var body: some View {
        if stacksAtLargeSizes && typeSize.isAccessibilitySize {
            VStack(alignment: .leading, spacing: tokens.spacing.sm) {
                text
                labelledControl
            }
        } else {
            HStack(alignment: .center, spacing: tokens.spacing.md) {
                text
                labelledControl
            }
        }
    }

    private var text: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
            Text(title)
            if !description.isEmpty {
                Text(description)
                    .font(Font(tokens.uiFont(size: tokens.smallSize)))
                    .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityHidden(labelsControl)
    }

    @ViewBuilder private var labelledControl: some View {
        if labelsControl {
            control()
                .accessibilityLabel(title)
                .accessibilityHint(description)
        } else {
            control()
        }
    }
}

/// One schema setting's row, with the control its kind needs.
struct SettingRow: View {
    let item: SettingItem
    let tokens: Tokens
    let write: (SettingValue) -> Void

    var body: some View {
        switch (item.control, item.value) {
        case (.integer, _), (.number, _):
            NumberSettingRow(item: item, tokens: tokens, write: write)
        case (.list, .list(let values)):
            ListField(item: item, values: values, tokens: tokens) { write(.list(values: $0)) }
        case (.switch, .bool(let value)):
            SettingRowLayout(
                title: item.title, description: item.description, tokens: tokens, stacksAtLargeSizes: false
            ) {
                Toggle(item.title, isOn: Binding(get: { value }, set: { write(.bool(value: $0)) }))
                    .labelsHidden()
            }
        case (.choice(let options), .text(let value)):
            SettingRowLayout(title: item.title, description: item.description, tokens: tokens) {
                Picker(item.title, selection: Binding(get: { value }, set: { write(.text(value: $0)) })) {
                    ForEach(options, id: \.self) { Text(Self.humanize($0)).tag($0) }
                }
                .labelsHidden()
                .pickerStyle(.menu)
                .fixedSize()
            }
        default:
            SettingRowLayout(title: item.title, description: item.description, tokens: tokens) {
                TextSettingField(title: item.title, value: Self.text(of: item.value)) { text in
                    write(Self.parse(text, like: item.value))
                }
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

/// A text setting's field, written when editing ends.
private struct TextSettingField: View {
    let title: String
    let value: String
    let write: (String) -> Void
    @State private var draft = ""

    private static let width: CGFloat = 140

    var body: some View {
        TextField(title, text: $draft)
            .multilineTextAlignment(.trailing)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .frame(width: Self.width)
            .onSubmit { write(draft) }
            .onAppear { draft = value }
    }
}

/// A list setting, one item per line, under its title and description.
private struct ListField: View {
    let item: SettingItem
    let values: [String]
    let tokens: Tokens
    let write: ([String]) -> Void
    @State private var draft = ""
    @FocusState private var editing: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
            Text(item.title)
            if !item.description.isEmpty {
                Text(item.description)
                    .font(Font(tokens.uiFont(size: tokens.smallSize)))
                    .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                    .fixedSize(horizontal: false, vertical: true)
            }
            TextEditor(text: $draft)
                .font(.system(.footnote, design: .monospaced))
                .frame(minHeight: 120)
                .focused($editing)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .accessibilityLabel(item.title)
        }
        .onAppear { draft = values.joined(separator: "\n") }
        .onChange(of: editing) { _, isEditing in
            guard !isEditing else { return }
            write(draft.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty })
        }
    }
}
