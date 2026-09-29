import UIKit

/// What an editing session asks of the browser around it.
protocol EditingHost: AnyObject {
    /// Runs a command from the registry, as the toolbar's buttons do.
    func run(_ command: String)
    /// Opens where a tapped link goes.
    func follow(link target: String, from session: EditingController)
    var toolbar: [CommandInfo] { get }
}

/// One open note: keeps the core's copy in step with the text view,
/// restyles it from each new plan, saves edits and keeps snapshots of the
/// text it replaces.
final class EditingController: NSObject, UITextViewDelegate {
    let textView = EditorTextView(usingTextLayoutManager: true)
    var path: String {
        didSet { saver.path = path }
    }
    let document: NoteDocument
    let vault: VaultFolder
    private(set) var tokens: Tokens
    private(set) var styler: PlanStyler
    private let saver: NoteSaver
    private let displayParagraphs = DisplayParagraphs()
    private var blockFragments: BlockFragments
    weak var host: EditingHost?
    private var styledLength = 0
    private var isRestyling = false
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
        styler = PlanStyler(tokens: tokens)
        blockFragments = BlockFragments(tokens: tokens)
        saver = NoteSaver(vault: vault, path: path)
        super.init()
        configure()
        textView.text = text
        restyle(edited: nil)
        NotificationCenter.default.addObserver(
            self, selector: #selector(saveNow), name: UIApplication.didEnterBackgroundNotification, object: nil
        )
    }

    private func configure() {
        textView.delegate = self
        textView.widthDidChange = { [weak self] in self?.columnWidthChanged() }
        textView.textLayoutManager?.delegate = blockFragments
        textView.textLayoutManager?.textContentManager?.delegate = displayParagraphs
        textView.keyboardDismissMode = .interactive
        textView.alwaysBounceVertical = true
        textView.autocapitalizationType = .sentences
        textView.smartQuotesType = .no
        textView.smartDashesType = .no
        textView.isFindInteractionEnabled = true
        textView.textContainer.lineFragmentPadding = 0
        textView.inputAccessoryView = AccessoryBar(
            commands: host?.toolbar ?? [], tokens: tokens
        ) { [weak self] command in self?.host?.run(command) }
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
        blockFragments = BlockFragments(tokens: tokens)
        textView.textLayoutManager?.delegate = blockFragments
        applyColors()
        columnWidthChanged()
    }

    func showToolbar(_ commands: [CommandInfo]) {
        (textView.inputAccessoryView as? AccessoryBar)?.show(commands)
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
        let text = textView.text ?? ""
        if textBeforeEdits == nil { textBeforeEdits = document.text() }
        document.update(text: text)
        restyle(edited: textView.selectedRange)
        saver.schedule(text) { [weak self] in
            self?.savedText = text
            self?.keepSnapshot()
        }
    }

    func textViewDidChangeSelection(_ textView: UITextView) {
        guard !isRestyling, textView.markedTextRange == nil,
              textView.textStorage.length == styledLength else { return }
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
        let plan = document.plan(selection: TextRange(textView.selectedRange))
        let tints = document.sentenceTints()
        isRestyling = true
        let selection = textView.selectedRange
        textView.textLayoutManager?.textContentManager?.performEditingTransaction {
            styler.apply(plan, tints: tints, to: storage, edited: edited)
        }
        if textView.selectedRange != selection { textView.selectedRange = selection }
        textView.typingAttributes = styler.typingAttributes
        styledLength = storage.length
        isRestyling = false
    }
}
