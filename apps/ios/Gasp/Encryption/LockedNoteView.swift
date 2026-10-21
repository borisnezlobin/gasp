import SwiftUI

/// An encrypted note there's no key for yet, in place of the editor: the
/// whale resting over still water, the note's name, and a password field.
/// The text view never sees the note's sealed file.
struct LockedNoteView: View {
    @Environment(AppModel.self) private var model
    let path: String
    let note: LockedNote
    @State private var password = ""
    @State private var problem: String?
    @State private var unlocking = false
    @State private var shake = 0
    @State private var keptKeyWorks = true
    /// Whether the page is on screen, so an unlock that finishes after it
    /// closed changes nothing on it.
    @State private var showing = false
    @FocusState private var fieldFocused: Bool

    /// Whether a key for this note is kept behind Face ID, read once so
    /// the button doesn't pop in after the page shows.
    private let offersBiometry: Bool

    init(path: String, note: LockedNote, preferences: EncryptionPreferences) {
        self.path = path
        self.note = note
        let kept = note.keyId.map(KeptKeys.has) ?? false
        offersBiometry = preferences.keepsKeys && kept
    }

    private var tokens: Tokens { model.library.tokens }
    private var vault: VaultFolder? { model.library.vault }

    private var title: String {
        model.library.title(of: path)
    }

    var body: some View {
        ZStack {
            tokens.swiftUIColor(\.background).ignoresSafeArea()
            VStack(spacing: tokens.spacing.lg) {
                RestingWhale(tokens: tokens)
                    .padding(.bottom, tokens.spacing.md)
                heading
                passwordField
                    .modifier(Shake(amount: CGFloat(shake)))
                problemLine
                VStack(spacing: tokens.spacing.sm) {
                    WideButton(
                        title: "Unlock note", busyTitle: "Unlocking…", style: .primary, busy: unlocking,
                        tokens: tokens, action: unlock
                    )
                    .disabled(password.isEmpty || unlocking)
                    if offersBiometry { biometryButton }
                }
            }
            .frame(maxWidth: 360)
            .padding(.horizontal, tokens.spacing.xl)
            .padding(.bottom, TabBarView.clearance)
        }
        .onAppear {
            showing = true
            fieldFocused = takesFocusAtOnce
        }
        .onDisappear { showing = false }
        .onChange(of: model.workspace.overviewOpen) { _, open in if open { fieldFocused = false } }
    }

    /// The field takes the cursor when the page shows, unless Face ID can
    /// unlock instead or something sits over the page.
    private var takesFocusAtOnce: Bool {
        !offersBiometry && !model.workspace.overviewOpen && !model.workspace.sidebarOpen
    }

