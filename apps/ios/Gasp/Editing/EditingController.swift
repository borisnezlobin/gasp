import UIKit

/// What an editing session asks of the browser around it.
protocol EditingHost: AnyObject {
    /// Runs a command from the registry, as the toolbar's buttons do.
    func run(_ command: String)
    /// The keymap's keys with a hardware keyboard.
    var keyBindings: [KeyBinding] { get }
    /// Opens where a tapped link goes.
    func follow(link target: String, from session: EditingController)
    /// The bar above the software keyboard, from toolbars.toml.
    var keyboardToolbar: PhoneToolbar { get }
}

/// One open note: keeps the core's copy in step with the text view,
/// restyles it from each new plan, saves edits and keeps snapshots of the
/// text it replaces.
final class EditingController: NSObject, UITextViewDelegate {
    let textView = EditorTextView(usingTextLayoutManager: true)
    var path: String {
        didSet {
            saver.path = path
            media.notePath = path
        }
    }
    let document: NoteDocument
    let vault: VaultFolder
    private(set) var tokens: Tokens
    private(set) var styler: PlanStyler
    let media: NoteMedia
    /// The bar above the software keyboard, kept while it's turned off so
    /// turning it back on brings it back.
    private var accessoryBar: AccessoryBar?
    /// Whether the note is being typed in and undo and redo can run, for
    /// the pill above the keyboard.
    let editState = EditState()
    var prose = ProseMarks()
    var code: CodeColors
    var codeColouring: DispatchWorkItem?
    let grammar: GrammarChecker
    /// Counts changes to the text, so a grammar check that finishes after
    /// the text moved on is dropped.
    var editCount = 0
    var grammarCheck: DispatchWorkItem?
    /// Whether a restyle for newly arrived math or images is on its way,
    /// and what arrived since the last one.
    var redrawQueued = false
    var arrivedMath: Set<String> = []
    var arrivedImages: Set<URL> = []
    var cardImagesArrived = false
    /// Set while the cursor is put in a table for one of its commands, so
    /// it isn't taken for a tap on the table's grid.
    var placingCursorForTable = false
    /// Where the line at the top of the screen should start, once the text
    /// is laid out, when the note opens where it was left.
    var pendingTop: Int?
    /// The wide tables' scrolling grids, by where each table starts.
    var tableGrids: [UInt32: TableGridView] = [:]
    private let saver: NoteSaver
    private let displayParagraphs = DisplayParagraphs()
    private var blockFragments: BlockFragments
    weak var host: EditingHost?
    private var styledLength = 0
    private var isRestyling = false
    /// The text view's edits the core hasn't heard about yet.
    var pendingEdit: PendingEdit?
    /// The edit count the sentence tints were fetched for.
    private var tintedEdits = -1
    private var scrollsToCursorAfterLayout = false
    private var readableWidth = true
    /// The text as it was before the edits since the last save, kept as a
    /// snapshot when the recovery interval allows.
    private var textBeforeEdits: String?
    /// The text as last read from or written to disk: where this phone's
    /// unsaved edits start from.
    private(set) var savedText: String

    init(path: String, text: String, vault: VaultFolder, tokens: Tokens, host: EditingHost) {
        self.path = path
        savedText = text
        self.vault = vault
        self.tokens = tokens
        self.host = host
        document = vault.document(text: text)
        media = NoteMedia(vault: vault, notePath: path)
        grammar = GrammarService.checker(for: vault)
        prose.colors = ProseColors(vault: vault)
        code = CodeColors(vault: vault)
        styler = PlanStyler(tokens: tokens)
        styler.media = media
        blockFragments = BlockFragments(tokens: tokens)
        saver = NoteSaver(vault: vault, path: path)
        super.init()
        configure()
        textView.text = text
        pendingEdit = nil
        restorePosition()
        prefetchMath()
        scheduleGrammarCheck()
        scheduleCodeColours()
        NotificationCenter.default.addObserver(
            self, selector: #selector(saveNow), name: UIApplication.didEnterBackgroundNotification, object: nil
        )
    }

