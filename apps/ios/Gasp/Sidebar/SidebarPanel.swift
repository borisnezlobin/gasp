import SwiftUI

/// The sidebar's contents: a search across the vault, and below it the
/// file tree, the note's outline, its links or the vault's tags.
struct SidebarPanel: View {
    @Environment(AppModel.self) private var model
    @FocusState private var searchFocused: Bool

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        @Bindable var workspace = model.workspace
        VStack(alignment: .leading, spacing: tokens.spacing.lg) {
            SearchField(query: $workspace.searchQuery, prompt: "Search notes", tokens: tokens)
                .focused($searchFocused)
            if workspace.searchQuery.isEmpty {
                SectionPicker(selection: $workspace.sidebarSection, tokens: tokens)
                ScrollView { section.padding(.bottom, tokens.spacing.xl) }
            } else {
                ScrollView {
                    SearchResults(query: workspace.searchQuery, tokens: tokens) { path, offset in
                        model.runner.show(path, at: offset)
                    }
                }
            }
            SidebarActions(tokens: tokens)
        }
        .padding(.horizontal, tokens.spacing.lg)
        .padding(.top, tokens.spacing.md)
        .onChange(of: workspace.searchFocusRequest, initial: true) { _, request in
            if request > 0 { searchFocused = true }
        }
    }

    @ViewBuilder private var section: some View {
        switch model.workspace.sidebarSection {
        case .files: FileTreeView(tokens: tokens)
        case .outline: OutlineList(tokens: tokens)
        case .links: LinksList(tokens: tokens)
        case .tags: TagsList(tokens: tokens)
        }
    }
}

/// The four views of the sidebar as icons with their names.
private struct SectionPicker: View {
    @Binding var selection: SidebarSection
    let tokens: Tokens

    var body: some View {
        HStack(spacing: tokens.spacing.xs) {
            ForEach(SidebarSection.allCases) { section in
                let selected = section == selection
                Button { selection = section } label: {
                    VStack(spacing: 2) {
                        Image(systemName: section.symbol)
                            .font(.system(size: 16))
                        Text(section.rawValue)
                            .font(Font(tokens.uiFont(size: tokens.smallSize * 0.85, bold: selected)))
                    }
                    .foregroundStyle(tokens.swiftUIColor(selected ? \.textStrong : \.textDetail))
                    .frame(maxWidth: .infinity, minHeight: 48)
                    .background(
                        RoundedRectangle(cornerRadius: CGFloat(tokens.spacing.radiusMd))
                            .fill(selected ? tokens.swiftUIColor(\.fillStrong) : .clear)
                    )
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(selected ? .isSelected : [])
            }
        }
    }
}

/// New note, today's note and settings, at the sidebar's foot.
private struct SidebarActions: View {
    @Environment(AppModel.self) private var model
    let tokens: Tokens

    var body: some View {
        HStack(spacing: tokens.spacing.md) {
            ActionChip(title: "New note", symbol: "square.and.pencil", tokens: tokens) { run("note.new") }
            ActionChip(title: "Today", symbol: "calendar", tokens: tokens) { run("daily.open") }
            Spacer()
            BarButton(symbol: "gearshape", label: "Settings", tokens: tokens) { run("settings.open") }
        }
        .padding(.bottom, tokens.spacing.sm)
    }

    private func run(_ command: String) {
        model.workspace.sidebarOpen = false
        model.runner.run(command)
    }
}
