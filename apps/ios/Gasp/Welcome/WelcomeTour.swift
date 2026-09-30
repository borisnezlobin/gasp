import SwiftUI
import UniformTypeIdentifiers

/// The welcome tour, the first thing a new install shows: hello, writing
/// in Markdown, choosing where notes live, and how sync works. Each step
/// fills the screen; swipe between them or take the button at the bottom.
/// Until a vault is chosen the tour stops at choosing one. Replayed from
/// Settings, it can be closed at any step.
struct WelcomeTour: View {
    @Environment(AppModel.self) private var model
    @State private var step: WelcomeStep = .hello
    @State private var pickingFolder = false
    @State private var settingUpSync = false
    @State private var keyboardShown = false
    @State private var problem: String?

    private var tokens: Tokens { model.library.tokens }
    private var welcome: WelcomeFlow { model.welcome }

    /// The steps that can be reached: the sync steps once there's a vault.
    private var steps: [WelcomeStep] {
        model.library.vault == nil ? [.hello, .writing, .vault] : WelcomeStep.allCases
    }

    var body: some View {
        VStack(spacing: 0) {
            topBar
            TabView(selection: $step) {
                ForEach(steps) { step in
                    page(step).tag(step)
                }
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
            if !keyboardShown {
                SeaBand(tide: step.tide, tokens: tokens)
                    .opacity(step == .hello ? 0 : 1)
                    .animation(.easeOut(duration: 0.4), value: step == .hello)
            }
        }
        .background(tokens.swiftUIColor(\.background).ignoresSafeArea())
        .fileImporter(isPresented: $pickingFolder, allowedContentTypes: [.folder], onCompletion: openPicked)
        .sheet(isPresented: $settingUpSync, onDismiss: finishIfSynced) {
            SyncSetupView(draft: SyncSetupDraft()).environment(model)
        }
        .alert("Couldn't make the vault", isPresented: showingProblem, presenting: problem) { _ in
            Button("OK", role: .cancel) {}
        } message: { Text($0) }
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillShowNotification)) { _ in
            keyboardShown = true
        }
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillHideNotification)) { _ in
            keyboardShown = false
        }
        .onAppear { step = steps.contains(welcome.firstStep) ? welcome.firstStep : .hello }
    }

    @ViewBuilder private func page(_ shown: WelcomeStep) -> some View {
        switch shown {
        case .hello:
            HelloStep(tokens: tokens) { go(to: .writing) }
        case .writing:
            WritingStep(tokens: tokens, isShowing: step == .writing, keyboardShown: keyboardShown) { go(to: .vault) }
        case .vault:
            VaultStep(tokens: tokens, makeNew: makeNewVault, openFolder: { pickingFolder = true }, trySample: openSample)
        case .syncHow:
            SyncHowStep(tokens: tokens, onward: { go(to: .syncSetUp) }, notNow: welcome.finish)
        case .syncSetUp:
            SyncSetUpStep(tokens: tokens, setUp: { settingUpSync = true }, notNow: welcome.finish)
        }
    }

    /// Skip on the first launch's early steps, which goes to choosing a
    /// vault, and Close on a replay.
    private var topBar: some View {
        HStack {
            Spacer()
            if !welcome.isFirstRun {
                topButton("Close", action: welcome.finish)
            } else if step.rawValue < WelcomeStep.vault.rawValue {
                topButton("Skip") { go(to: .vault) }
            }
        }
        .frame(height: 44)
        .padding(.horizontal, WelcomeMetrics.margin - 8)
    }

    private func topButton(_ title: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(Font(tokens.uiFont(size: tokens.bodySize)))
                .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                .padding(.horizontal, 8)
                .frame(minHeight: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func go(to next: WelcomeStep) {
        withAnimation(.snappy(duration: 0.35)) { step = next }
    }

    private func makeNewVault() {
        do {
            try model.openNewVault()
            go(to: .syncHow)
        } catch {
            problem = error.shownMessage
        }
    }

    private func openSample() {
        do {
            try model.openSampleVault()
            welcome.finish()
        } catch {
            problem = error.shownMessage
        }
    }

    private func openPicked(_ result: Result<URL, Error>) {
        guard case .success(let folder) = result else { return }
        model.switchVault(to: folder)
        go(to: .syncHow)
    }

    private func finishIfSynced() {
        if model.library.kind == .synced { welcome.finish() }
    }

    private var showingProblem: Binding<Bool> {
        Binding(get: { problem != nil }, set: { if !$0 { problem = nil } })
    }
}
