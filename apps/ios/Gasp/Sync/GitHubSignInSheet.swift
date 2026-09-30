import SwiftUI
import UIKit

/// Signing in with GitHub: the short code to approve on GitHub, then the
/// person's repositories (or a new private one), then setting up and what
/// it did. Closing the sheet stops asking GitHub.
struct GitHubSignInSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var flow = GitHubSignInFlow()
    @State private var attempt = 0

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        NavigationStack {
            stage
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(tokens.swiftUIColor(\.background).ignoresSafeArea())
                .navigationTitle("Sign in with GitHub")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        if flow.finishedFolder == nil {
                            Button("Cancel") { dismiss() }.disabled(isWorking)
                        }
                    }
                }
        }
        .font(Font(tokens.uiFont(size: tokens.bodySize)))
        .interactiveDismissDisabled(isWorking)
        .task(id: attempt) { await flow.signIn() }
    }

    @ViewBuilder private var stage: some View {
        switch flow.stage {
        case .asking:
            SetupProgress(text: "Asking GitHub for a code", tokens: tokens)
        case .code(let userCode, let page):
            SignInCodeStage(userCode: userCode, page: page, tokens: tokens)
        case .failed(let message):
            SetupFailure(message: message, tokens: tokens) { attempt += 1 }
        case .repositories(let account, let token):
            RepositoryChoiceStage(account: account, tokens: tokens) { repository in
                run { await flow.use(repository, token: token) }
            } makeNew: {
                let own = model.library.isThisPhonesOwn ? model.library.folder : nil
                model.tabs.saveAllNotes()
                run { await flow.makeRepository(account: account, token: token, ownVault: own) }
            }
        case .working(let text):
            SetupProgress(text: text, tokens: tokens)
        case .done(let finished):
            SetupDone(sentence: finished.sentence, detail: waitingLine(finished.summary), tokens: tokens) { dismiss() }
        }
    }

    private var isWorking: Bool {
        if case .working = flow.stage { return true }
        return false
    }

    private func run(_ work: @escaping () async -> Void) {
        Task {
            await work()
            if let folder = flow.finishedFolder { model.openSyncedVault(at: folder) }
        }
    }

    private func waitingLine(_ summary: InPlaceSetupSummary) -> String? {
        guard summary.waiting > 0 else { return nil }
        let notes = summary.waiting == 1 ? "1 note differs" : "\(summary.waiting) notes differ"
        return "\(notes) between this iPhone and GitHub. The sync indicator opens them side by side."
    }
}

/// The code to type on GitHub, big, and the button that copies it and
/// opens GitHub's page for it.
private struct SignInCodeStage: View {
    let userCode: String
    let page: URL?
    let tokens: Tokens
    @Environment(\.openURL) private var openURL
    @State private var copied = false

    var body: some View {
        VStack(spacing: tokens.spacing.xl) {
            Spacer(minLength: 0)
            Text(userCode)
                .font(Font(tokens.codeFont(size: tokens.bodySize * 2.4, bold: true)))
                .monospacedDigit()
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                .textSelection(.enabled)
                .padding(.vertical, tokens.spacing.xl)
                .frame(maxWidth: .infinity)
                .background(
                    RoundedRectangle(cornerRadius: CGFloat(tokens.spacing.radiusLg))
                        .fill(tokens.swiftUIColor(\.fill))
                )
                .overlay(alignment: .topTrailing) { copiedMark }
                .accessibilityLabel("Code \(userCode.map(String.init).joined(separator: " "))")
            Spacer(minLength: 0)
        }
        .padding(.horizontal, WelcomeMetrics.margin)
        .safeAreaInset(edge: .bottom) {
            WelcomeFooter(primary: "Copy code and open GitHub", tokens: tokens, action: copyAndOpen) {
                HStack(spacing: tokens.spacing.md) {
                    ProgressView()
                    Text("Waiting for you to approve Gasp on GitHub")
                        .font(Font(tokens.uiFont(size: tokens.smallSize)))
                        .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                }
                .accessibilityElement(children: .combine)
            }
            .padding(.bottom, tokens.spacing.lg)
        }
    }

    /// A check once the code is on the clipboard, in a corner kept for it.
    private var copiedMark: some View {
        Image(systemName: "checkmark.circle.fill")
            .font(tokens.symbolFont(1.1))
            .foregroundStyle(tokens.swiftUIColor(\.accent))
            .padding(tokens.spacing.md)
            .opacity(copied ? 1 : 0)
            .animation(.easeOut(duration: 0.2), value: copied)
            .accessibilityHidden(!copied)
            .accessibilityLabel("Copied")
    }

