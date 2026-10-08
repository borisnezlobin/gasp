import SwiftUI

/// The iCloud vault's state in the sync indicator's place.
struct ICloudIndicator: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let tokens = model.library.tokens
        Button { model.workspace.sheet = .icloudDetails } label: {
            SyncGlyph(look: SyncLook(icloud: model.icloud), tokens: tokens)
                .frame(width: 44, height: 44)
                .contentShape(Circle())
        }
        .buttonStyle(PressFillStyle(tokens: tokens))
        .accessibilityLabel(model.icloud.headline)
        .accessibilityHint("Shows where your notes are in iCloud")
    }
}

extension ICloudCenter {
    /// Where the iCloud vault stands, in a few words.
    var headline: String {
        if !copies.isEmpty { return Self.counted(copies.count, "note has a copy", "notes have copies") }
        if !downloading.isEmpty { return "Downloading " + Self.counted(downloading.count, "file", "files") }
        return "Up to date"
    }

    /// `iCloud Drive › Gasp`.
    var place: String {
        "iCloud Drive › " + (folder.map(ICloudContainer.shownName(of:)) ?? icloudFolderName())
    }

    static func counted(_ count: Int, _ one: String, _ many: String) -> String {
        "\(count) \(count == 1 ? one : many)"
    }
}

/// What the indicator opens for the iCloud vault: where it is, what's
/// still downloading, and each copy iCloud left with a way to open both
/// notes and to move the copy to the trash.
struct ICloudDetailsSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    private var tokens: Tokens { model.library.tokens }
    private var icloud: ICloudCenter { model.icloud }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ICloudStatusHeader(tokens: tokens)
                    LabeledContent("Where", value: icloud.place)
                }
                if !icloud.copies.isEmpty {
                    Section {
                        ForEach(icloud.copies, id: \.copy) { pair in
                            ICloudCopyRows(pair: pair, tokens: tokens, open: open)
                        }
                    } header: {
                        Text("Changed on two devices at once")
                    } footer: {
                        Text("iCloud kept both versions. Keep what you need in the note, then move the copy to the Trash.")
                    }
                }
                if !icloud.downloading.isEmpty {
                    Section("Downloading") {
                        ForEach(icloud.downloading.prefix(Self.shownDownloads), id: \.self) { path in
                            Label(path, systemImage: "arrow.down.circle")
                                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                                .lineLimit(1)
                                .truncationMode(.middle)
                        }
                        if icloud.downloading.count > Self.shownDownloads {
                            Text("and \(icloud.downloading.count - Self.shownDownloads) more")
                                .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                        }
                    }
                }
            }
            .textCase(nil)
            .navigationTitle("iCloud")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
            }
        }
        .font(Font(tokens.uiFont(size: tokens.bodySize)))
        .presentationDetents([.medium, .large])
        .onAppear { icloud.rescan() }
    }

    private static let shownDownloads = 12

    private func open(_ path: String) {
        dismiss()
        model.runner.show(path)
    }
}

/// The glyph beside where things stand.
private struct ICloudStatusHeader: View {
    @Environment(AppModel.self) private var model
    let tokens: Tokens

    var body: some View {
        HStack(spacing: tokens.spacing.lg) {
            SyncGlyph(look: SyncLook(icloud: model.icloud), tokens: tokens, size: 26)
                .frame(width: 36)
                .accessibilityHidden(true)
            Text(model.icloud.headline)
                .font(Font(tokens.uiFont(size: tokens.bodySize, bold: true)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
        }
        .padding(.vertical, tokens.spacing.xs)
    }
}

/// A note and the copy iCloud left beside it: open either, or move the
/// copy to the trash.
private struct ICloudCopyRows: View {
    @Environment(AppModel.self) private var model
    let pair: ICloudCopyPair
    let tokens: Tokens
    let open: (String) -> Void

    var body: some View {
        noteRow(pair.original, symbol: "doc.text")
        noteRow(pair.copy, symbol: "doc.on.doc")
        Button(role: .destructive) {
            model.runner.delete(pair.copy)
            model.icloud.rescan()
        } label: {
            Label("Move the copy to the Trash", systemImage: "trash")
        }
    }

    private func noteRow(_ path: String, symbol: String) -> some View {
        HStack {
            Label {
                Text(Self.name(of: path))
                    .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                    .lineLimit(2)
            } icon: {
                Image(systemName: symbol).foregroundStyle(tokens.swiftUIColor(\.icon))
            }
            Spacer()
            Button("Open") { open(path) }
                .buttonStyle(.bordered)
                .buttonBorderShape(.capsule)
                .accessibilityLabel("Open \(Self.name(of: path))")
        }
    }

    private static func name(of path: String) -> String {
        URL(fileURLWithPath: path).deletingPathExtension().lastPathComponent
    }
}
