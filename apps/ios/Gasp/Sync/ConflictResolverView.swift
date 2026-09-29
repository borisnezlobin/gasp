import SwiftUI

/// The notes waiting for a person. One note opens straight away; several
/// list first. Each is settled on its own and syncs as soon as it's saved.
struct ConflictResolverView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var path: [String] = []

    private var tokens: Tokens { model.library.tokens }
    private var conflicts: [ConflictNote] { model.sync.conflicts }

    var body: some View {
        NavigationStack(path: $path) {
            content
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } }
                }
        }
        .font(Font(tokens.uiFont(size: tokens.bodySize)))
        .onChange(of: conflicts.map(\.path)) { _, waiting in
            path.removeAll { !waiting.contains($0) }
        }
    }

    @ViewBuilder private var content: some View {
        if conflicts.isEmpty {
            ContentUnavailableView(
                "Every note is settled",
                systemImage: "checkmark.circle",
                description: Text("Sync has sent your choices to your other devices.")
            )
        } else if conflicts.count == 1, let note = conflicts.first {
            ConflictNoteView(note: note)
        } else {
            List(conflicts, id: \.path) { note in
                NavigationLink(value: note.path) {
                    NoteConflictRow(note: note, tokens: tokens)
                }
            }
            .navigationTitle("Notes to settle")
            .navigationDestination(for: String.self) { path in
                if let note = conflicts.first(where: { $0.path == path }) {
                    ConflictNoteView(note: note)
                }
            }
        }
    }
}

private struct NoteConflictRow: View {
    let note: ConflictNote
    let tokens: Tokens

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
            Text(note.title)
                .font(Font(tokens.textFont(size: tokens.bodySize, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
            Text(note.places.count == 1 ? "1 place" : "\(note.places.count) places")
                .font(Font(tokens.uiFont(size: tokens.smallSize)))
                .foregroundStyle(tokens.swiftUIColor(\.textDetail))
        }
    }
}

/// What to keep in one place, as the person picked it.
enum PlaceDecision: Equatable {
    case thisDevice
    case otherDevice
    case both
    case edited(String)

    var choice: PlaceChoice {
        switch self {
        case .thisDevice: .thisDevice
        case .otherDevice: .otherDevice
        case .both: .both
        case .edited(let text): .edited(text: text)
        }
    }

    /// Whether it keeps this device's lines, and the other device's.
    var keeps: (thisDevice: Bool, otherDevice: Bool) {
        switch self {
        case .thisDevice: (true, false)
        case .otherDevice: (false, true)
        case .both: (true, true)
        case .edited: (false, false)
        }
    }

    var isEdited: Bool {
        if case .edited = self { true } else { false }
    }
}

/// One note's places, each with both versions and a choice, and Save.
struct ConflictNoteView: View {
    @Environment(AppModel.self) private var model
    let note: ConflictNote
    @State private var decisions: [PlaceDecision?]
    @State private var saving = false
    @State private var problem: String?

    init(note: ConflictNote) {
        self.note = note
        _decisions = State(initialValue: Array(repeating: nil, count: note.places.count))
    }

