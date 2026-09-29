import SwiftUI

/// Every command in the shared registry, searchable by name or category,
/// with its hardware-keyboard key beside it. Picking one closes the
/// palette and runs it on the note that was showing.
struct CommandPalette: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @FocusState private var searching: Bool

    private var tokens: Tokens { model.library.tokens }

    private var shown: [CommandInfo] {
        let listed = model.library.commands.filter(\.inPalette)
        let words = query.lowercased().split(separator: " ")
        guard !words.isEmpty else { return listed }
        return listed.filter { command in
            let haystack = "\(command.title) \(command.category)".lowercased()
            return words.allSatisfy { haystack.contains($0) }
        }
    }

    private var groups: [(category: String, commands: [CommandInfo])] {
        var order: [String] = []
        var byCategory: [String: [CommandInfo]] = [:]
        for command in shown {
            if byCategory[command.category] == nil { order.append(command.category) }
            byCategory[command.category, default: []].append(command)
        }
        return order.map { ($0, byCategory[$0] ?? []) }
    }

    var body: some View {
        VStack(spacing: tokens.spacing.md) {
            SearchField(query: $query, prompt: "Find a command", tokens: tokens)
                .focused($searching)
                .onSubmit(runFirst)
                .padding(.horizontal, tokens.spacing.xl)
                .padding(.top, tokens.spacing.xl)
            List {
                ForEach(groups, id: \.category) { group in
                    Section(group.category) {
                        ForEach(group.commands, id: \.id) { command in
                            row(command)
                        }
                    }
                }
            }
            .listStyle(.plain)
            .font(Font(tokens.uiFont(size: tokens.smallSize, bold: true)))
            .overlay {
                if shown.isEmpty { ContentUnavailableView.search(text: query) }
            }
        }
        .background(tokens.swiftUIColor(\.background))
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .onAppear { searching = true }
        .onKeyPress(.escape) {
            dismiss()
            return .handled
        }
    }

    private func row(_ command: CommandInfo) -> some View {
        Button { run(command.id) } label: {
            HStack(spacing: tokens.spacing.md) {
                Image(systemName: CommandSymbols.name(for: command.id))
                    .foregroundStyle(tokens.swiftUIColor(\.icon))
                    .frame(width: 24)
                Text(command.title)
                    .font(Font(tokens.uiFont(size: tokens.bodySize)))
                    .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                Spacer()
                if let shortcut = command.shortcut {
                    Text(shortcut)
                        .font(Font(tokens.uiFont(size: tokens.smallSize)))
                        .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                }
            }
            .frame(minHeight: 36)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .listRowBackground(tokens.swiftUIColor(\.background))
    }

    private func runFirst() {
        if let first = shown.first { run(first.id) }
    }

    private func run(_ id: String) {
        dismiss()
        let runner = model.runner
        DispatchQueue.main.async { runner.run(id) }
    }
}
