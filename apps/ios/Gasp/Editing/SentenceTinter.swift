import UIKit

/// Sentence-length highlighting: a tint behind each sentence by how long
/// it reads, under anything that already has a fill of its own, such as
/// inline code or a highlight.
struct SentenceTinter {
    let tokens: Tokens
    let storage: NSTextStorage

    func tint(_ tints: [SentenceTint], within lines: [NSRange]) {
        guard !tints.isEmpty else { return }
        for tint in tints {
            let sentence = tint.range.nsRange
            let color = self.color(tint.length)
            for line in lines {
                let overlap = NSIntersectionRange(sentence, line)
                guard overlap.length > 0, NSMaxRange(overlap) <= storage.length else { continue }
                fillUnfilled(overlap, with: color)
            }
        }
    }

    private func fillUnfilled(_ range: NSRange, with color: UIColor) {
        storage.enumerateAttribute(.backgroundColor, in: range) { value, run, _ in
            if value == nil { storage.addAttribute(.backgroundColor, value: color, range: run) }
        }
    }

    private func color(_ length: SentenceLength) -> UIColor {
        switch length {
        case .short: tokens.color(\.sentenceShort)
        case .medium: tokens.color(\.sentenceMedium)
        case .long: tokens.color(\.sentenceLong)
        }
    }
}
