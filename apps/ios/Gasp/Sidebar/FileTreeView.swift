import SwiftUI
import UniformTypeIdentifiers

/// The vault's folders and notes as a tree, folders first. The note
/// showing is picked out; press and hold a note to open it in a new tab,
/// rename it, move it to another folder or move it to the trash. Press and
/// drag a note or a folder onto a folder, or onto the tree's own space for
/// the top of the vault, to move it there. The folder under the finger
/// fills in, and a closed one springs open after a moment.
///
/// Moving from the menu or the Move note command picks the note up here
/// instead: it lifts off the tree, and every folder it could go into, the
/// top of the vault first, offers "Move here".
struct FileTreeView: View {
    @Environment(AppModel.self) private var model
    let tokens: Tokens
    @State private var hovering = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let moving = model.workspace.moving {
                HStack(spacing: 0) {
                    TreeLabel(title: model.library.name, symbol: "tray.full", depth: 0, tokens: tokens, isCurrent: false)
                    MoveHere(entry: moving, folder: "", tokens: tokens)
                }
            }
            FolderContents(folder: "", depth: 0, tokens: tokens)
        }
            .padding(.bottom, tokens.bodySize * 4)
            .background(DropHighlight(isOn: hovering && model.workspace.canDrop(into: ""), tokens: tokens))
            .onDrop(
                of: [.plainText],
                delegate: TreeDropDelegate(folder: "", model: model, hovering: $hovering, expanded: .constant(true))
            )
    }
}

private struct FolderContents: View {
    @Environment(AppModel.self) private var model
    let folder: String
    let depth: Int
    let tokens: Tokens

    private var subfolders: [String] {
        model.library.folders
            .filter { ($0 as NSString).deletingLastPathComponent == folder }
            .sorted { $0.localizedStandardCompare($1) == .orderedAscending }
    }

    private var notes: [NoteSummary] {
        model.library.notes
            .filter { $0.folder == folder }
            .sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(subfolders, id: \.self) { subfolder in
                FolderRow(folder: subfolder, depth: depth, tokens: tokens)
            }
            ForEach(notes, id: \.path) { note in
                TreeNoteRow(note: note, depth: depth, tokens: tokens)
            }
        }
    }
}

private struct FolderRow: View {
    @Environment(AppModel.self) private var model
    let folder: String
    let depth: Int
    let tokens: Tokens
    @State private var expanded = false
    @State private var hovering = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 0) {
                Button { withAnimation(.snappy) { expanded.toggle() } } label: {
                    TreeLabel(
                        title: (folder as NSString).lastPathComponent,
                        symbol: expanded ? "folder.fill" : "folder",
                        depth: depth,
                        tokens: tokens,
                        isCurrent: false
                    )
                }
                .buttonStyle(.plain)
                .onDrag { model.workspace.pickUp(.folder(folder)) }
                .accessibilityValue(expanded ? "Open" : "Closed")
                if let moving = model.workspace.moving {
                    MoveHere(entry: moving, folder: folder, tokens: tokens)
                }
            }
            if expanded {
                FolderContents(folder: folder, depth: depth + 1, tokens: tokens)
            }
        }
        .background(DropHighlight(isOn: hovering && model.workspace.canDrop(into: folder), tokens: tokens))
        .onDrop(
            of: [.plainText],
            delegate: TreeDropDelegate(folder: folder, model: model, hovering: $hovering, expanded: $expanded)
        )
        .onAppear { expanded = containsActiveNote }
    }

    private var containsActiveNote: Bool {
        model.tabs.active.path?.hasPrefix(folder + "/") == true
    }
}

private struct TreeNoteRow: View {
    @Environment(AppModel.self) private var model
    let note: NoteSummary
    let depth: Int
    let tokens: Tokens

    var body: some View {
        let isCurrent = model.tabs.active.path == note.path
        let lifted = model.workspace.moving == .note(note.path)
        Button { model.runner.show(note.path) } label: {
            TreeLabel(
                title: note.title, symbol: "doc.text", depth: depth, tokens: tokens,
                isCurrent: isCurrent, lifted: lifted
            )
        }
        .buttonStyle(.plain)
        .onDrag { model.workspace.pickUp(.note(note.path)) }
        .contextMenu {
            Button("Open in new tab", systemImage: "plus.square.on.square") {
                model.runner.show(note.path, inNewTab: true)
            }
            Button("Rename", systemImage: "character.cursor.ibeam") {
                model.workspace.prompt = .rename(path: note.path)
            }
            Button("Move to folder", systemImage: "folder") {
                withAnimation(.snappy) { model.workspace.startMoving(.note(note.path)) }
            }
            Button("Move to trash", systemImage: "trash", role: .destructive) {
                model.workspace.prompt = .delete(path: note.path)
            }
        }
        .accessibilityAddTraits(isCurrent ? .isSelected : [])
    }
}

/// A row of the tree: indented by its depth, filled when it's the note
/// showing, and lifted off the tree while it's being moved.
private struct TreeLabel: View {
    let title: String
    let symbol: String
    let depth: Int
    let tokens: Tokens
    let isCurrent: Bool
    var lifted = false

