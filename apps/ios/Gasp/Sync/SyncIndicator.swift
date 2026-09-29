import SwiftUI

/// How each sync phase looks: its symbol and colour, as the desktop's
/// indicator draws them.
struct SyncLook {
    let symbol: String
    let color: KeyPath<Palette, ThemeColor>
    /// How many things wait, shown on the symbol.
    let count: UInt32?

    init(_ phase: SyncPhaseKind) {
        switch phase {
        case .hidden, .synced:
            self.init("checkmark.icloud", \.textFaint)
        case .syncing:
            self.init("arrow.triangle.2.circlepath", \.syncing)
        case .offline(let waiting):
            self.init("icloud.slash", \.syncing, count: waiting)
        case .needsSetup:
            self.init("gear.badge.questionmark", \.syncing)
        case .signIn:
            self.init("key", \.accent)
        case .conflict(let files):
            self.init("arrow.triangle.merge", \.conflict, count: files)
        case .failed:
            self.init("exclamationmark.icloud", \.conflict)
        }
    }

    private init(_ symbol: String, _ color: KeyPath<Palette, ThemeColor>, count: UInt32? = nil) {
        self.symbol = symbol
        self.color = color
        self.count = count.flatMap { $0 > 0 ? $0 : nil }
    }
}

/// A phase's symbol, turning while a sync runs, with a count of what's
/// waiting when something is.
struct SyncGlyph: View {
    let phase: SyncPhaseKind
    let tokens: Tokens
    var size: CGFloat = 18
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var turning = false

    private var look: SyncLook { SyncLook(phase) }
    private var spins: Bool { phase == .syncing && !reduceMotion }

    var body: some View {
        Image(systemName: look.symbol)
            .font(.system(size: size, weight: .regular))
            .foregroundStyle(tokens.swiftUIColor(look.color))
            .rotationEffect(.degrees(turning ? 360 : 0))
            .animation(turning ? .linear(duration: 1.2).repeatForever(autoreverses: false) : .default, value: turning)
            .overlay(alignment: .topTrailing) { badge }
            .onAppear { turning = spins }
            .onChange(of: spins) { _, spinning in turning = spinning }
    }

    @ViewBuilder private var badge: some View {
        if let count = look.count {
            Text("\(count)")
                .font(Font(tokens.uiFont(size: tokens.smallSize * 0.75, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.background))
                .padding(.horizontal, 4)
                .frame(minWidth: 15, minHeight: 15)
                .background(Capsule().fill(tokens.swiftUIColor(look.color)))
                .offset(x: 8, y: -6)
        }
    }
}

/// The sync indicator, at the sidebar's foot and wherever the bottom bar
/// has the `sync` widget. It shows only for the synced vault; a tap opens
/// the details and Sync now.
struct SyncIndicator: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        if let overview = model.sync.overview, overview.phase != .hidden {
            let tokens = model.library.tokens
            Button { model.workspace.sheet = .syncDetails } label: {
                SyncGlyph(phase: overview.phase, tokens: tokens)
                    .frame(width: 44, height: 44)
                    .contentShape(Circle())
            }
            .buttonStyle(PressFillStyle(tokens: tokens))
            .accessibilityLabel(overview.headline)
            .accessibilityHint("Shows sync details")
        }
    }
}
