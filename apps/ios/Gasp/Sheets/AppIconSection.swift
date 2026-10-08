import SwiftUI
import UIKit

/// The two whales the Home Screen can show. Each has a dark version, which
/// iOS shows by itself in dark mode.
enum AppIconChoice: String, CaseIterable, Identifiable {
    case breaching
    case upClose

    var id: String { rawValue }

    /// The alternate icon's name in the asset catalog; nil is the main icon.
    var alternateName: String? {
        switch self {
        case .breaching: nil
        case .upClose: "AppIconHead"
        }
    }

    var preview: String {
        switch self {
        case .breaching: "AppIconPreview"
        case .upClose: "AppIconHeadPreview"
        }
    }

    var label: String {
        switch self {
        case .breaching: "Breaching"
        case .upClose: "Up close"
        }
    }

    static var current: AppIconChoice {
        allCases.first { $0.alternateName == UIApplication.shared.alternateIconName } ?? .breaching
    }
}

/// Picks the app icon, from tiles that look like the icons themselves.
struct AppIconSection: View {
    @Environment(AppModel.self) private var model
    @State private var chosen = AppIconChoice.current

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        if UIApplication.shared.supportsAlternateIcons {
            Section("App icon") {
                HStack(spacing: CGFloat(tokens.spacing.xl)) {
                    ForEach(AppIconChoice.allCases) { choice in
                        AppIconTile(choice: choice, isChosen: choice == chosen, tokens: tokens) { choose(choice) }
                    }
                    Spacer(minLength: 0)
                }
                .padding(.vertical, CGFloat(tokens.spacing.sm))
            }
        }
    }

    private func choose(_ choice: AppIconChoice) {
        guard choice != chosen else { return }
        let previous = chosen
        chosen = choice
        UIApplication.shared.setAlternateIconName(choice.alternateName) { error in
            guard error != nil else { return }
            DispatchQueue.main.async { chosen = previous }
        }
    }
}

/// One icon as a tile: the icon's own shape, with a ring and a check
/// around the chosen one. The ring's room is kept for every tile, so
/// choosing moves nothing.
private struct AppIconTile: View {
    let choice: AppIconChoice
    let isChosen: Bool
    let tokens: Tokens
    let action: () -> Void

    private static let side: CGFloat = 64
    private static let ringWidth: CGFloat = 2.5
    private static let ringGap: CGFloat = 3
    /// iOS draws icons with corners about 22% of their side.
    private static let cornerShare: CGFloat = 0.2237

    private var iconRadius: CGFloat { Self.side * Self.cornerShare }
    private var ringRadius: CGFloat { iconRadius + Self.ringGap + Self.ringWidth / 2 }

    var body: some View {
        Button(action: action) {
            VStack(spacing: CGFloat(tokens.spacing.sm)) {
                icon
                Text(choice.label)
                    .font(Font(tokens.uiFont(size: tokens.smallSize)))
                    .foregroundStyle(tokens.swiftUIColor(isChosen ? \.textStrong : \.textDetail))
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(choice.label) icon")
        .accessibilityAddTraits(isChosen ? [.isSelected] : [])
    }

    private var icon: some View {
        Image(choice.preview)
            .resizable()
            .frame(width: Self.side, height: Self.side)
            .clipShape(RoundedRectangle(cornerRadius: iconRadius, style: .continuous))
            .padding(Self.ringGap + Self.ringWidth)
            .overlay {
                RoundedRectangle(cornerRadius: ringRadius, style: .continuous)
                    .inset(by: Self.ringWidth / 2)
                    .stroke(tokens.swiftUIColor(\.textStrong), lineWidth: Self.ringWidth)
                    .opacity(isChosen ? 1 : 0)
            }
            .overlay(alignment: .bottomTrailing) { check }
    }

    private var check: some View {
        Image(systemName: "checkmark.circle.fill")
            .font(.system(size: 20, weight: .semibold))
            .symbolRenderingMode(.palette)
            .foregroundStyle(tokens.swiftUIColor(\.background), tokens.swiftUIColor(\.textStrong))
            .opacity(isChosen ? 1 : 0)
    }
}
