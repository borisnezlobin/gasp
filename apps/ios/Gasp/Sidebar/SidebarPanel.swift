import SwiftUI

/// The sidebar's contents: a search across the vault, and below it the
/// file tree, the note's outline, its links or the vault's tags. While a
/// note is being moved it's the file tree alone, under the note's name,
/// with a way to make a new folder for it or to stop.
struct SidebarPanel: View {
    @Environment(AppModel.self) private var model
    @FocusState private var searchFocused: Bool

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        @Bindable var workspace = model.workspace
        VStack(alignment: .leading, spacing: tokens.spacing.lg) {
            if let moving = workspace.moving {
                MovingHeading(entry: moving, tokens: tokens)
                ScrollView { FileTreeView(tokens: tokens).padding(.bottom, tokens.spacing.xl) }
                MovingActions(entry: moving, tokens: tokens)
            } else {
                browsing
            }
        }
        .padding(.horizontal, tokens.spacing.lg)
        .padding(.top, tokens.spacing.md)
        .onChange(of: workspace.searchFocusRequest, initial: true) { _, request in
            if request > 0 { searchFocused = true }
        }
    }

    @ViewBuilder private var browsing: some View {
        @Bindable var workspace = model.workspace
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

    @ViewBuilder private var section: some View {
        switch model.workspace.sidebarSection {
        case .files: FileTreeView(tokens: tokens)
        case .outline: OutlineList(tokens: tokens)
        case .links: LinksList(tokens: tokens)
        case .tags: TagsList(tokens: tokens)
        }
    }
}

/// What's being moved, named where the search field usually is.
private struct MovingHeading: View {
    let entry: TreeEntry
    let tokens: Tokens

    private var name: String {
        let last = (entry.path as NSString).lastPathComponent
        guard case .note = entry else { return last }
        return (last as NSString).deletingPathExtension
    }

    var body: some View {
        HStack(spacing: tokens.spacing.md) {
            Image(systemName: "arrow.up.and.down.and.arrow.left.and.right")
                .font(tokens.symbolFont(0.875))
                .foregroundStyle(tokens.swiftUIColor(\.icon))
            Text("Move “\(name)”")
                .font(Font(tokens.uiFont(size: tokens.bodySize * 1.15, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                .lineLimit(1)
        }
        .frame(minHeight: 44)
        .accessibilityAddTraits(.isHeader)
    }
}

/// New folder, which makes one at the top of the vault and moves the note
/// into it, and Cancel, at the sidebar's foot while a note is moving.
private struct MovingActions: View {
    @Environment(AppModel.self) private var model
    let entry: TreeEntry
    let tokens: Tokens

    var body: some View {
        HStack(spacing: tokens.spacing.md) {
            ActionChip(title: "New folder", symbol: "folder.badge.plus", tokens: tokens) {
                model.workspace.prompt = .newFolder(moving: entry)
            }
            Spacer()
            Button("Cancel") {
                withAnimation(.snappy) { model.workspace.moving = nil }
            }
            .font(Font(tokens.uiFont(size: tokens.bodySize, bold: true)))
            .foregroundStyle(tokens.swiftUIColor(\.textStrong))
            .frame(minHeight: 44)
        }
        .padding(.bottom, tokens.spacing.sm)
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
                            .font(tokens.symbolFont())
                            .frame(height: tokens.bodySize * 1.25)
                        Text(section.rawValue)
                            .font(Font(tokens.uiFont(size: tokens.smallSize * 0.85, bold: selected)))
                    }
                    .foregroundStyle(tokens.swiftUIColor(selected ? \.textStrong : \.textDetail))
                    .padding(.vertical, tokens.spacing.sm)
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

/// New note, today's note, sync's state (for the synced vault) and
/// settings, at the sidebar's foot.
private struct SidebarActions: View {
    @Environment(AppModel.self) private var model
    let tokens: Tokens

    var body: some View {
        HStack(spacing: tokens.spacing.md) {
            ActionChip(title: "New note", symbol: "square.and.pencil", tokens: tokens) { run("note.new") }
            ActionChip(title: "Today", symbol: "calendar", tokens: tokens) { run("daily.open") }
            Spacer()
            SyncIndicator()
            BarButton(symbol: "gearshape", label: "Settings", tokens: tokens) { run("settings.open") }
        }
        .padding(.bottom, tokens.spacing.sm)
    }

    private func run(_ command: String) {
        model.workspace.sidebarOpen = false
        model.runner.run(command)
    }
}
