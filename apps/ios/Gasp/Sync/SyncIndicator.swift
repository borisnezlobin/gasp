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

    /// How the iCloud vault looks: a quiet check once everything is down,
    /// a cloud with an arrow and a count while files download, and the
    /// conflict colour with a count while iCloud's copies wait.
    init(icloud: ICloudCenter) {
        if !icloud.copies.isEmpty {
            self.init("doc.on.doc", \.conflict, count: UInt32(clamping: icloud.copies.count))
        } else if !icloud.downloading.isEmpty {
            self.init("icloud.and.arrow.down", \.syncing, count: UInt32(clamping: icloud.downloading.count))
        } else {
            self.init("checkmark.icloud", \.textFaint)
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
    let look: SyncLook
    let isTurning: Bool
    let tokens: Tokens
    var size: CGFloat = 18
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var turning = false

    init(phase: SyncPhaseKind, tokens: Tokens, size: CGFloat = 18) {
        self.init(look: SyncLook(phase), isTurning: phase == .syncing, tokens: tokens, size: size)
    }

    init(look: SyncLook, isTurning: Bool = false, tokens: Tokens, size: CGFloat = 18) {
        self.look = look
        self.isTurning = isTurning
        self.tokens = tokens
        self.size = size
    }

    private var spins: Bool { isTurning && !reduceMotion }

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
/// has the `sync` widget. It shows for the synced vault, where a tap opens
/// the details and Sync now, and for the iCloud vault, where it shows
/// iCloud's state and a tap opens its details.
struct SyncIndicator: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        if model.icloud.isAttached {
            ICloudIndicator()
        } else if let overview = model.sync.overview, overview.phase != .hidden {
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
