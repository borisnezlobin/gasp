import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// The sheets over the browser: the palette, settings, templates, file
/// recovery, sharing an export, Look up, and picking an image from Photos
/// or Files.
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
            .photosPicker(isPresented: presenting(.photos), selection: $photo, matching: .images)
            .onChange(of: photo) { _, item in
                guard let item else { return }
                photo = nil
                Task { await insert(item) }
            }
            .fileImporter(
                isPresented: presenting(.imageFiles),
                allowedContentTypes: [.image],
                allowsMultipleSelection: true
            ) { result in
                insert(files: result)
            }
            .confirmationDialog("Insert image", isPresented: presenting(.imageSource)) {
                ForEach(ImageSource.allCases) { source in
                    Button(source.title) { model.workspace.sheet = source.sheet }
                }
            }
    }

    /// Every sheet but the ones that present themselves.
    private var sheetBinding: Binding<WorkspaceSheet?> {
        Binding(
            get: { model.workspace.sheet.flatMap { $0.presentsItself ? nil : $0 } },
            set: { model.workspace.sheet = $0 }
        )
    }

    /// Whether `sheet` is up. Closing it clears the workspace's sheet only
    /// while it's still this one, so a dialog's choice can open the next.
    private func presenting(_ sheet: WorkspaceSheet) -> Binding<Bool> {
        Binding(
            get: { model.workspace.sheet?.id == sheet.id },
            set: { if !$0, model.workspace.sheet?.id == sheet.id { model.workspace.sheet = nil } }
        )
    }

    @ViewBuilder private func sheetContent(_ sheet: WorkspaceSheet) -> some View {
        switch sheet {
        case .palette: CommandPalette()
        case .settings: SettingsScreen()
        case .toolbars: ToolbarSettingsSheet()
        case .templates: TemplatePicker()
        case .recovery(let path): RecoverySheet(path: path)
        case .share(let url): ShareSheet(items: [url])
        case .lookUp(let term): LookUpView(term: term)
        case .photos, .imageFiles, .imageSource: EmptyView()
        case .syncSetup(let draft): SyncSetupView(draft: draft)
        case .syncDetails: SyncDetailsSheet()
        case .resolver: ConflictResolverView()
        case .syncChooser: SyncChooserSheet()
        case .icloudSetup: ICloudSetupSheet()
        case .githubSignIn: GitHubSignInSheet()
        case .icloudDetails: ICloudDetailsSheet()
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

    private func insert(files result: Result<[URL], Error>) {
        guard case .success(let urls) = result else {
            model.workspace.tell("Couldn't open Files.")
            return
        }
        for url in urls {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            guard let data = try? Data(contentsOf: url) else {
                model.workspace.tell("Couldn't read \(url.lastPathComponent).")
                continue
            }
            let fileExtension = url.pathExtension.isEmpty ? "png" : url.pathExtension.lowercased()
            model.runner.insertImage(data, extension: fileExtension)
        }
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
            .alert("New folder", isPresented: showing(\.isNewFolder), presenting: model.workspace.prompt) { prompt in
                TextField("Folder name", text: $newName)
                Button("Make folder") {
                    if case .newFolder(let parent) = prompt { model.runner.makeFolder(named: newName, in: parent) }
                }
                Button("Cancel", role: .cancel) {}
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
        switch model.workspace.prompt {
        case .rename(let path): newName = URL(fileURLWithPath: path).deletingPathExtension().lastPathComponent
        case .newFolder: newName = ""
        default: break
        }
    }
}

extension WorkspacePrompt {
    var isRename: Bool { if case .rename = self { true } else { false } }
    var isDelete: Bool { if case .delete = self { true } else { false } }
    var isExport: Bool { if case .export = self { true } else { false } }
    var isPickVault: Bool { if case .pickVault = self { true } else { false } }
    var isNewFolder: Bool { if case .newFolder = self { true } else { false } }
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