    var body: some View {
        HStack(spacing: tokens.spacing.md) {
            Image(systemName: symbol)
                .font(tokens.symbolFont(0.875))
                .foregroundStyle(tokens.swiftUIColor(\.icon))
                .frame(width: 20)
            Text(title)
                .font(Font(tokens.textFont(size: tokens.bodySize, bold: isCurrent)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                .lineLimit(1)
            Spacer(minLength: 0)
        }
        .padding(.leading, CGFloat(depth) * CGFloat(tokens.spacing.xl) + CGFloat(tokens.spacing.sm))
        .frame(minHeight: 40)
        .background(
            RoundedRectangle(cornerRadius: CGFloat(tokens.spacing.radiusMd))
                .fill(backdrop)
                .shadow(color: .black.opacity(lifted ? 0.3 : 0), radius: 12, y: 4)
        )
        .scaleEffect(lifted ? 1.02 : 1)
        .animation(.snappy, value: lifted)
        .contentShape(Rectangle())
    }

    private var backdrop: Color {
        if lifted { return tokens.swiftUIColor(\.selection) }
        return isCurrent ? tokens.swiftUIColor(\.fillStrong) : .clear
    }
}

/// What a folder offers while something is being moved: "Move here" when
/// it could go in, a check where it is now, and nothing for a folder it
/// can't go into, such as itself.
private struct MoveHere: View {
    @Environment(AppModel.self) private var model
    let entry: TreeEntry
    let folder: String
    let tokens: Tokens

    var body: some View {
        if entry.canMove(into: folder) {
            Button {
                withAnimation(.snappy) { model.runner.finishMoving(entry, into: folder) }
            } label: {
                Text("Move here")
                    .font(Font(tokens.uiFont(size: tokens.smallSize, bold: true)))
                    .foregroundStyle(tokens.swiftUIColor(\.textStrong))
                    .padding(.horizontal, tokens.spacing.md)
                    .frame(minHeight: 32)
                    .background(Capsule().fill(tokens.swiftUIColor(\.fillStrong)))
                    .frame(minHeight: 44)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Move here, into \(folder.isEmpty ? model.library.name : (folder as NSString).lastPathComponent)")
        } else if folder == entry.parent {
            Label("Here now", systemImage: "checkmark")
                .labelStyle(.titleAndIcon)
                .font(Font(tokens.uiFont(size: tokens.smallSize)))
                .foregroundStyle(tokens.swiftUIColor(\.textDetail))
                .padding(.horizontal, tokens.spacing.md)
        }
    }
}

/// The fill behind a folder, or the whole tree, while a drop would land
/// in it.
private struct DropHighlight: View {
    let isOn: Bool
    let tokens: Tokens

    var body: some View {
        RoundedRectangle(cornerRadius: CGFloat(tokens.spacing.radiusMd))
            .fill(tokens.swiftUIColor(\.selection))
            .opacity(isOn ? 1 : 0)
            .animation(.easeOut(duration: 0.15), value: isOn)
    }
}

/// Drops on one folder of the tree, `""` for the top of the vault. A drop
/// that would move nothing is refused and leaves the folder unlit.
private struct TreeDropDelegate: DropDelegate {
    let folder: String
    let model: AppModel
    @Binding var hovering: Bool
    @Binding var expanded: Bool

    /// How long a closed folder is hovered over before it opens.
    private static let springDelay: Duration = .milliseconds(700)

    private var entry: TreeEntry? { model.workspace.treeDrag }
    private var accepts: Bool { model.workspace.canDrop(into: folder) }

    func validateDrop(info: DropInfo) -> Bool {
        entry != nil
    }

    func dropEntered(info: DropInfo) {
        hovering = true
        if accepts { Haptics.target() }
        springOpenSoon()
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: accepts ? .move : .forbidden)
    }

    func dropExited(info: DropInfo) {
        hovering = false
    }

    func performDrop(info: DropInfo) -> Bool {
        hovering = false
        guard let entry, accepts else { return false }
        model.workspace.treeDrag = nil
        model.runner.move(entry, into: folder)
        Haptics.dropped()
        withAnimation(.snappy) { expanded = true }
        return true
    }

    private func springOpenSoon() {
        guard !expanded else { return }
        let opening = $expanded
        let stillHovering = $hovering
        Task { @MainActor in
            try? await Task.sleep(for: Self.springDelay)
            guard stillHovering.wrappedValue, !opening.wrappedValue else { return }
            Haptics.target()
            withAnimation(.snappy) { opening.wrappedValue = true }
        }
    }
}

extension Workspace {
    /// Remembers what the drag carries and answers its item for the drag.
    func pickUp(_ entry: TreeEntry) -> NSItemProvider {
        treeDrag = entry
        moving = nil
        Haptics.pickedUp()
        return NSItemProvider(object: entry.path as NSString)
    }

    func canDrop(into folder: String) -> Bool {
        treeDrag?.canMove(into: folder) == true
    }
}

/// The taps under the finger as a note or folder is picked up, lands
/// over a folder and is dropped.
enum Haptics {
    static func pickedUp() {
        UIImpactFeedbackGenerator(style: .medium).impactOccurred()
    }

    static func target() {
        UISelectionFeedbackGenerator().selectionChanged()
    }

    static func dropped() {
        UINotificationFeedbackGenerator().notificationOccurred(.success)
    }
}