    private func configure() {
        textView.delegate = self
        textView.widthDidChange = { [weak self] in self?.columnWidthChanged() }
        textView.didLayout = { [weak self] in self?.placeTableGrids() }
        textView.textLayoutManager?.delegate = blockFragments
        textView.textLayoutManager?.textContentManager?.delegate = displayParagraphs
        textView.keyboardDismissMode = .interactive
        textView.alwaysBounceVertical = true
        textView.autocapitalizationType = .sentences
        textView.smartQuotesType = .no
        textView.smartDashesType = .no
        if grammar.isEnabled() { textView.spellCheckingType = .no }
        textView.isFindInteractionEnabled = true
        textView.textContainer.lineFragmentPadding = 0
        NotificationCenter.default.addObserver(
            self, selector: #selector(storageEdited(_:)), name: NSTextStorage.didProcessEditingNotification,
            object: textView.textStorage
        )
        textView.boundKeys = KeyCommands.uiKeyCommands(
            host?.keyBindings ?? [], action: #selector(EditorTextView.runBoundKey(_:))
        )
        textView.runBoundCommand = { [weak self] command in self?.host?.run(command) }
        let toolbar = host?.keyboardToolbar ?? PhoneToolbar(enabled: false, labels: .icons, entries: [])
        let bar = AccessoryBar(toolbar: toolbar, tokens: tokens) { [weak self] command in
            self?.host?.run(command)
        }
        accessoryBar = bar
        textView.inputAccessoryView = toolbar.enabled ? bar : nil
        applyColors()
        installTapHandling()
    }

    private func applyColors() {
        textView.backgroundColor = tokens.color(\.background)
        textView.tintColor = tokens.color(\.accent)
        textView.typingAttributes = styler.typingAttributes
    }

    // MARK: Look

    /// Draws with new tokens, such as after a zoom or a theme change.
    func use(_ tokens: Tokens) {
        self.tokens = tokens
        styler = PlanStyler(tokens: tokens)
        styler.media = media
        blockFragments = BlockFragments(tokens: tokens)
        textView.textLayoutManager?.delegate = blockFragments
        applyColors()
        columnWidthChanged()
    }

    /// Shows the keyboard toolbar anew, after toolbars.toml changed; a
    /// toolbar that's been turned off takes the bar away.
    func showToolbar(_ toolbar: PhoneToolbar) {
        accessoryBar?.show(toolbar)
        let wanted: UIView? = toolbar.enabled ? accessoryBar : nil
        if textView.inputAccessoryView !== wanted {
            textView.inputAccessoryView = wanted
            textView.reloadInputViews()
        }
    }

    func setReadableWidth(_ enabled: Bool) {
        readableWidth = enabled
        columnWidthChanged()
    }

    /// Plans every line again, after something that changes how every line
    /// draws, such as cycling Markdown symbols.
    func redrawAll() {
        styler.forgetApplied()
        restyle(edited: nil)
    }

    private func columnWidthChanged() {
        let spacing = tokens.spacing
        let side = sideInset(spacing)
        textView.textContainerInset = UIEdgeInsets(
            top: CGFloat(spacing.lg), left: side, bottom: CGFloat(spacing.xxl) * 4, right: side
        )
        styler.setColumnWidth(textView.bounds.width - side * 2)
        restyle(edited: nil)
        if pendingTop != nil {
            DispatchQueue.main.async { [weak self] in self?.scrollToPendingTop() }
        }
        guard scrollsToCursorAfterLayout else { return }
        scrollsToCursorAfterLayout = false
        DispatchQueue.main.async { [textView] in
            textView.scrollRangeToVisible(textView.selectedRange)
        }
    }

    /// The side margins: the theme's gutter, and on a wide screen enough
    /// to keep lines at a readable length.
    private func sideInset(_ spacing: Spacing) -> CGFloat {
        let gutter = CGFloat(spacing.xl)
        let maxWidth = CGFloat(spacing.editorMaxWidth)
        guard readableWidth, textView.bounds.width > maxWidth + gutter * 2 else { return gutter }
        return (textView.bounds.width - maxWidth) / 2
    }

    // MARK: Cursor

