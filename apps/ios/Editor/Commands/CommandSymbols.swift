/// The SF Symbol for each command that shows as a button, in the
/// keyboard's bar and in the palette.
enum CommandSymbols {
    private static let names: [String: String] = [
        "keyboard.hide": "keyboard.chevron.compact.down",
        "note.import-image": "photo",
        "edit.indent": "increase.indent",
        "edit.outdent": "decrease.indent",
        "format.callout": "text.bubble",
        "format.math-inline": "function",
        "footnote.insert-or-jump": "textformat.superscript",
        "prose.toggle-sentence-highlighting": "text.line.first.and.arrowtriangle.forward",
        "table.insert": "tablecells",
        "find.open": "magnifyingglass",
        "find.replace": "arrow.left.arrow.right",
        "edit.undo": "arrow.uturn.backward",
        "edit.redo": "arrow.uturn.forward",
        "format.bold": "bold",
        "format.italic": "italic",
        "format.underline": "underline",
        "format.strikethrough": "strikethrough",
        "format.highlight": "highlighter",
        "format.link": "link",
        "format.code": "chevron.left.forwardslash.chevron.right",
        "format.comment": "percent",
        "edit.toggle-task": "checklist",
        "palette.open": "ellipsis",
        "search.open": "text.magnifyingglass",
        "switcher.open": "doc.text.magnifyingglass",
        "note.new": "square.and.pencil",
        "daily.open": "calendar",
        "tab.new": "plus.square.on.square",
        "tab.close": "xmark.square",
        "settings.open": "gearshape",
        "app.export": "square.and.arrow.up",
        "app.print": "printer",
        "note.recover": "clock.arrow.circlepath",
        "note.rename": "character.cursor.ibeam",
        "note.delete": "trash",
        "edit.move-line-up": "arrow.up.to.line",
        "edit.move-line-down": "arrow.down.to.line",
        "link.follow": "arrow.up.forward.square",
        "markdown.cycle-symbols": "number.square",
        "sidebar.files.toggle": "sidebar.leading",
        "outline.jump-to-heading": "list.bullet.indent",
        "history.back": "chevron.backward",
        "history.forward": "chevron.forward",
        "template.insert": "doc.badge.plus",
        "edit.look-up": "character.book.closed",
        "view.zoom-in": "plus.magnifyingglass",
        "view.zoom-out": "minus.magnifyingglass"
    ]

    /// The symbol for `command`, or a generic one.
    static func name(for command: String) -> String {
        names[command] ?? "command"
    }
}
