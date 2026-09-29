import SwiftUI

/// The bar at the bottom: the `browser-bar` toolbar in toolbars.toml, its
/// buttons either side of the note showing, which sits where the toolbar
/// has its spacer. Swipe the title sideways for the neighbouring tab, tap
/// it for every tab. By default it's the sidebar on the left and the tab
/// overview on the right.
struct TabBarView: View {
    @Environment(AppModel.self) private var model

    /// Room the bar takes at the bottom of the screen.
    static let clearance: CGFloat = 84

    private var tokens: Tokens { model.library.tokens }

    /// The bar's items before the note's title, and after it.
    private var sides: (leading: [ToolbarEntry], trailing: [ToolbarEntry]) {
        let entries = model.library.browserBar.entries
        guard let title = entries.firstIndex(of: .spacer) else { return (entries, []) }
        let trailing = entries[(title + 1)...].filter { $0 != .spacer }
        return (Array(entries[..<title]), trailing)
    }

    var body: some View {
        let sides = sides
        HStack(spacing: tokens.spacing.sm) {
            ForEach(Array(sides.leading.enumerated()), id: \.offset) { _, entry in
                BrowserBarItem(entry: entry, tokens: tokens)
            }
            TabTitleSwiper()
            ForEach(Array(sides.trailing.enumerated()), id: \.offset) { _, entry in
                BrowserBarItem(entry: entry, tokens: tokens)
            }
        }
        .padding(tokens.spacing.sm)
        .background(
            Capsule()
                .fill(tokens.swiftUIColor(\.popover))
                .padding(1)
                .background(Capsule().fill(tokens.swiftUIColor(\.ring)))
                .shadow(color: tokens.swiftUIColor(\.shadow), radius: 12, y: 4)
        )
        .padding(.horizontal, tokens.spacing.xl)
        .padding(.bottom, tokens.spacing.sm)
    }
}

/// One of the bottom bar's items: a command's button, the tab overview
/// with how many tabs are open, a menu, a line, or sync's indicator.
private struct BrowserBarItem: View {
    @Environment(AppModel.self) private var model
    let entry: ToolbarEntry
    let tokens: Tokens

    private static let overview = "tab.overview"

    var body: some View {
        switch entry {
        case .command(let command) where command.id == Self.overview:
            overviewButton(command)
        case .command(let command):
            BarButton(symbol: CommandSymbols.name(for: command.id), label: command.title, tokens: tokens) {
                model.runner.run(command.id)
            }
        case .menu(let title, let commands):
            menu(title: title, commands: commands)
        case .separator:
            Capsule()
                .fill(tokens.swiftUIColor(\.divider))
                .frame(width: 1, height: 22)
        case .widget(let name, _) where name == "sync":
            SyncIndicator()
        case .widget, .spacer:
            EmptyView()
        }
    }

    private func overviewButton(_ command: CommandInfo) -> some View {
        BarButton(symbol: CommandSymbols.name(for: command.id), label: command.title, tokens: tokens) {
            model.runner.run(command.id)
        }
        .overlay(alignment: .center) {
            Text("\(model.tabs.tabs.count)")
                .font(Font(tokens.uiFont(size: tokens.smallSize * 0.85, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.icon))
                .offset(x: 2, y: 2)
                .allowsHitTesting(false)
        }
    }

    private func menu(title: String, commands: [CommandInfo]) -> some View {
        Menu {
            ForEach(commands, id: \.id) { command in
                Button { model.runner.run(command.id) } label: {
                    Label(command.title, systemImage: CommandSymbols.name(for: command.id))
                }
            }
        } label: {
            Image(systemName: "ellipsis.circle")
                .font(tokens.symbolFont(1.125))
                .foregroundStyle(tokens.swiftUIColor(\.icon))
                .frame(width: 44, height: 44)
                .contentShape(Circle())
        }
        .accessibilityLabel(title)
    }
}

/// A round icon button in the bottom bar.
struct BarButton: View {
    let symbol: String
    let label: String
    let tokens: Tokens
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(tokens.symbolFont(1.125))
                .foregroundStyle(tokens.swiftUIColor(\.icon))
                .frame(width: 44, height: 44)
                .contentShape(Circle())
        }
        .buttonStyle(PressFillStyle(tokens: tokens))
        .accessibilityLabel(label)
    }
}

/// A fill that shows while a button is held.
struct PressFillStyle: ButtonStyle {
    let tokens: Tokens

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(
                Circle().fill(configuration.isPressed ? tokens.swiftUIColor(\.fillStrong) : .clear)
            )
    }
}
