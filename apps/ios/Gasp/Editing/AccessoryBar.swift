import UIKit

/// The bar above the software keyboard: the `keyboard` toolbar from the
/// vault's toolbars.toml, its commands as buttons (icons, labels or both,
/// as the toolbar says), separators as thin lines and menus as buttons
/// that open a menu of commands, in a row that scrolls sideways when they
/// don't fit.
final class AccessoryBar: UIInputView {
    private let scroller = UIScrollView()
    private let row = UIStackView()
    private let run: (String) -> Void
    private let tokens: Tokens
    private static let height: CGFloat = 46
    private static let buttonSide: CGFloat = 44

    init(toolbar: KeyboardToolbar, tokens: Tokens, run: @escaping (String) -> Void) {
        self.run = run
        self.tokens = tokens
        super.init(frame: CGRect(x: 0, y: 0, width: 0, height: Self.height), inputViewStyle: .keyboard)
        allowsSelfSizing = true
        layOut()
        show(toolbar)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("The bar is made in code")
    }

    override var intrinsicContentSize: CGSize {
        CGSize(width: UIView.noIntrinsicMetric, height: Self.height)
    }

    /// Replaces what's on the bar, after toolbars.toml changed.
    func show(_ toolbar: KeyboardToolbar) {
        row.arrangedSubviews.forEach { $0.removeFromSuperview() }
        for entry in toolbar.entries {
            row.addArrangedSubview(view(for: entry, labels: toolbar.labels))
        }
    }

    private func layOut() {
        scroller.showsHorizontalScrollIndicator = false
        scroller.alwaysBounceHorizontal = true
        scroller.translatesAutoresizingMaskIntoConstraints = false
        row.axis = .horizontal
        row.alignment = .center
        row.spacing = CGFloat(tokens.spacing.xs)
        row.translatesAutoresizingMaskIntoConstraints = false
        addSubview(scroller)
        scroller.addSubview(row)
        let inset = CGFloat(tokens.spacing.sm)
        NSLayoutConstraint.activate([
            scroller.leadingAnchor.constraint(equalTo: leadingAnchor),
            scroller.trailingAnchor.constraint(equalTo: trailingAnchor),
            scroller.topAnchor.constraint(equalTo: topAnchor),
            scroller.bottomAnchor.constraint(equalTo: bottomAnchor),
            row.leadingAnchor.constraint(equalTo: scroller.contentLayoutGuide.leadingAnchor, constant: inset),
            row.trailingAnchor.constraint(equalTo: scroller.contentLayoutGuide.trailingAnchor, constant: -inset),
            row.centerYAnchor.constraint(equalTo: scroller.frameLayoutGuide.centerYAnchor),
            row.heightAnchor.constraint(equalToConstant: Self.height - inset * 2)
        ])
    }

    private func view(for entry: ToolbarEntry, labels: ToolbarLabels) -> UIView {
        switch entry {
        case let .command(command):
            return button(for: command, labels: labels)
        case .separator:
            return separator()
        case .spacer:
            return gap()
        case let .menu(title, commands):
            return menuButton(title: title, commands: commands, labels: labels)
        }
    }

    private func configuration(title: String?, symbol: String?) -> UIButton.Configuration {
        var configuration = UIButton.Configuration.plain()
        configuration.image = symbol.flatMap { UIImage(systemName: $0) }
        configuration.title = title
        configuration.imagePadding = CGFloat(tokens.spacing.xs)
        configuration.preferredSymbolConfigurationForImage = UIImage.SymbolConfiguration(textStyle: .body)
        configuration.baseForegroundColor = tokens.color(\.icon)
        configuration.cornerStyle = .medium
        return configuration
    }

    /// The title and symbol a button shows, as the toolbar's style says.
    private func face(title: String, symbol: String, labels: ToolbarLabels) -> (String?, String?) {
        switch labels {
        case .icons: return (nil, symbol)
        case .iconsAndLabels: return (title, symbol)
        case .labels: return (title, nil)
        }
    }

    private func styled(_ button: UIButton, title: String, labelled: Bool) -> UIButton {
        button.configurationUpdateHandler = { [tokens] button in
            let fill = button.isHighlighted ? tokens.color(\.fillStrong) : .clear
            button.configuration?.background.backgroundColor = fill
        }
        button.accessibilityLabel = title
        let width = labelled
            ? button.widthAnchor.constraint(greaterThanOrEqualToConstant: Self.buttonSide)
            : button.widthAnchor.constraint(equalToConstant: Self.buttonSide)
        width.isActive = true
        return button
    }

    private func button(for command: CommandInfo, labels: ToolbarLabels) -> UIButton {
        let (title, symbol) = face(
            title: command.title, symbol: CommandSymbols.name(for: command.id), labels: labels
        )
        let button = UIButton(
            configuration: configuration(title: title, symbol: symbol),
            primaryAction: UIAction { [weak self] _ in self?.run(command.id) }
        )
        return styled(button, title: command.title, labelled: title != nil)
    }

    private func menuButton(title: String, commands: [CommandInfo], labels: ToolbarLabels) -> UIButton {
        let (shown, symbol) = face(title: title, symbol: "ellipsis.circle", labels: labels)
        let actions = commands.map { command in
            let image = UIImage(systemName: CommandSymbols.name(for: command.id))
            return UIAction(title: command.title, image: image) { [weak self] _ in
                self?.run(command.id)
            }
        }
        let button = UIButton(configuration: configuration(title: shown, symbol: symbol))
        button.menu = UIMenu(title: title, children: actions)
        button.showsMenuAsPrimaryAction = true
        return styled(button, title: title, labelled: shown != nil)
    }

    /// A thin line between groups of buttons.
    private func separator() -> UIView {
        let line = UIView()
        line.backgroundColor = tokens.color(\.divider)
        line.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            line.widthAnchor.constraint(equalToConstant: 1),
            line.heightAnchor.constraint(equalToConstant: Self.buttonSide / 2)
        ])
        return line
    }

    /// A wider gap: the bar scrolls, so there's no far end to push to.
    private func gap() -> UIView {
        let space = UIView()
        space.translatesAutoresizingMaskIntoConstraints = false
        space.widthAnchor.constraint(equalToConstant: Self.buttonSide / 2).isActive = true
        return space
    }
}
