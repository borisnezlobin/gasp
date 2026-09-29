import SwiftUI
import UIKit

/// The desktop keymap with a hardware keyboard: one hidden button per key
/// binding, which SwiftUI turns into a key command that works wherever
/// the cursor is and lists in the overlay shown while Command is held.
/// While a note is being edited, its text view answers the same keys
/// first (see `uiKeyCommands`), so the text view's own handling of keys
/// such as Command-B never gets in the way. The core leaves out the keys
/// the text view moves and deletes with.
struct KeyCommands: View {
    let bindings: [KeyBinding]
    let commands: [CommandInfo]
    let run: (String) -> Void

    /// A binding the text view doesn't handle itself, ready for SwiftUI.
    private struct Shortcut {
        let command: String
        let key: KeyEquivalent
        let modifiers: EventModifiers
    }

    var body: some View {
        ZStack {
            ForEach(Array(shortcuts.enumerated()), id: \.offset) { _, shortcut in
                Button(title(of: shortcut.command)) { run(shortcut.command) }
                    .keyboardShortcut(shortcut.key, modifiers: shortcut.modifiers)
            }
        }
        .frame(width: 0, height: 0)
        .opacity(0)
        .accessibilityHidden(true)
    }

    /// Keys with Command, Control or Option. Tab and Escape alone stay with
    /// whatever has focus, so a search field keeps its own Tab.
    private var shortcuts: [Shortcut] {
        bindings.compactMap { binding in
            let modified = binding.commandKey || binding.control || binding.option
            guard modified, let key = Self.key(binding.input) else { return nil }
            return Shortcut(command: binding.command, key: key, modifiers: Self.modifiers(binding))
        }
    }

    private func title(of command: String) -> String {
        commands.first { $0.id == command }?.title ?? command
    }

    private static let namedKeys: [String: KeyEquivalent] = [
        "up": .upArrow, "down": .downArrow, "left": .leftArrow, "right": .rightArrow,
        "escape": .escape, "tab": .tab, "enter": .return, "space": .space,
        "backspace": .delete, "delete": .deleteForward, "home": .home, "end": .end,
        "pageup": .pageUp, "pagedown": .pageDown
    ]

    /// The bindings as UIKit key commands that run `action` with the
    /// command's id as their property list. They have no title, so the
    /// overlay lists each key once, from the SwiftUI buttons.
    static func uiKeyCommands(_ bindings: [KeyBinding], action: Selector) -> [UIKeyCommand] {
        bindings.compactMap { binding in
            guard let input = uiInput(binding.input) else { return nil }
            let command = UIKeyCommand(
                action: action, input: input, modifierFlags: flags(binding), propertyList: binding.command
            )
            command.wantsPriorityOverSystemBehavior = true
            return command
        }
    }

    private static let namedInputs: [String: String] = [
        "up": UIKeyCommand.inputUpArrow, "down": UIKeyCommand.inputDownArrow,
        "left": UIKeyCommand.inputLeftArrow, "right": UIKeyCommand.inputRightArrow,
        "escape": UIKeyCommand.inputEscape, "tab": "\t", "enter": "\r", "space": " ",
        "backspace": "\u{8}", "delete": UIKeyCommand.inputDelete, "home": UIKeyCommand.inputHome,
        "end": UIKeyCommand.inputEnd, "pageup": UIKeyCommand.inputPageUp, "pagedown": UIKeyCommand.inputPageDown
    ]

    private static func uiInput(_ input: String) -> String? {
        if let named = namedInputs[input] { return named }
        if input.hasPrefix("f"), let number = Int(input.dropFirst()), (1...12).contains(number) {
            return functionKeys[number - 1]
        }
        return input.count == 1 ? input : nil
    }

    private static let functionKeys = [
        UIKeyCommand.f1, UIKeyCommand.f2, UIKeyCommand.f3, UIKeyCommand.f4, UIKeyCommand.f5, UIKeyCommand.f6,
        UIKeyCommand.f7, UIKeyCommand.f8, UIKeyCommand.f9, UIKeyCommand.f10, UIKeyCommand.f11, UIKeyCommand.f12
    ]

    private static func flags(_ binding: KeyBinding) -> UIKeyModifierFlags {
        var flags: UIKeyModifierFlags = []
        if binding.commandKey { flags.insert(.command) }
        if binding.shift { flags.insert(.shift) }
        if binding.option { flags.insert(.alternate) }
        if binding.control { flags.insert(.control) }
        return flags
    }

    private static func key(_ input: String) -> KeyEquivalent? {
        if let named = namedKeys[input] { return named }
        guard input.count == 1, let character = input.first else { return nil }
        return KeyEquivalent(character)
    }

    private static func modifiers(_ binding: KeyBinding) -> EventModifiers {
        var modifiers: EventModifiers = []
        if binding.commandKey { modifiers.insert(.command) }
        if binding.shift { modifiers.insert(.shift) }
        if binding.option { modifiers.insert(.option) }
        if binding.control { modifiers.insert(.control) }
        return modifiers
    }
}
