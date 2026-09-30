import SwiftUI
import UIKit

/// The writing step: a real note to type in, with a heading, bold, tasks,
/// a link and some math, and the keyboard's bar over the keyboard. The
/// cursor starts inside the bold word, so its stars show from the start.
struct WritingStep: View {
    let tokens: Tokens
    let isShowing: Bool
    let keyboardShown: Bool
    let onward: () -> Void
    @State private var practice: PracticeNote?

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.md) {
            WelcomeHeading(text: "Write in Markdown", tokens: tokens)
                .padding(.horizontal, WelcomeMetrics.margin)
            if !keyboardShown {
                WelcomeSentence(text: "The symbols show beside your cursor and fade when it leaves.", tokens: tokens)
                    .padding(.horizontal, WelcomeMetrics.margin)
            }
            noteCard
                .padding(.horizontal, WelcomeMetrics.margin)
                .padding(.vertical, tokens.spacing.sm)
            WelcomeFooter(primary: "Continue", tokens: tokens, keyboardShown: keyboardShown, action: onward)
        }
        .onChange(of: isShowing, initial: true) { _, showing in showing ? arrive() : leave() }
    }

    private var noteCard: some View {
        ZStack {
            PaperSurface(radius: CGFloat(tokens.spacing.radiusLg), tokens: tokens)
            if let practice {
                MarkdownEditor(session: practice.session)
                    .padding(.horizontal, tokens.spacing.lg)
                    .clipShape(RoundedRectangle(cornerRadius: CGFloat(tokens.spacing.radiusLg)))
            }
        }
        .frame(maxHeight: .infinity)
    }

    /// Makes the note the first time the step shows, and puts the cursor in
    /// it once the step has slid into place.
    private func arrive() {
        if practice == nil { practice = PracticeNote(tokens: tokens) }
        guard let practice else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) {
            guard isShowing else { return }
            practice.session.textView.becomeFirstResponder()
            practice.placeCursorInBoldWord()
        }
    }

    private func leave() {
        practice?.session.textView.resignFirstResponder()
    }
}

/// The practice note: a vault of its own in the app's caches, so nothing
/// typed here reaches the user's notes, and an editing session on it that
/// runs its toolbar's commands on itself.
final class PracticeNote: EditingHost {
    static let text = """
    # Things to try

    Two stars make **bold** and one makes *italics*. Tap another line and the stars fade away.

    - [ ] Tick this box
    - [ ] Link a note with [[double brackets]]

    Math goes between dollar signs, like $e^{i\\pi} + 1 = 0$.

    """
    private static let path = "Things to try.md"
    /// Where the cursor starts: inside the bold word.
    private static let caretAfter = "**bo"

    private(set) var session: EditingController!
    private let vault: VaultFolder

    /// Nil when the caches folder can't be written.
    init?(tokens: Tokens) {
        let folder = URL.cachesDirectory.appending(path: "Practice", directoryHint: .isDirectory)
        try? FileManager.default.removeItem(at: folder)
        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try Self.text.write(to: folder.appending(path: Self.path), atomically: true, encoding: .utf8)
            vault = try VaultFolder.open(path: folder.path)
        } catch {
            return nil
        }
        session = EditingController(path: Self.path, text: Self.text, vault: vault, tokens: tokens, host: self)
        session.setReadableWidth(false)
        session.textView.backgroundColor = .clear
    }

    func placeCursorInBoldWord() {
        let text = Self.text as NSString
        let found = text.range(of: Self.caretAfter)
        guard found.location != NSNotFound else { return }
        session.placeCursor(at: found.location + found.length)
    }

    // MARK: EditingHost

    func run(_ command: String) {
        if command == "keyboard.hide" {
            session.textView.resignFirstResponder()
            return
        }
        _ = session.runNoteCommand(command)
    }

    var keyBindings: [KeyBinding] { vault.keyBindings() }

    var keyboardToolbar: PhoneToolbar { vault.keyboardToolbar() }

    func follow(link target: String, from session: EditingController) {}
}