    private var tokens: Tokens { model.library.tokens }
    private var decided: Int { decisions.compactMap { $0 }.count }
    private var canSave: Bool { decided == note.places.count && !saving }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: tokens.spacing.xl) {
                Text(introduction)
                    .font(Font(tokens.uiFont(size: tokens.smallSize)))
                    .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                ForEach(note.places.indices, id: \.self) { index in
                    PlaceCard(
                        number: index + 1, count: note.places.count, place: note.places[index],
                        decision: $decisions[index], tokens: tokens
                    )
                }
                if let problem { ProblemText(message: problem, tokens: tokens) }
                saveButton
            }
            .padding(tokens.spacing.xl)
        }
        .background(tokens.swiftUIColor(\.background))
        .scrollDismissesKeyboard(.interactively)
        .navigationTitle(note.title)
        .task { await decideFromLaunchArguments() }
    }

    private var introduction: String {
        let places = note.places.count == 1 ? "one place" : "\(note.places.count) places"
        return "This iPhone and another device changed \(places) in this note. "
            + "Pick what to keep, then save. Your other notes keep syncing meanwhile."
    }

    private var saveButton: some View {
        Button(action: save) {
            HStack(spacing: tokens.spacing.md) {
                if saving { ProgressView().tint(tokens.swiftUIColor(\.onAccent)) }
                Text(saveLabel).font(Font(tokens.uiFont(size: tokens.bodySize, bold: true)))
            }
            .foregroundStyle(tokens.swiftUIColor(canSave ? \.onAccent : \.textDetail))
            .frame(maxWidth: .infinity, minHeight: 48)
            .background(
                Capsule().fill(tokens.swiftUIColor(canSave ? \.accent : \.fill))
            )
        }
        .buttonStyle(.plain)
        .disabled(!canSave)
    }

    private var saveLabel: String {
        if saving { return "Saving" }
        let left = note.places.count - decided
        return left == 0 ? "Save and sync" : "Pick \(left) more"
    }

    private func save() {
        let choices = decisions.compactMap { $0?.choice }
        guard canSave, choices.count == note.places.count else { return }
        saving = true
        problem = nil
        Task { @MainActor in
            do {
                try await model.sync.resolve(note, choices: choices)
            } catch {
                problem = error.shownMessage
            }
            saving = false
        }
    }

    /// `-resolverChoice theirs` (or `mine`, `both`, `edit:<text>`) picks
    /// that in every place and saves, so tests can drive the resolver.
    private func decideFromLaunchArguments() async {
        guard let argument = UserDefaults.standard.string(forKey: "resolverChoice"),
              let decision = PlaceDecision(launchArgument: argument) else { return }
        try? await Task.sleep(for: .seconds(1))
        decisions = Array(repeating: decision, count: note.places.count)
        try? await Task.sleep(for: .seconds(2))
        save()
    }
}

extension PlaceDecision {
    init?(launchArgument: String) {
        switch launchArgument {
        case "mine": self = .thisDevice
        case "theirs": self = .otherDevice
        case "both": self = .both
        default:
            guard launchArgument.hasPrefix("edit:") else { return nil }
            self = .edited(String(launchArgument.dropFirst("edit:".count)))
        }
    }
}

/// One place: the lines above it, both versions, and what to keep.
private struct PlaceCard: View {
    let number: Int
    let count: Int
    let place: ConflictPlace
    @Binding var decision: PlaceDecision?
    let tokens: Tokens

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.md) {
            if count > 1 {
                Text("Place \(number) of \(count)")
                    .font(Font(tokens.uiFont(size: tokens.smallSize, bold: true)))
                    .foregroundStyle(tokens.swiftUIColor(\.textDetail))
            }
            if !place.context.isEmpty {
                Text(place.context)
                    .font(Font(tokens.textFont(size: tokens.smallSize)))
                    .foregroundStyle(tokens.swiftUIColor(\.textFaint))
                    .lineLimit(2)
                    .accessibilityLabel("Above it: \(place.context)")
            }
            VersionBlock(
                side: "This iPhone", text: place.thisDevice, color: \.thisDevice,
                kept: decision.map { $0.keeps.thisDevice }, tokens: tokens
            )
            VersionBlock(
                side: "Other device", text: place.otherDevice, color: \.otherDevice,
                kept: decision.map { $0.keeps.otherDevice }, tokens: tokens
            )
            ChoiceGrid(place: place, decision: $decision, tokens: tokens)
            if case .edited(let text) = decision {
                EditedText(text: text, tokens: tokens) { decision = .edited($0) }
            }
        }
        .padding(tokens.spacing.lg)
        .background(
            RoundedRectangle(cornerRadius: CGFloat(tokens.spacing.radiusLg)).fill(tokens.swiftUIColor(\.card))
        )
    }
}

/// One device's lines. Once a choice is made, a kept version gets a
/// check and a stronger fill, and one left out fades.
private struct VersionBlock: View {
    let side: String
    let text: String
    let color: KeyPath<Palette, ThemeColor>
    /// Nil until a choice is made.
    let kept: Bool?
    let tokens: Tokens

