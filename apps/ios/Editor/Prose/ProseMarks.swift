import UIKit

/// Sentence-length tints and grammar flags for a note, as the styler lays
/// them on restyled lines.
struct ProseMarks {
    var tints: [SentenceTint] = []
    var flags: [GrammarFlag] = []
    var colors: ProseColors?

    /// Marks the flags that fall on `lines`.
    func mark(within lines: [NSRange], storage: NSTextStorage) {
        guard let colors, !flags.isEmpty else { return }
        for flag in flags {
            let range = flag.range.nsRange
            for line in lines {
                let overlap = NSIntersectionRange(range, line)
                guard overlap.length > 0, NSMaxRange(overlap) <= storage.length else { continue }
                storage.addAttribute(.grammarFlag, value: GrammarMark(flag: flag, color: colors.color(flag.kind)),
                                     range: overlap)
            }
        }
    }
}

/// The grammar underlines' colours, from the vault's theme.
struct ProseColors {
    let spelling: UIColor
    let mechanical: UIColor

    init(vault: VaultFolder) {
        spelling = UIColor(themed: vault.themeColor(name: "color.flag-spelling"))
        mechanical = UIColor(themed: vault.themeColor(name: "color.flag-mechanical"))
    }

    func color(_ kind: GrammarFlagKind) -> UIColor {
        switch kind {
        case .spelling: spelling
        case .mechanical: mechanical
        }
    }
}

extension UIColor {
    /// A colour token that follows light and dark mode.
    convenience init(themed: ThemedColor) {
        let light = UIColor(themed.light)
        let dark = UIColor(themed.dark)
        self.init { traits in traits.userInterfaceStyle == .dark ? dark : light }
    }
}

/// One grammar checker per vault, shared by its notes. It learns the
/// vault's words in the background once, and checks on its own queue.
enum GrammarService {
    private static var checkers: [ObjectIdentifier: GrammarChecker] = [:]
    static let queue = DispatchQueue(label: "com.borisnezlobin.editor.grammar", qos: .utility)

    /// The vault's checker, made on first use. Called on the main thread.
    static func checker(for vault: VaultFolder) -> GrammarChecker {
        let id = ObjectIdentifier(vault)
        if let checker = checkers[id] { return checker }
        let checker = GrammarChecker(vault: vault)
        checkers[id] = checker
        if checker.isEnabled() {
            queue.async { checker.learnVaultWords() }
        }
        return checker
    }

    /// Forgets the checkers, after the vault's settings changed.
    static func reset() {
        checkers = [:]
    }
}
