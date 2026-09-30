import SwiftUI

/// How to sync a vault that doesn't yet: iCloud first and biggest, GitHub
/// under it, and the repository address and token form for people who
/// already have both.
struct SyncChooserSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL

    private var tokens: Tokens { model.library.tokens }
    private var gitHubIsAvailable: Bool { GitHubConnection.clientId != nil }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: tokens.spacing.lg) {
                    SyncChoiceCard(
                        title: "Sync with iCloud",
                        detail: "Your notes go in iCloud Drive, in a folder called Gasp. "
                            + "Your Mac and other devices pick them up from there.",
                        isMain: true, tokens: tokens, action: { model.workspace.sheet = .icloudSetup }
                    ) {
                        CloudDrawing(tokens: tokens) {
                            Image(systemName: "folder.fill")
                                .font(tokens.symbolFont(1.1))
                                .foregroundStyle(tokens.swiftUIColor(\.icon))
                        }
                        .frame(height: tokens.bodySize * 3.6)
                    }
                    SyncChoiceCard(
                        title: "Sign in with GitHub",
                        detail: gitHubIsAvailable
                            ? "Keeps every version in a private repository. Works on Windows and Linux too."
                            : GitHubSignInFlow.unavailable,
                        isMain: false, tokens: tokens, action: { model.workspace.sheet = .githubSignIn }
                    ) {
                        Image(systemName: "arrow.triangle.branch")
                            .font(tokens.symbolFont(1.2, weight: .medium))
                            .foregroundStyle(tokens.swiftUIColor(\.icon))
                    }
                    .disabled(!gitHubIsAvailable)
                    otherWays
                }
                .padding(.horizontal, WelcomeMetrics.margin)
                .padding(.vertical, tokens.spacing.lg)
            }
            .background(tokens.swiftUIColor(\.background).ignoresSafeArea())
            .navigationTitle("Sync this vault")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
        }
        .font(Font(tokens.uiFont(size: tokens.bodySize)))
        .presentationDetents([.large])
    }

    private var otherWays: some View {
        VStack(alignment: .leading, spacing: 0) {
            if gitHubIsAvailable {
                WelcomeQuietButton(title: "No GitHub account? Make one", symbol: "person.badge.plus", tokens: tokens) {
                    if let page = URL(string: githubSignUpPage()) { openURL(page) }
                }
            }
            WelcomeQuietButton(title: "Use a repository address and token", symbol: "key", tokens: tokens) {
                model.workspace.sheet = .syncSetup(SyncSetupDraft())
            }
        }
        .padding(.top, tokens.spacing.sm)
    }
}

/// One way to sync: its picture, name and what it does, on a raised
/// card. The main one stands the picture above its words and is larger;
/// the other sits in a row; a disabled one fades.
struct SyncChoiceCard<Picture: View>: View {
    let title: String
    let detail: String
    let isMain: Bool
    let tokens: Tokens
    let action: () -> Void
    @ViewBuilder let picture: () -> Picture
    @Environment(\.isEnabled) private var isEnabled

    var body: some View {
        Button(action: action) {
            layout
                .padding(isMain ? tokens.spacing.xl : tokens.spacing.lg)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(PaperSurface(radius: CGFloat(tokens.spacing.radiusLg), tokens: tokens))
                .opacity(isEnabled ? 1 : 0.6)
                .contentShape(RoundedRectangle(cornerRadius: CGFloat(tokens.spacing.radiusLg)))
        }
        .buttonStyle(CardPressStyle())
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
    }

    @ViewBuilder private var layout: some View {
        if isMain {
            VStack(alignment: .leading, spacing: tokens.spacing.lg) {
                picture()
                words
            }
        } else {
            HStack(alignment: .top, spacing: tokens.spacing.lg) {
                picture().frame(width: tokens.bodySize * 2.2)
                words
                Spacer(minLength: 0)
            }
        }
    }

    private var words: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
            Text(title)
                .font(Font(tokens.uiFont(size: tokens.bodySize * (isMain ? 1.25 : 1), bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
            Text(detail)
                .font(Font(tokens.uiFont(size: isMain ? tokens.bodySize : tokens.smallSize)))
                .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

/// Sinks a little while pressed, as a card under a finger does.
private struct CardPressStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.98 : 1)
            .animation(.easeOut(duration: 0.12), value: configuration.isPressed)
    }
}
