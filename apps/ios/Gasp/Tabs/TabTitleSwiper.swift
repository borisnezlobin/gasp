import SwiftUI

/// The note showing, in the middle of the bottom bar, on a track of every
/// tab's title. Dragging it sideways moves the track with the finger, so
/// the neighbouring tab's title slides in, and letting go past a third of
/// the way (or with a flick) settles on that tab with a spring. Past the
/// last tab waits a new tab on the start page, as in Safari; before the
/// first there's nothing, so the title only gives a little and springs
/// back. A tap shows every tab.
struct TabTitleSwiper: View {
    @Environment(AppModel.self) private var model
    /// How far the finger has moved, unresisted. `-tabBarDrag <points>` at
    /// launch starts it there, to look at a drag in a screenshot.
    @State private var drag = CGFloat(UserDefaults.standard.double(forKey: "tabBarDrag"))
    @State private var dragging = false
    /// Whether letting go now would move to the neighbouring tab, so the
    /// finger feels the moment it becomes true or stops being.
    @State private var willSwitch = false

    private var tokens: Tokens { model.library.tokens }
    private var tabs: TabStore { model.tabs }
    private static let settling = Animation.spring(response: 0.36, dampingFraction: 0.86)

    /// Whether a new tab waits past the last one; not when the last tab is
    /// already a new tab.
    private var newTabWaits: Bool {
        tabs.tabs.last?.path != nil
    }

    private var hasPrevious: Bool { tabs.activeIndex > 0 }
    private var hasNext: Bool { tabs.activeIndex < tabs.tabs.count - 1 || newTabWaits }

    var body: some View {
        GeometryReader { geometry in
            let width = geometry.size.width
            track(width: width)
                .offset(x: position(width: width))
                .animation(dragging ? nil : Self.settling, value: position(width: width))
                .frame(width: width, height: geometry.size.height, alignment: .leading)
                .contentShape(Rectangle())
                .onTapGesture { model.workspace.overviewOpen = true }
                .gesture(swipe(width: width))
        }
        .frame(maxWidth: .infinity, minHeight: 44, maxHeight: 44)
        .clipped()
        .mask(edgeFade)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(tabs.title(of: tabs.active))
        .accessibilityHint("Swipe up or down to move between tabs. Double-tap to see them all.")
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { model.workspace.overviewOpen = true }
        .accessibilityAdjustableAction { direction in
            switch direction {
            case .increment: settle(on: .next)
            case .decrement: settle(on: .previous)
            @unknown default: break
            }
        }
    }

    private func track(width: CGFloat) -> some View {
        HStack(spacing: 0) {
            ForEach(tabs.tabs) { tab in
                TabTitle(title: tabs.title(of: tab), folder: folder(of: tab), tokens: tokens)
                    .frame(width: width)
            }
            if newTabWaits {
                TabTitle(title: "New tab", folder: nil, tokens: tokens)
                    .frame(width: width)
            }
        }
    }

    /// Where the track sits: the active tab's title in view, moved by the
    /// drag, which only gives a little where there's no tab to move to.
    private func position(width: CGFloat) -> CGFloat {
        let blocked = (drag > 0 && !hasPrevious) || (drag < 0 && !hasNext)
        let moved = blocked ? Self.rubberBand(drag, limit: width / 5) : drag
        return -CGFloat(tabs.activeIndex) * width + moved
    }

    /// Movement that gets harder the further it goes, never past `limit`.
    private static func rubberBand(_ distance: CGFloat, limit: CGFloat) -> CGFloat {
        let stretched = limit * (1 - 1 / (abs(distance) * 0.55 / max(limit, 1) + 1))
        return distance < 0 ? -stretched : stretched
    }

    /// The edges fade so a title sliding out goes under the buttons.
    private var edgeFade: some View {
        HStack(spacing: 0) {
            LinearGradient(colors: [.clear, .black], startPoint: .leading, endPoint: .trailing)
                .frame(width: tokens.spacing.md)
            Rectangle().fill(.black)
            LinearGradient(colors: [.black, .clear], startPoint: .leading, endPoint: .trailing)
                .frame(width: tokens.spacing.md)
        }
    }

    private enum Direction { case previous, next }

    private func swipe(width: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: 12)
            .onChanged { value in
                dragging = true
                drag = value.translation.width
                noticeSwitchPoint(width: width)
            }
            .onEnded { value in
                dragging = false
                let passed = value.translation.width
                let flung = value.predictedEndTranslation.width
                if (passed < -width / 3 || flung < -width / 2) && hasNext {
                    settle(on: .next)
                } else if (passed > width / 3 || flung > width / 2) && hasPrevious {
                    settle(on: .previous)
                }
                drag = 0
                willSwitch = false
            }
    }

    /// Ticks as the drag passes a third of the way towards a tab it can
    /// move to, and again if it comes back.
    private func noticeSwitchPoint(width: CGFloat) {
        let open = drag < 0 ? hasNext : hasPrevious
        let passed = open && abs(drag) > width / 3
        guard passed != willSwitch else { return }
        willSwitch = passed
        Haptics.tick()
    }

    /// Moves to the neighbouring tab, making the new tab when it's the one
    /// past the last.
    private func settle(on direction: Direction) {
        switch direction {
        case .previous where hasPrevious:
            tabs.select(tabs.activeIndex - 1)
        case .next where tabs.activeIndex < tabs.tabs.count - 1:
            tabs.select(tabs.activeIndex + 1)
        case .next where newTabWaits:
            Haptics.limit()
            tabs.newTab()
        default:
            break
        }
    }

    private func folder(of tab: BrowserTab) -> String? {
        guard let path = tab.path else { return nil }
        let folder = (path as NSString).deletingLastPathComponent
        return folder.isEmpty ? nil : folder
    }
}

/// A tab's title in the bottom bar, with its folder under it.
private struct TabTitle: View {
    let title: String
    let folder: String?
    let tokens: Tokens

    var body: some View {
        VStack(spacing: 0) {
            Text(title)
                .font(Font(tokens.textFont(size: tokens.bodySize, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
            if let folder {
                Text(folder)
                    .font(Font(tokens.uiFont(size: tokens.smallSize * 0.85)))
                    .foregroundStyle(tokens.swiftUIColor(\.textDetail))
            }
        }
        .lineLimit(1)
        .padding(.horizontal, tokens.spacing.sm)
    }
}
