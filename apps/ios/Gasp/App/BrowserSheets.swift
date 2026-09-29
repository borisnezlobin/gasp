import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// The sheets over the browser: the palette, settings, templates, file
/// recovery, sharing an export, Look up, and picking a photo.
struct BrowserSheets: ViewModifier {
    @Environment(AppModel.self) private var model
    @State private var photo: PhotosPickerItem?

    func body(content: Content) -> some View {
        @Bindable var workspace = model.workspace
        content
            .sheet(item: sheetBinding) { sheet in
                sheetContent(sheet)
                    .environment(model)
            }
            .photosPicker(isPresented: photosBinding, selection: $photo, matching: .images)
            .onChange(of: photo) { _, item in
                guard let item else { return }
                photo = nil
                Task { await insert(item) }
            }
    }

    /// Every sheet but the photo picker, which is presented on its own.
    private var sheetBinding: Binding<WorkspaceSheet?> {
        Binding(
            get: {
                if case .photos = model.workspace.sheet { return nil }
                return model.workspace.sheet
            },
            set: { model.workspace.sheet = $0 }
        )
    }

    private var photosBinding: Binding<Bool> {
        Binding(
            get: { if case .photos = model.workspace.sheet { true } else { false } },
            set: { if !$0 { model.workspace.sheet = nil } }
        )
    }

    @ViewBuilder private func sheetContent(_ sheet: WorkspaceSheet) -> some View {
        switch sheet {
        case .palette: CommandPalette()
        case .settings: SettingsScreen()
        case .toolbars: ToolbarSettingsSheet()
        case .templates: TemplatePicker()
        case .recovery(let path): RecoverySheet(path: path)
        case .moveNote(let path): MoveNoteSheet(path: path)
        case .share(let url): ShareSheet(items: [url])
        case .lookUp(let term): LookUpView(term: term)
        case .photos: EmptyView()
        case .syncSetup(let draft): SyncSetupView(draft: draft)
        case .syncDetails: SyncDetailsSheet()
        case .resolver: ConflictResolverView()
        }
    }

    private func insert(_ item: PhotosPickerItem) async {
        guard let data = try? await item.loadTransferable(type: Data.self) else {
            model.workspace.tell("Couldn't read that photo.")
            return
        }
        let type = item.supportedContentTypes.first { $0.conforms(to: .image) }
        model.runner.insertImage(data, extension: type?.preferredFilenameExtension ?? "jpg")
    }
}

/// The questions the browser asks before acting: a new name, whether to
/// move a note to the trash, which format to export, and which folder to
/// open as the vault.
struct BrowserPrompts: ViewModifier {
    @Environment(AppModel.self) private var model
    @State private var newName = ""

    func body(content: Content) -> some View {
        content
            .alert("Rename note", isPresented: showing(\.isRename), presenting: model.workspace.prompt) { prompt in
                TextField("Name", text: $newName)
                Button("Rename") { if case .rename(let path) = prompt { model.runner.rename(path, to: newName) } }
                Button("Cancel", role: .cancel) {}
            }
            .confirmationDialog(
                "Move this note to the trash?", isPresented: showing(\.isDelete),
                titleVisibility: .visible, presenting: model.workspace.prompt
            ) { prompt in
                Button("Move to trash", role: .destructive) {
                    if case .delete(let path) = prompt { model.runner.delete(path) }
                }
            }
            .confirmationDialog(
                "Export as", isPresented: showing(\.isExport), presenting: model.workspace.prompt
            ) { prompt in
                ForEach(ExportFormat.allCases, id: \.title) { format in
                    Button(format.title) {
                        if case .export(let path) = prompt { model.runner.export(path, as: format) }
                    }
                }
            }
            .fileImporter(isPresented: showing(\.isPickVault), allowedContentTypes: [.folder]) { result in
                if case .success(let folder) = result { model.switchVault(to: folder) }
            }
            .onChange(of: model.workspace.prompt?.id) { prefillName() }
    }

    private func showing(_ kind: KeyPath<WorkspacePrompt, Bool>) -> Binding<Bool> {
        Binding(
            get: { model.workspace.prompt?[keyPath: kind] ?? false },
            set: { if !$0 { model.workspace.prompt = nil } }
        )
    }

    private func prefillName() {
        guard case .rename(let path) = model.workspace.prompt else { return }
        newName = URL(fileURLWithPath: path).deletingPathExtension().lastPathComponent
    }
}

extension WorkspacePrompt {
    var isRename: Bool { if case .rename = self { true } else { false } }
    var isDelete: Bool { if case .delete = self { true } else { false } }
    var isExport: Bool { if case .export = self { true } else { false } }
    var isPickVault: Bool { if case .pickVault = self { true } else { false } }
}

/// The system share sheet, for an exported file.
struct ShareSheet: UIViewControllerRepresentable {
    let items: [Any]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }

    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}

/// The system dictionary's entry for a word.
struct LookUpView: UIViewControllerRepresentable {
    let term: String

    func makeUIViewController(context: Context) -> UIReferenceLibraryViewController {
        UIReferenceLibraryViewController(term: term)
    }

    func updateUIViewController(_ controller: UIReferenceLibraryViewController, context: Context) {}
}
