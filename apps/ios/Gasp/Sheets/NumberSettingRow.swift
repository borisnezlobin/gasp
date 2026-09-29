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

/// A number setting: its name and value on one line with a stepper at the
/// trailing side, and its description wrapping under the name. Tapping the
/// value types a new one on the number pad; it's kept in range when
/// editing ends. At the accessibility text sizes the stepper moves under
/// the text, so nothing overlaps.
struct NumberSettingRow: View {
    let item: SettingItem
    let tokens: Tokens
    let write: (SettingValue) -> Void
    @Environment(\.dynamicTypeSize) private var typeSize
    @State private var draft = ""
    @FocusState private var typing: Bool

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
        if typeSize.isAccessibilitySize {
            VStack(alignment: .leading, spacing: tokens.spacing.sm) {
                text.frame(maxWidth: .infinity, alignment: .leading)
                stepper
            }
        } else {
            HStack(alignment: .top, spacing: tokens.spacing.md) {
                text
                Spacer(minLength: 0)
                stepper
            }
        }
    }

    private var text: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
            HStack(alignment: .firstTextBaseline, spacing: tokens.spacing.sm) {
                Text(item.title)
                valueField
            }
            if !item.description.isEmpty {
                Text(item.description)
                    .font(Font(tokens.uiFont(size: tokens.smallSize)))
                    .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private var valueField: some View {
        TextField("", text: $draft)
            .keyboardType(isWhole ? .numberPad : .decimalPad)
            .focused($typing)
            .font(Font(tokens.uiFont(size: tokens.bodySize, bold: true)).monospacedDigit())
            .frame(minWidth: tokens.bodySize)
            .foregroundStyle(tokens.swiftUIColor(\.textStrong))
            .fixedSize()
            .padding(.horizontal, tokens.spacing.sm)
            .padding(.vertical, 2)
            .background(
                Capsule().fill(tokens.swiftUIColor(typing ? \.selection : \.fillStrong))
            )
            .accessibilityLabel(item.title)
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

    private var stepper: some View {
        Stepper(item.title, value: Binding(get: { value }, set: save), in: limits.minimum...limits.maximum, step: step)
            .labelsHidden()
            .fixedSize()
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
