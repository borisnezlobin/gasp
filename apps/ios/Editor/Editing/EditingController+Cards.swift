import SwiftUI
import UIKit

/// Small cards that point at the text they're about: the grammar checker's
/// suggestion for an underlined word, and a footnote's text, the phone's
/// stand-in for the desktop's hover.
extension EditingController: UIPopoverPresentationControllerDelegate {
    func showCard(for flag: GrammarFlag, at range: NSRange) {
        let card = FlagCard(flag: flag, tokens: tokens) { [weak self] choice in
            self?.dismissCard()
            switch choice {
            case .replace(let text): self?.accept(flag, replacement: text)
            case .ignore: self?.ignore(flag)
            }
        }
        present(card, pointingAt: range)
    }

    func showFootnote(_ label: String, at range: NSRange) {
        let text = document.footnoteText(label: label) ?? "This footnote has no text yet."
        let card = FootnoteCard(label: label, text: text, tokens: tokens) { [weak self] in
            guard let self else { return }
            dismissCard()
            textView.selectedRange = NSRange(location: range.location, length: 0)
            host?.run("footnote.insert-or-jump")
        }
        present(card, pointingAt: range)
    }

    func adaptivePresentationStyle(
        for controller: UIPresentationController, traitCollection: UITraitCollection
    ) -> UIModalPresentationStyle {
        .none
    }

    private func present(_ card: some View, pointingAt range: NSRange) {
        guard let presenter = topController() else { return }
        let hosting = UIHostingController(rootView: card)
        hosting.modalPresentationStyle = .popover
        hosting.view.backgroundColor = tokens.color(\.popover)
        let width = min(textView.bounds.width - CGFloat(tokens.spacing.xl) * 2, 340)
        hosting.preferredContentSize = hosting.sizeThatFits(in: CGSize(width: width, height: .greatestFiniteMagnitude))
        if let popover = hosting.popoverPresentationController {
            popover.sourceView = textView
            popover.sourceRect = rect(of: range)
            popover.permittedArrowDirections = [.up, .down]
            popover.delegate = self
        }
        presenter.present(hosting, animated: true)
    }

    private func dismissCard() {
        topController()?.dismiss(animated: true)
    }

    private func rect(of range: NSRange) -> CGRect {
        let start = textView.position(from: textView.beginningOfDocument, offset: range.location)
        let end = start.flatMap { textView.position(from: $0, offset: range.length) }
        guard let start, let end, let textRange = textView.textRange(from: start, to: end) else { return .zero }
        return textView.firstRect(for: textRange)
    }

    private func topController() -> UIViewController? {
        var top = textView.window?.rootViewController
        while let presented = top?.presentedViewController { top = presented }
        return top
    }
}

/// What the grammar card offers.
enum FlagChoice {
    case replace(String)
    case ignore
}

/// The grammar checker's card: what's wrong, the fixes it suggests, and
/// a way to leave the phrase alone from now on.
private struct FlagCard: View {
    let flag: GrammarFlag
    let tokens: Tokens
    let choose: (FlagChoice) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.md) {
            Text(flag.message)
                .font(Font(tokens.uiFont(size: tokens.bodySize)))
                .foregroundStyle(tokens.swiftUIColor(\.text))
                .fixedSize(horizontal: false, vertical: true)
            if !flag.replacements.isEmpty {
                FlowButtons(replacements: flag.replacements, tokens: tokens) { choose(.replace($0)) }
            }
            Button("Ignore everywhere") { choose(.ignore) }
                .font(Font(tokens.uiFont(size: tokens.smallSize)))
                .foregroundStyle(tokens.swiftUIColor(\.textMuted))
                .frame(minHeight: 44, alignment: .leading)
        }
        .padding(tokens.spacing.lg)
    }
}

/// Each suggested fix as a button that applies it.
private struct FlowButtons: View {
    let replacements: [String]
    let tokens: Tokens
    let pick: (String) -> Void

    var body: some View {
        HStack(spacing: tokens.spacing.sm) {
            ForEach(replacements, id: \.self) { replacement in
                Button { pick(replacement) } label: {
                    Text(replacement.isEmpty ? "Remove it" : replacement)
                        .font(Font(tokens.textFont(size: tokens.bodySize, bold: true)))
                        .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                        .padding(.horizontal, tokens.spacing.md)
                        .frame(minHeight: 36)
                        .background(
                            RoundedRectangle(cornerRadius: tokens.spacing.radiusMd).fill(tokens.swiftUIColor(\.fill))
                        )
                }
                .buttonStyle(.plain)
            }
        }
    }
}

/// A footnote's text, shown when its number is tapped.
private struct FootnoteCard: View {
    let label: String
    let text: String
    let tokens: Tokens
    let jump: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.md) {
            Text(text)
                .font(Font(tokens.textFont(size: tokens.bodySize)))
                .foregroundStyle(tokens.swiftUIColor(\.text))
                .fixedSize(horizontal: false, vertical: true)
            Button("Go to footnote \(label)", action: jump)
                .font(Font(tokens.uiFont(size: tokens.smallSize)))
                .foregroundStyle(tokens.swiftUIColor(\.link))
                .frame(minHeight: 44, alignment: .leading)
        }
        .padding(tokens.spacing.lg)
    }
}