    private func copyAndOpen() {
        UIPasteboard.general.string = userCode
        copied = true
        Haptics.tap()
        if let page { openURL(page) }
    }
}

/// Who signed in, a new private repository as the main choice, and the
/// repositories they already have.
private struct RepositoryChoiceStage: View {
    let account: GitHubAccount
    let tokens: Tokens
    let use: (GitHubRepository) -> Void
    let makeNew: () -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: tokens.spacing.xl) {
                VStack(alignment: .leading, spacing: tokens.spacing.md) {
                    Text("Signed in as \(account.login).")
                        .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                    WelcomePrimaryButton(
                        title: "Make a private repository called \(githubNotesRepositoryName(known: account.repositories))",
                        tokens: tokens, action: makeNew
                    )
                    Text("Gasp keeps your GitHub sign-in in this iPhone's Keychain.")
                        .font(Font(tokens.uiFont(size: tokens.smallSize)))
                        .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                }
                if !account.repositories.isEmpty { existing }
            }
            .padding(WelcomeMetrics.margin)
        }
        #if DEBUG
        .task {
            guard FakeGitHub.makesRepositoryAtOnce else { return }
            try? await Task.sleep(for: .seconds(2))
            makeNew()
        }
        #endif
    }

    private var existing: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.sm) {
            Text("Or use one you already have")
                .font(Font(tokens.uiFont(size: tokens.bodySize, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                .accessibilityAddTraits(.isHeader)
            ForEach(account.repositories, id: \.fullName) { repository in
                RepositoryRow(repository: repository, tokens: tokens) { use(repository) }
                Divider().overlay(tokens.swiftUIColor(\.divider))
            }
        }
    }
}

private struct RepositoryRow: View {
    let repository: GitHubRepository
    let tokens: Tokens
    let use: () -> Void

    var body: some View {
        Button(action: use) {
            HStack(spacing: tokens.spacing.md) {
                Text(repository.name)
                    .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                    .lineLimit(1)
                if repository.private {
                    Image(systemName: "lock.fill")
                        .font(tokens.symbolFont(0.75))
                        .foregroundStyle(tokens.swiftUIColor(\.icon))
                        .accessibilityLabel("Private")
                }
                Spacer()
                Image(systemName: "chevron.right")
                    .font(tokens.symbolFont(0.75, weight: .semibold))
                    .foregroundStyle(tokens.swiftUIColor(\.textFaint))
            }
            .frame(minHeight: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityHint("Syncs this vault with it")
    }
}

/// A spinner and what's happening, in the middle of a setup sheet.
struct SetupProgress: View {
    let text: String
    let tokens: Tokens

    var body: some View {
        VStack(spacing: tokens.spacing.lg) {
            ProgressView().controlSize(.large)
            Text(text)
                .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                .multilineTextAlignment(.center)
        }
        .padding(WelcomeMetrics.margin)
        .accessibilityElement(children: .combine)
    }
}

/// Why a setup step didn't work, and a way to try it again.
struct SetupFailure: View {
    let message: String
    let tokens: Tokens
    let retry: () -> Void

    var body: some View {
        VStack(alignment: .leading) {
            ProblemText(message: message, tokens: tokens)
            Spacer(minLength: 0)
        }
        .padding(WelcomeMetrics.margin)
        .safeAreaInset(edge: .bottom) {
            WelcomeFooter(primary: "Try again", tokens: tokens, action: retry)
                .padding(.bottom, tokens.spacing.lg)
        }
    }
}

/// The end of a setup: a check, what happened, and Done.
struct SetupDone: View {
    let sentence: String
    var detail: String?
    let tokens: Tokens
    let done: () -> Void

    var body: some View {
        VStack(spacing: tokens.spacing.lg) {
            Spacer(minLength: 0)
            Image(systemName: "checkmark.circle.fill")
                .font(tokens.symbolFont(2.6))
                .foregroundStyle(tokens.swiftUIColor(\.accent))
                .accessibilityHidden(true)
            Text(sentence)
                .font(Font(tokens.uiFont(size: tokens.bodySize * 1.1, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                .multilineTextAlignment(.center)
            if let detail {
                WelcomeSentence(text: detail, tokens: tokens).multilineTextAlignment(.center)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, WelcomeMetrics.margin)
        .safeAreaInset(edge: .bottom) {
            WelcomeFooter(primary: "Done", tokens: tokens, action: done)
                .padding(.bottom, tokens.spacing.lg)
        }
    }
}