    private var symbol: String {
        switch kept {
        case .some(true): "checkmark.circle.fill"
        case .some(false): "circle.dashed"
        case .none: "circle"
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
            Label(side, systemImage: symbol)
                .font(Font(tokens.uiFont(size: tokens.smallSize, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(color))
            Text(text.isEmpty ? "Nothing here" : text.trimmingCharacters(in: .newlines))
                .font(Font(tokens.textFont(size: tokens.bodySize, italic: text.isEmpty)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                .strikethrough(kept == false, color: tokens.swiftUIColor(\.textFaint))
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(tokens.spacing.md)
        .background(
            RoundedRectangle(cornerRadius: CGFloat(tokens.spacing.radiusMd))
                .fill(tokens.swiftUIColor(color).opacity(kept == true ? 0.18 : 0.07))
        )
        .opacity(kept == false ? 0.5 : 1)
        .animation(.snappy(duration: 0.2), value: kept)
        .accessibilityElement(children: .combine)
        .accessibilityValue(kept.map { $0 ? "Kept" : "Left out" } ?? "Not decided")
    }
}

/// The four ways to settle a place.
private struct ChoiceGrid: View {
    let place: ConflictPlace
    @Binding var decision: PlaceDecision?
    let tokens: Tokens

    var body: some View {
        Grid(horizontalSpacing: tokens.spacing.sm, verticalSpacing: tokens.spacing.sm) {
            GridRow {
                option("Keep this iPhone's", symbol: "iphone", .thisDevice)
                option("Keep the other's", symbol: "laptopcomputer", .otherDevice)
            }
            GridRow {
                option("Keep both", symbol: "square.stack", .both)
                editOption
            }
        }
    }

    private func option(_ title: String, symbol: String, _ value: PlaceDecision) -> some View {
        ChoiceButton(title: title, symbol: symbol, selected: decision == value, tokens: tokens) {
            decision = value
        }
    }

    private var editOption: some View {
        ChoiceButton(title: "Edit", symbol: "pencil", selected: decision?.isEdited == true, tokens: tokens) {
            guard decision?.isEdited != true else { return }
            decision = .edited(Self.bothTexts(place))
        }
    }

    /// Where an edit starts: this iPhone's lines, then the other device's.
    private static func bothTexts(_ place: ConflictPlace) -> String {
        [place.thisDevice, place.otherDevice]
            .map { $0.trimmingCharacters(in: .newlines) }
            .filter { !$0.isEmpty }
            .joined(separator: "\n")
    }
}

private struct ChoiceButton: View {
    let title: String
    let symbol: String
    let selected: Bool
    let tokens: Tokens
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label(title, systemImage: selected ? "checkmark" : symbol)
                .font(Font(tokens.uiFont(size: tokens.smallSize, bold: selected)))
                .foregroundStyle(tokens.swiftUIColor(selected ? \.background : \.textStrong))
                .lineLimit(1)
                .minimumScaleFactor(0.8)
                .frame(maxWidth: .infinity, minHeight: 44)
                .background(Capsule().fill(tokens.swiftUIColor(selected ? \.textStrong : \.fill)))
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

/// The place's text as the person writes it.
private struct EditedText: View {
    let text: String
    let tokens: Tokens
    let changed: (String) -> Void
    @State private var draft = ""

    var body: some View {
        TextEditor(text: $draft)
            .font(Font(tokens.textFont(size: tokens.bodySize)))
            .scrollContentBackground(.hidden)
            .frame(minHeight: 100)
            .padding(tokens.spacing.sm)
            .background(
                RoundedRectangle(cornerRadius: CGFloat(tokens.spacing.radiusMd)).fill(tokens.swiftUIColor(\.fill))
            )
            .accessibilityLabel("Text to keep here")
            .onAppear { draft = text }
            .onChange(of: draft) { _, new in if new != text { changed(new) } }
    }
}
