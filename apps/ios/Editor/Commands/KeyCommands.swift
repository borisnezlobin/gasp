import SwiftUI

/// The desktop keymap with a hardware keyboard: one hidden button per key
/// binding, which SwiftUI turns into a key command that works wherever
/// the cursor is and lists in the overlay shown while Command is held. The
/// core leaves out the keys the text view moves and deletes with.
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
