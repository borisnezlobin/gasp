import SwiftUI

/// The smallest and largest value a number setting takes. The schema only
/// says a setting is a whole number, so the few with a floor above zero
/// are listed here, as the desktop's settings screen lists them.
struct NumberLimits: Equatable {
    let minimum: Double
    let maximum: Double

    private static let wholeNumber = NumberLimits(minimum: 0, maximum: Double(UInt32.max))
    private static let known: [String: NumberLimits] = [
        "appearance.base-font-size": NumberLimits(minimum: 6, maximum: 72),
        "sync.interval-minutes": NumberLimits(minimum: 1, maximum: Double(UInt32.max)),
        "recovery.interval-minutes": NumberLimits(minimum: 1, maximum: Double(UInt32.max)),
        "recovery.keep-days": NumberLimits(minimum: 1, maximum: Double(UInt32.max)),
        "prose.sentence-length.short-below": NumberLimits(minimum: 1, maximum: Double(UInt32.max)),
        "prose.sentence-length.long-above": NumberLimits(minimum: 1, maximum: Double(UInt32.max))
    ]

    static func forSetting(_ key: String) -> NumberLimits {
        known[key] ?? wholeNumber
    }

    func clamped(_ value: Double) -> Double {
        min(max(value, minimum), maximum)
    }

    /// The typed text as a value in range, or nil when it isn't a number.
    func value(fromTyped text: String, wholeNumbers: Bool) -> Double? {
        let trimmed = text.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: ",", with: ".")
        guard let typed = Double(trimmed), typed.isFinite else { return nil }
        return clamped(wholeNumbers ? typed.rounded() : typed)
    }
}

/// A number setting in the shared row layout, its control one capsule
/// holding − and + around the value. Tapping the value types a new one on
/// the number pad; it's kept in range when editing ends.
struct NumberSettingRow: View {
    let item: SettingItem
    let tokens: Tokens
    let write: (SettingValue) -> Void
    @State private var draft = ""
    @FocusState private var typing: Bool

    /// The control's height, and the width of each of its buttons.
    private static let controlHeight: CGFloat = 36
    private static let buttonWidth: CGFloat = 40

    private var isWhole: Bool {
        if case .integer = item.value { return true }
        return false
    }

    private var value: Double {
        switch item.value {
        case .integer(let value): Double(value)
        case .number(let value): value
        default: 0
        }
    }

    private var limits: NumberLimits { NumberLimits.forSetting(item.key) }
    private var step: Double { isWhole ? 1 : 0.5 }

    var body: some View {
        SettingRowLayout(title: item.title, description: item.description, tokens: tokens, labelsControl: false) {
            control
        }
    }

    /// − value +, as one capsule.
    private var control: some View {
        HStack(spacing: 0) {
            stepButton(symbol: "minus", by: -step, label: "Less", enabled: value > limits.minimum)
            valueField
            stepButton(symbol: "plus", by: step, label: "More", enabled: value < limits.maximum)
        }
        .frame(height: Self.controlHeight)
        .background(Capsule().fill(tokens.swiftUIColor(\.fillStrong)))
        .fixedSize()
    }

    private func stepButton(symbol: String, by change: Double, label: String, enabled: Bool) -> some View {
        Button {
            let stepped = limits.clamped(value + change)
            let atEnd = stepped == limits.minimum || stepped == limits.maximum
            atEnd ? Haptics.limit() : Haptics.tick()
            save(stepped)
        } label: {
            Image(systemName: symbol)
                .font(.system(size: 14, weight: .semibold))
                .frame(width: Self.buttonWidth, height: Self.controlHeight)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(tokens.swiftUIColor(\.text))
        .opacity(enabled ? 1 : 0.3)
        .disabled(!enabled)
        .accessibilityLabel("\(label) \(item.title)")
    }

    private var valueField: some View {
        TextField("", text: $draft)
            .keyboardType(isWhole ? .numberPad : .decimalPad)
            .focused($typing)
            .multilineTextAlignment(.center)
            .font(Font(tokens.uiFont(size: tokens.bodySize, bold: true)).monospacedDigit())
            .foregroundStyle(tokens.swiftUIColor(\.textStrong))
            .frame(minWidth: tokens.bodySize * 1.6)
            .fixedSize()
            .padding(.horizontal, tokens.spacing.xs)
            .frame(height: Self.controlHeight - 8)
            .background(
                Capsule().fill(tokens.swiftUIColor(\.selection)).opacity(typing ? 1 : 0)
            )
            .accessibilityLabel(item.title)
            .accessibilityValue(draft)
            .accessibilityHint("Type a number")
            .toolbar {
                if typing {
                    ToolbarItemGroup(placement: .keyboard) {
                        Spacer()
                        Button("Done") { typing = false }
                    }
                }
            }
            .onAppear { draft = Self.shown(value) }
            .onChange(of: item.value) { draft = Self.shown(value) }
            .onChange(of: typing) { _, isTyping in if !isTyping { commit() } }
    }

    private func commit() {
        guard let typed = limits.value(fromTyped: draft, wholeNumbers: isWhole) else {
            draft = Self.shown(value)
            return
        }
        draft = Self.shown(typed)
        if typed != value { save(typed) }
    }

    private func save(_ number: Double) {
        write(isWhole ? .integer(value: Int64(number.rounded())) : .number(value: number))
    }

    /// `5` for a whole number, `1.5` otherwise.
    static func shown(_ number: Double) -> String {
        number.rounded() == number ? String(Int64(number)) : String(number)
    }
}
