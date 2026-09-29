import UIKit

/// Syntax colours in fenced code blocks, as the desktop draws them: the
/// core names each stretch of code, and the theme's `color.code.*` tokens
/// colour it.
struct CodeColors {
    var spans: [CodeSpan] = []
    private let colors: [CodeColor: UIColor]

    init(vault: VaultFolder) {
        let kinds: [CodeColor] = [.comment, .string, .number, .constant, .keyword, .function, .type]
        colors = Dictionary(uniqueKeysWithValues: kinds.map { kind in
            (kind, UIColor(themed: vault.themeColor(name: codeColorToken(color: kind))))
        })
    }

    /// Colours the code that falls on `lines`.
    func paint(within lines: [NSRange], storage: NSTextStorage) {
        guard !spans.isEmpty else { return }
        for span in spans {
            let range = span.range.nsRange
            guard let color = colors[span.color] else { continue }
            for line in lines {
                let overlap = NSIntersectionRange(range, line)
                guard overlap.length > 0, NSMaxRange(overlap) <= storage.length else { continue }
                storage.addAttribute(.foregroundColor, value: color, range: overlap)
            }
        }
    }
}

/// The code colours arrive off the main thread a moment after the note
/// opens or changes, and the code blocks' lines are restyled with them.
extension EditingController {
    private static let colourDelay: TimeInterval = 0.3

    func scheduleCodeColours() {
        codeColouring?.cancel()
        let work = DispatchWorkItem { [weak self] in self?.colourCode() }
        codeColouring = work
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.colourDelay, execute: work)
    }

    private func colourCode() {
        let (document, version) = (document, editCount)
        GrammarService.queue.async { [weak self] in
            let spans = document.codeSpans()
            DispatchQueue.main.async {
                guard let self, self.editCount == version, spans != self.code.spans else { return }
                self.code.spans = spans
                self.styler.forgetLines { line in
                    line.decorations.contains { if case .codeBlock = $0 { true } else { false } }
                }
                self.restyle(edited: nil)
            }
        }
    }
}
