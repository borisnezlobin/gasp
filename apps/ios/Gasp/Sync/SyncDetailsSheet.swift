import SwiftUI

/// What the indicator opens: where sync stands and what to do about it,
/// what the last syncs brought in and sent, and Sync now.
struct SyncDetailsSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    private var tokens: Tokens { model.library.tokens }

    var body: some View {
        NavigationStack {
            if let overview = model.sync.overview {
                List {
                    Section { SyncStatusHeader(overview: overview, tokens: tokens) }
                    Section { actions(for: overview) }
                    if !overview.recent.isEmpty {
                        Section("Recent syncs") {
                            ForEach(Array(overview.recent.enumerated()), id: \.offset) { _, run in
                                SyncRunRow(run: run, tokens: tokens)
                            }
                        }
                    }
                    Section { SyncPlaceRows(overview: overview) }
                }
                .textCase(nil)
                .navigationTitle("Sync")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
                }
            } else {
                notSynced
            }
        }
        .font(Font(tokens.uiFont(size: tokens.bodySize)))
        .presentationDetents([.medium, .large])
    }

    /// These notes don't sync: the sample notes or a folder another app
    /// keeps in step.
    private var notSynced: some View {
        ContentUnavailableView {
            Label("These notes don't sync", systemImage: "icloud.slash")
        } description: {
            Text("Sync keeps your notes the same on every device, through iCloud or GitHub.")
        } actions: {
            if VaultLocation.syncedFolder != nil {
                Button("Open your synced notes") {
                    dismiss()
                    model.switchVault(to: .synced)
                }
                .buttonStyle(.borderedProminent)
            } else {
                Button("Set up sync") { model.workspace.sheet = .syncChooser }
                    .buttonStyle(.borderedProminent)
            }
        }
    }

    @ViewBuilder private func actions(for overview: SyncOverview) -> some View {
        if case .conflict = overview.phase {
            Button { model.workspace.sheet = .resolver } label: {
                Label("Resolve conflicts", systemImage: "arrow.triangle.merge")
            }
        }
        if overview.phase.needsSettings {
            Button { model.workspace.sheet = .settings } label: {
                Label("Open sync settings", systemImage: "gearshape")
            }
        }
        Button { model.sync.syncNow() } label: {
            HStack {
                Label(model.sync.isRunning ? "Syncing" : "Sync now", systemImage: "arrow.triangle.2.circlepath")
                Spacer()
                if model.sync.isRunning { ProgressView() }
            }
        }
        .disabled(model.sync.isRunning)
    }
}

extension SyncPhaseKind {
    /// Whether the way forward is on the settings screen: a token, or a
    /// branch that doesn't match.
    var needsSettings: Bool {
        switch self {
        case .signIn, .needsSetup: true
        default: false
        }
    }
}

/// The phase's symbol beside what the core says about it.
private struct SyncStatusHeader: View {
    let overview: SyncOverview
    let tokens: Tokens

    var body: some View {
        HStack(alignment: .top, spacing: tokens.spacing.lg) {
            SyncGlyph(phase: overview.phase, tokens: tokens, size: 26)
                .frame(width: 36)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: tokens.spacing.xs) {
                Text(overview.headline)
                    .font(Font(tokens.uiFont(size: tokens.bodySize, bold: true)))
                    .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                if let explanation = overview.explanation {
                    Text(explanation)
                        .font(Font(tokens.uiFont(size: tokens.smallSize)))
                        .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                }
            }
        }
        .padding(.vertical, tokens.spacing.xs)
        .accessibilityElement(children: .combine)
    }
}

/// One sync that changed something: when, what came in, what went out.
private struct SyncRunRow: View {
    let run: SyncRunSummary
    let tokens: Tokens

    private var when: String {
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .full
        return formatter.localizedString(for: Date(timeIntervalSinceNow: -run.secondsAgo), relativeTo: Date())
    }

    var body: some View {
        VStack(alignment: .leading, spacing: tokens.spacing.xs) {
            Text(when)
                .font(Font(tokens.uiFont(size: tokens.smallSize)))
                .foregroundStyle(tokens.swiftUIColor(\.textDetail))
            if !run.received.isEmpty {
                FileList(symbol: "arrow.down", label: "Brought in", names: run.received, tokens: tokens)
            }
            if !run.sent.isEmpty {
                FileList(symbol: "arrow.up", label: "Sent", names: run.sent, tokens: tokens)
            }
        }
    }
}

/// Notes a sync moved one way, first few by name.
private struct FileList: View {
    let symbol: String
    let label: String
    let names: [String]
    let tokens: Tokens
    private static let shown = 3

    private var text: String {
        let named = names.prefix(Self.shown).joined(separator: ", ")
        let more = names.count - Self.shown
        return more > 0 ? "\(named) and \(more) more" : named
    }

    var body: some View {
        Label {
            Text(text).foregroundStyle(tokens.swiftUIColor(\.textStrong)).lineLimit(2)
        } icon: {
            Image(systemName: symbol).foregroundStyle(tokens.swiftUIColor(\.icon))
        }
        .accessibilityLabel("\(label): \(text)")
    }
}

/// Where the notes sync: the repository and its branch.
struct SyncPlaceRows: View {
    let overview: SyncOverview

    var body: some View {
        LabeledContent("Repository", value: overview.repository ?? "None")
        LabeledContent("Branch", value: overview.branch)
    }
}