    private var heading: some View {
        VStack(spacing: tokens.spacing.xs) {
            Text(title)
                .font(Font(tokens.textFont(size: tokens.titleSize, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                .multilineTextAlignment(.center)
                .lineLimit(3)
            Text(note.hasOwnPassword ? "This note has its own password." : "Type the vault password to read it.")
                .font(Font(tokens.uiFont(size: tokens.smallSize)))
                .foregroundStyle(tokens.swiftUIColor(\.textMuted))
                .multilineTextAlignment(.center)
        }
    }

    private var passwordField: some View {
        SecureField("Password", text: $password)
            .textContentType(.password)
            .submitLabel(.go)
            .focused($fieldFocused)
            .onSubmit(unlock)
            .onChange(of: password) { problem = nil }
            .font(Font(tokens.uiFont(size: tokens.bodySize)))
            .foregroundStyle(tokens.swiftUIColor(\.textStrong))
            .padding(.horizontal, tokens.spacing.lg)
            .frame(minHeight: 48)
            .background(
                RoundedRectangle(cornerRadius: CGFloat(tokens.spacing.radiusMd))
                    .fill(tokens.swiftUIColor(\.fill))
            )
            .accessibilityLabel(note.hasOwnPassword ? "Note password" : "Vault password")
    }

    /// Why the last try didn't work, on a line that's always there so
    /// nothing moves when it appears.
    private var problemLine: some View {
        Text(problem ?? " ")
            .font(Font(tokens.uiFont(size: tokens.smallSize)))
            .foregroundStyle(tokens.swiftUIColor(\.conflict))
            .multilineTextAlignment(.center)
            .lineLimit(2, reservesSpace: true)
            .accessibilityHidden(problem == nil)
    }

    private var biometryButton: some View {
        WideButton(
            title: "Unlock with \(KeptKeys.biometryName)", symbol: KeptKeys.biometrySymbol,
            style: .quiet, tokens: tokens, action: unlockWithKeptKey
        )
        .disabled(!keptKeyWorks || unlocking)
    }

    private func unlock() {
        guard let vault, !password.isEmpty, !unlocking else { return }
        let (path, password) = (path, password)
        Task { @MainActor in
            unlocking = true
            let result = await outcome { try vault.unlockNote(path: path, password: password) }
            unlocking = false
            guard let result = stillWanted(result) else { return }
            switch result {
            case .success:
                keepKey()
                model.runner.noteUnlocked()
            case .failure(let error):
                showWrongPassword(error)
            }
        }
    }

    private func unlockWithKeptKey() {
        guard let vault, let id = note.keyId, !unlocking else { return }
        let path = path
        Task { @MainActor in
            unlocking = true
            defer { unlocking = false }
            guard let key = await KeptKeys.read(id: id, reason: "Unlock your encrypted notes") else { return }
            let result = await outcome { try vault.unlockNoteWithKey(path: path, key: key) }
            guard let result = stillWanted(result) else { return }
            switch result {
            case .success:
                model.runner.noteUnlocked()
            case .failure(let error):
                showKeptKeyProblem(error, id: id)
            }
        }
    }

    /// Runs an unlock off the main thread, so making the key never holds
    /// up the screen.
    private func outcome(_ work: @escaping () throws -> String) async -> Result<String, Error> {
        let locksBefore = model.tabs.locks
        let result = await Result.offMain(work)
        guard case .success = result, model.tabs.locks != locksBefore else { return result }
        return .failure(UnlockCameTooLate())
    }

    /// The result, unless the page closed or the notes locked while the
    /// unlock ran. A late unlock is undone, so the lock holds.
    private func stillWanted(_ result: Result<String, Error>) -> Result<String, Error>? {
        if case .failure(let error) = result, error is UnlockCameTooLate {
            model.runner.lockEncryptedNotes()
            return nil
        }
        return showing ? result : nil
    }

    private func showWrongPassword(_ error: Error) {
        problem = error.shownMessage
        Haptics.warning()
        withAnimation(.linear(duration: 0.3)) { shake += 1 }
    }

    private func showKeptKeyProblem(_ error: Error, id: String) {
        guard case VaultError.WrongPassword = error else {
            problem = error.shownMessage
            return
        }
        KeptKeys.forget(id: id)
        keptKeyWorks = false
        problem = "The password changed since \(KeptKeys.biometryName) was set up. Type the new one."
        fieldFocused = true
    }

    /// Keeps the key that just opened the note behind Face ID, when the
    /// vault's settings ask for it.
    private func keepKey() {
        guard EncryptionPreferences(vault: vault).keepsKeys, let key = vault?.noteKey(path: path) else { return }
        KeptKeys.keep(key)
    }
}

/// The notes locked while an unlock was making its key.
private struct UnlockCameTooLate: Error {}

/// The tour's whale, gliding slowly over a few lines of still water.
private struct RestingWhale: View {
    let tokens: Tokens
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var afloat = false

    private static let width: CGFloat = 180
    private static let aspect: CGFloat = 1000.0 / 353.0
    private static let waterLines: [(width: CGFloat, inset: CGFloat)] = [(150, 0), (96, 34), (52, -40)]

    var body: some View {
        VStack(spacing: tokens.spacing.sm) {
            Image("WhaleGlide")
                .resizable()
                .frame(width: Self.width, height: Self.width / Self.aspect)
                .offset(y: afloat ? -3 : 3)
                .animation(
                    reduceMotion ? nil : .easeInOut(duration: 3.2).repeatForever(autoreverses: true),
                    value: afloat
                )
            VStack(spacing: tokens.spacing.xs) {
                ForEach(Self.waterLines.indices, id: \.self) { index in
                    Capsule()
                        .fill(tokens.swiftUIColor(\.divider))
                        .frame(width: Self.waterLines[index].width, height: 2)
                        .offset(x: Self.waterLines[index].inset)
                }
            }
        }
        .onAppear { afloat = true }
        .accessibilityHidden(true)
    }
}

/// A sideways shake, for a password that didn't work.
private struct Shake: GeometryEffect {
    var amount: CGFloat
    var animatableData: CGFloat {
        get { amount }
        set { amount = newValue }
    }

    func effectValue(size: CGSize) -> ProjectionTransform {
        ProjectionTransform(CGAffineTransform(translationX: 6 * sin(amount * .pi * 4), y: 0))
    }
}

/// A full-width button on a capsule: the strong ink for the main action,
/// a quiet fill for the one beside it. Busy shows a spinner and the busy
/// title in place of the title without changing the button's size.
struct WideButton: View {
    enum Style {
        case primary
        case quiet
    }

    let title: String
    var busyTitle: String?
    var symbol: String?
    let style: Style
    var busy = false
    let tokens: Tokens
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            ZStack {
                label.opacity(busy ? 0 : 1).accessibilityHidden(busy)
                busyLabel.opacity(busy ? 1 : 0)
            }
            .frame(maxWidth: .infinity, minHeight: 48)
            .background(Capsule().fill(background))
            .contentShape(Capsule())
        }
        .buttonStyle(PressScaleStyle())
    }

    private var label: some View {
        Label {
            Text(title)
        } icon: {
            if let symbol { Image(systemName: symbol) }
        }
        .labelStyle(.titleAndIcon)
        .font(Font(tokens.uiFont(size: tokens.bodySize, bold: style == .primary)))
        .foregroundStyle(foreground)
    }

    private var busyLabel: some View {
        HStack(spacing: tokens.spacing.sm) {
            ProgressView().tint(foreground)
            if let busyTitle {
                Text(busyTitle)
                    .font(Font(tokens.uiFont(size: tokens.bodySize, bold: style == .primary)))
                    .foregroundStyle(foreground)
            }
        }
        .accessibilityHidden(!busy)
    }

    private var foreground: Color {
        style == .primary ? tokens.swiftUIColor(\.background) : tokens.swiftUIColor(\.textStrong)
    }

    private var background: Color {
        style == .primary ? tokens.swiftUIColor(\.textStrong) : tokens.swiftUIColor(\.fill)
    }
}

/// Presses in a touch, and dims when the button can't be used.
private struct PressScaleStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.96 : 1)
            .opacity(isEnabled ? 1 : 0.4)
            .animation(.easeOut(duration: 0.12), value: configuration.isPressed)
    }
}
