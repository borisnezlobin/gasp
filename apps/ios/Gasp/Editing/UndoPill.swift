import SwiftUI
import UIKit

/// Whether a note is being typed in, and whether its edits can be undone
/// or redone now, kept up to date from its text view's undo manager.
@Observable
final class EditState {
    var isEditing = false
    private(set) var canUndo = false
    private(set) var canRedo = false
    @ObservationIgnored private weak var undoManager: UndoManager?
    @ObservationIgnored private var observers: [NSObjectProtocol] = []

    private static let changes: [Notification.Name] = [
        .NSUndoManagerDidCloseUndoGroup,
        .NSUndoManagerDidUndoChange,
        .NSUndoManagerDidRedoChange,
        .NSUndoManagerCheckpoint
    ]

    deinit {
        observers.forEach(NotificationCenter.default.removeObserver)
    }

    /// Follows `manager`, the text view's, from now on.
    func follow(_ manager: UndoManager?) {
        guard manager !== undoManager else {
            refresh()
            return
        }
        observers.forEach(NotificationCenter.default.removeObserver)
        undoManager = manager
        observers = Self.changes.map { name in
            NotificationCenter.default.addObserver(forName: name, object: manager, queue: .main) { [weak self] _ in
                self?.refresh()
            }
        }
        refresh()
    }

    func refresh() {
        let undo = undoManager?.canUndo ?? false
        let redo = undoManager?.canRedo ?? false
        if undo != canUndo { canUndo = undo }
        if redo != canRedo { canRedo = redo }
    }
}

/// Undo and redo in a small pill that floats at the right edge just above
/// the keyboard's bar while a note is edited. A button with nothing to do
/// greys out and stops taking taps.
struct UndoPill: View {
    let state: EditState
    let tokens: Tokens
    let run: (String) -> Void

    /// Room the pill takes above the keyboard's bar, so the cursor can be
    /// scrolled clear of it.
    static let clearance: CGFloat = 52
    private static let buttonSide: CGFloat = 40

    var body: some View {
        HStack(spacing: 0) {
            button("arrow.uturn.backward", label: "Undo", enabled: state.canUndo) { run("edit.undo") }
            button("arrow.uturn.forward", label: "Redo", enabled: state.canRedo) { run("edit.redo") }
        }
        .padding(.horizontal, tokens.spacing.xs)
        .background(
            Capsule()
                .fill(tokens.swiftUIColor(\.popover))
                .padding(1)
                .background(Capsule().fill(tokens.swiftUIColor(\.ring)))
                .shadow(color: tokens.swiftUIColor(\.shadow), radius: 10, y: 3)
        )
        .accessibilityElement(children: .contain)
    }

    private func button(_ symbol: String, label: String, enabled: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(tokens.symbolFont(weight: .medium))
                .foregroundStyle(tokens.swiftUIColor(enabled ? \.icon : \.iconDisabled))
                .frame(width: Self.buttonSide, height: Self.buttonSide)
                .contentShape(Circle())
        }
        .buttonStyle(PressFillStyle(tokens: tokens))
        .disabled(!enabled)
        .accessibilityLabel(label)
        .animation(.easeOut(duration: 0.15), value: enabled)
    }
}
