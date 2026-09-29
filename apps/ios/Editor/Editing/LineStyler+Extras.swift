import UIKit

/// Widgets drawn beside a line rather than in it: the live math preview
/// above an equation being edited or below a `$$` block, and an image
/// below its embed while the embed's source shows. The line makes room
/// with paragraph spacing, and its layout fragment draws the picture.
extension LineStyler {
    func addExtras(_ paragraph: NSRange, _ style: NSParagraphStyle) {
        var above: FragmentExtra?
        var below: FragmentExtra?
        for widget in line.widgets where widget.placement != .replace {
            guard let extra = extra(for: widget, style) else { continue }
            if extra.place == .above { above = extra } else { below = extra }
        }
        guard above != nil || below != nil, let range = clamped(paragraph) else { return }
        let extras = FragmentExtras(above: above, below: below, trailing: nil, indent: style.headIndent)
        storage.addAttribute(.fragmentExtras, value: extras, range: range)
        adjustParagraphStyle(paragraph) { style in
            style.paragraphSpacingBefore += above?.room ?? 0
            style.paragraphSpacing += below?.room ?? 0
        }
    }

    private func extra(for widget: Widget, _ style: NSParagraphStyle) -> FragmentExtra? {
        let place: FragmentExtra.Place = widget.placement == .above ? .above : .below
        let gap = CGFloat(tokens.spacing.sm)
        switch widget.kind {
        case .mathPreview(let tex, let display):
            let size = display ? tokens.bodySize : baseLook.size(tokens)
            guard let math = media?.math(MathKey(tex: tex, display: display, fontSize: size)) else { return nil }
            let content = FragmentExtra.Content.math(math, color: tokens.color(\.text))
            return FragmentExtra(place: place, content: content, centered: display, gap: gap)
        case .image(let target, _, let width, let height, _):
            guard let attachment = imageAttachment(target: target, requested: (width, height), style) else {
                return nil
            }
            return FragmentExtra(place: place, content: .image(attachment), centered: false, gap: gap)
        default:
            return nil
        }
    }
}