    /// Puts the cursor at `location` (UTF-16) and shows it, without raising
    /// the keyboard.
    func placeCursor(at location: Int) {
        textView.selectedRange = NSRange(location: min(location, textView.textStorage.length), length: 0)
        pendingTop = nil
        if textView.bounds.width == 0 {
            scrollsToCursorAfterLayout = true
        } else {
            textView.scrollRangeToVisible(textView.selectedRange)
        }
    }

    /// Puts the cursor at the start of the heading titled `title`.
    func showHeading(_ title: String) {
        let match = document.outline().first { $0.title.caseInsensitiveCompare(title) == .orderedSame }
        if let match { placeCursor(at: Int(match.range.start)) }
    }

    var selection: TextRange {
        TextRange(textView.selectedRange)
    }

    // MARK: Saving

    @objc func saveNow() {
        saver.flush()
        savePosition()
    }

    /// Whether the text has changed since it was last read or saved.
    var hasUnsavedEdits: Bool {
        document.text() != savedText
    }

    /// Stops this session from writing its text, because sync replaced the
    /// note on disk and a new session takes over.
    func retire() {
        saver.discardUnsaved()
    }

    // MARK: UITextViewDelegate

    func textViewDidChange(_ textView: UITextView) {
        guard textView.markedTextRange == nil else { return }
        guard let edit = pendingEdit else {
            restyle(edited: textView.selectedRange)
            return
        }
        pendingEdit = nil
        if textBeforeEdits == nil { textBeforeEdits = document.text() }
        sendToCore(edit)
        editCount += 1
        restyle(edited: edit.newRange)
        saver.schedule { [weak textView] in textView?.textStorage.string ?? "" } afterSave: { [weak self] text in
            self?.savedText = text
            self?.keepSnapshot()
        }
        scheduleGrammarCheck()
        scheduleCodeColours()
        editState.refresh()
    }

    func textViewDidBeginEditing(_ textView: UITextView) {
        editState.follow(textView.undoManager)
        editState.isEditing = true
        textView.contentInset.bottom = UndoPill.clearance
        showCommandStates()
    }

    func textViewDidEndEditing(_ textView: UITextView) {
        editState.isEditing = false
        textView.contentInset.bottom = 0
    }

    func textViewDidChangeSelection(_ textView: UITextView) {
        guard !isRestyling, textView.markedTextRange == nil,
              textView.textStorage.length == styledLength else { return }
        let selection = textView.selectedRange
        if selection.length == 0, textView.isFirstResponder, editGridCell(at: selection.location) { return }
        restyle(edited: nil)
    }

    /// Keeps the text the saved edits replaced, once the recovery interval
    /// has passed since the last snapshot.
    private func keepSnapshot() {
        guard let before = textBeforeEdits else { return }
        textBeforeEdits = nil
        _ = vault.recordSnapshot(path: path, text: before, now: false)
    }

    func restyle(edited: NSRange?) {
        let storage = textView.textStorage
        if tintedEdits != editCount || styler.needsWholePlan {
            prose.tints = document.sentenceTints()
            tintedEdits = editCount
        }
        isRestyling = true
        let selection = textView.selectedRange
        textView.changeAttributes {
            if !applyPlan(to: storage, edited: edited, whole: styler.needsWholePlan) {
                styler.forgetApplied()
                _ = applyPlan(to: storage, edited: edited, whole: true)
            }
        }
        if textView.selectedRange != selection { textView.selectedRange = selection }
        textView.typingAttributes = styler.typingAttributes
        styledLength = storage.length
        isRestyling = false
        fetchMissingMedia()
        updateTableGrids()
        showCommandStates()
    }

    /// Presses the keyboard bar's toggles that are on at the cursor and
    /// greys out the commands that would do nothing there.
    private func showCommandStates() {
        guard textView.isFirstResponder, let accessoryBar else { return }
        accessoryBar.show(document.commandStates(selection: selection))
    }

    private func applyPlan(to storage: NSTextStorage, edited: NSRange?, whole: Bool) -> Bool {
        let update = document.planUpdate(selection: TextRange(textView.selectedRange), whole: whole)
        return styler.apply(update, prose: prose, code: code, to: storage, edited: edited)
    }
}
