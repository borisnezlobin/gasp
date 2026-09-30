import SwiftUI

/// The app works like a browser: the active tab's note fills the screen,
/// a bar at the bottom names it (swipe it to move between tabs, tap it for
/// every tab), and the sidebar slides in over the note from the left edge.
struct BrowserView: View {
    @Environment(AppModel.self) private var model
    /// The hardware keyboard's shortcuts join after the first frame, as
    /// building them would hold it up.
    @State private var shortcutsReady = false

    private var tokens: Tokens { model.library.tokens }
    private var workspace: Workspace { model.workspace }

    var body: some View {
        ZStack(alignment: .bottom) {
            tokens.swiftUIColor(\.background).ignoresSafeArea()
            TabPage(tab: model.tabs.active)
                .id(pageIdentity)
            if !workspace.keyboardShown {
                TabBarView()
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
            undoPill
            NoticeView(message: workspace.notice, tokens: tokens)
        }
        .animation(.snappy(duration: 0.25), value: workspace.keyboardShown)
        .animation(.snappy(duration: 0.2), value: model.tabs.activeSession?.editState.isEditing)
        .overlay { SidebarOverlay() }
        .overlay { if workspace.overviewOpen { TabOverview().transition(.opacity) } }
        .animation(.snappy(duration: 0.25), value: workspace.overviewOpen)
        .background {
            if shortcutsReady {
                KeyCommands(
                    bindings: model.library.keyBindings,
                    commands: model.library.commands
                ) { model.runner.run($0) }
            }
        }
        .modifier(BrowserSheets())
        .modifier(BrowserPrompts())
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillShowNotification)) { _ in
            workspace.keyboardShown = true
        }
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillHideNotification)) { _ in
            workspace.keyboardShown = false
        }
        .onChange(of: model.library.configGeneration) { model.runner.refreshSessions() }
        .onOpenURL(perform: open)
        .task { await openLaunchLink() }
    }

    /// Undo and redo, over the note's right edge just above the keyboard,
    /// while the note is typed in.
    @ViewBuilder private var undoPill: some View {
        if let session = model.tabs.activeSession, session.editState.isEditing {
            UndoPill(state: session.editState, tokens: tokens) { model.runner.run($0) }
                .frame(maxWidth: .infinity, alignment: .trailing)
                .padding(.trailing, tokens.spacing.lg)
                .padding(.bottom, tokens.spacing.sm)
                .transition(.opacity.combined(with: .scale(scale: 0.9, anchor: .bottomTrailing)))
        }
    }

    /// A new identity whenever the tab or its note changes, so the page is
    /// rebuilt for it.
    private var pageIdentity: String {
        "\(model.tabs.active.id) \(model.tabs.active.path ?? "") \(model.tabs.generation)"
    }

    private func open(_ url: URL) {
        guard url.scheme == "gasp" else { return }
        let query = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        switch url.host() {
        case "open": openNote(query)
        case "sync": openSyncSetup(query)
        default: break
        }
    }

    /// `gasp://sync/setup?repository=you/notes&branch=master` opens the
    /// sync setup with those filled in. It never starts a clone itself.
    private func openSyncSetup(_ query: [URLQueryItem]) {
        var draft = SyncSetupDraft()
        draft.repository = query.first { $0.name == "repository" }?.value ?? ""
        draft.branch = query.first { $0.name == "branch" }?.value ?? draft.branch
        workspace.sheet = .syncSetup(draft)
    }

    /// `gasp://open?path=Folder/Note.md&line=12` opens a note, with the
    /// cursor on a line counted from 1.
    private func openNote(_ query: [URLQueryItem]) {
        guard let path = query.first(where: { $0.name == "path" })?.value,
              model.library.notes.contains(where: { $0.path == path }) else { return }
        model.tabs.openInNewTab(path)
        let line = query.first { $0.name == "line" }?.value.flatMap(Int.init)
        if let line, let session = model.tabs.activeSession {
            session.placeCursor(at: Self.offset(ofLine: line - 1, in: session.document.text()))
        }
    }

    /// `-open gasp://open?path=…` on the command line opens a note at
    /// launch, `-editing YES` puts the cursor in it with the keyboard up,
    /// and `-run <command>` runs a command (or `overview` shows the tabs,
    /// `sync-details` the sync sheet, `sync-chooser`, `icloud-setup`,
    /// `github-sign-in` and `icloud-details` the sheets for setting sync
    /// up and for the iCloud vault), as `xcrun simctl launch` passes
    /// them. `-syncRepository`, `-syncBranch` and `-syncToken` fill
    /// in the sync setup, and `-syncStart YES` clones straight away.
    private func openLaunchLink() async {
        let arguments = UserDefaults.standard
        PerformanceProbe.shared.start(model: model)
        shortcutsReady = true
        if let link = arguments.string(forKey: "open"), let url = URL(string: link) { open(url) }
        openSyncSetupFromArguments(arguments)
        // The note's text view joins the window a moment after launch.
        try? await Task.sleep(for: .milliseconds(600))
        if arguments.bool(forKey: "editing") { model.tabs.activeSession?.textView.becomeFirstResponder() }
        guard let command = arguments.string(forKey: "run") else { return }
        if command == "overview" {
            workspace.overviewOpen = true
        } else if let sheet = Self.launchSheets[command] {
            workspace.sheet = sheet
        } else {
            model.runner.run(command)
        }
    }

    private static let launchSheets: [String: WorkspaceSheet] = [
        "sync-details": .syncDetails,
        "sync-chooser": .syncChooser,
        "icloud-setup": .icloudSetup,
        "github-sign-in": .githubSignIn,
        "icloud-details": .icloudDetails
    ]

    private func openSyncSetupFromArguments(_ arguments: UserDefaults) {
        guard let repository = arguments.string(forKey: "syncRepository") else { return }
        var draft = SyncSetupDraft()
        draft.repository = repository
        draft.branch = arguments.string(forKey: "syncBranch") ?? draft.branch
        draft.token = arguments.string(forKey: "syncToken") ?? ""
        draft.startsAtOnce = arguments.bool(forKey: "syncStart")
        workspace.sheet = .syncSetup(draft)
    }

    private static func offset(ofLine line: Int, in text: String) -> Int {
        let lines = text.components(separatedBy: "\n").prefix(max(line, 0))
        return lines.reduce(0) { $0 + ($1 as NSString).length + 1 }
    }
}

/// One tab's page: its note, or the start page a new tab opens on.
private struct TabPage: View {
    @Environment(AppModel.self) private var model
    let tab: BrowserTab

    var body: some View {
        if let session = model.tabs.session(for: tab) {
            MarkdownEditor(session: session)
                .ignoresSafeArea(.container, edges: .bottom)
        } else if tab.path != nil {
            ContentUnavailableView(
                "Can't open this note",
                systemImage: "doc.questionmark",
                description: Text("It may have been moved or deleted.")
            )
        } else {
            StartPage()
        }
    }
}

/// A short message that fades after a moment.
private struct NoticeView: View {
    let message: String?
    let tokens: Tokens

    var body: some View {
        if let message {
            Text(message)
                .font(Font(tokens.uiFont(size: tokens.smallSize)))
                .foregroundStyle(tokens.swiftUIColor(\.background))
                .padding(.horizontal, tokens.spacing.lg)
                .padding(.vertical, tokens.spacing.md)
                .background(Capsule().fill(tokens.swiftUIColor(\.textStrong)))
                .padding(.bottom, TabBarView.clearance)
                .transition(.opacity)
                .accessibilityAddTraits(.updatesFrequently)
        }
    }
}
