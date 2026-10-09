import UIKit

/// The bar above the software keyboard: the `keyboard` toolbar from the
/// vault's toolbars.toml, its commands as buttons (icons, labels or both,
/// as the toolbar says), separators as thin lines and menus as buttons
/// that open a menu of commands, in a row that scrolls sideways when they
/// don't fit. The button that hides the keyboard stays put at the right
/// end, and the row fades out as it passes under it. Toggles that are on
/// where the cursor is show pressed, and commands that would change
/// nothing there are greyed out.
final class AccessoryBar: UIInputView {
    /// Holds the scrolling row, and fades it out at its right edge.
    private let rail = UIView()
    private let scroller = UIScrollView()
    private let row = UIStackView()
    private let fade = CAGradientLayer()
    private let run: (String) -> Void
    private let tokens: Tokens
    /// The command buttons on the bar, by command.
    private var commandButtons: [String: UIButton] = [:]
    private static let height: CGFloat = 46
    private static let buttonSide: CGFloat = 44
    private static let hideCommand = "keyboard.hide"

    init(toolbar: PhoneToolbar, tokens: Tokens, run: @escaping (String) -> Void) {
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

    override func layoutSubviews() {
        super.layoutSubviews()
        fade.frame = rail.bounds
        let fadeWidth = CGFloat(tokens.spacing.lg) / max(rail.bounds.width, 1)
        fade.locations = [0, NSNumber(value: 1 - fadeWidth), 1]
    }

    /// Replaces what's on the bar, after toolbars.toml changed.
    func show(_ toolbar: PhoneToolbar) {
        row.arrangedSubviews.forEach { $0.removeFromSuperview() }
        commandButtons = [:]
        for entry in toolbar.entries {
            row.addArrangedSubview(view(for: entry, labels: toolbar.labels))
        }
    }

    private func layOut() {
        scroller.showsHorizontalScrollIndicator = false
        scroller.alwaysBounceHorizontal = true
        scroller.translatesAutoresizingMaskIntoConstraints = false
        fade.colors = [UIColor.black.cgColor, UIColor.black.cgColor, UIColor.clear.cgColor]
        fade.startPoint = CGPoint(x: 0, y: 0.5)
        fade.endPoint = CGPoint(x: 1, y: 0.5)
        rail.layer.mask = fade
        rail.translatesAutoresizingMaskIntoConstraints = false
        row.axis = .horizontal
        row.alignment = .center
        row.spacing = CGFloat(tokens.spacing.xs)
        row.translatesAutoresizingMaskIntoConstraints = false
        let hide = hideButton()
        addSubview(rail)
        addSubview(hide)
        rail.addSubview(scroller)
        scroller.addSubview(row)
        constrain(hide: hide)
    }

    private func constrain(hide: UIButton) {
        let inset = CGFloat(tokens.spacing.sm)
        NSLayoutConstraint.activate([
            rail.leadingAnchor.constraint(equalTo: leadingAnchor),
            rail.trailingAnchor.constraint(equalTo: hide.leadingAnchor),
            rail.topAnchor.constraint(equalTo: topAnchor),
            rail.bottomAnchor.constraint(equalTo: bottomAnchor),
            scroller.leadingAnchor.constraint(equalTo: rail.leadingAnchor),
            scroller.trailingAnchor.constraint(equalTo: rail.trailingAnchor),
            scroller.topAnchor.constraint(equalTo: rail.topAnchor),
            scroller.bottomAnchor.constraint(equalTo: rail.bottomAnchor),
            hide.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -inset),
            hide.centerYAnchor.constraint(equalTo: centerYAnchor),
            hide.heightAnchor.constraint(equalToConstant: Self.height - inset * 2),
            row.leadingAnchor.constraint(equalTo: scroller.contentLayoutGuide.leadingAnchor, constant: inset),
            row.trailingAnchor.constraint(
                equalTo: scroller.contentLayoutGuide.trailingAnchor, constant: -CGFloat(tokens.spacing.lg)
            ),
            row.centerYAnchor.constraint(equalTo: scroller.frameLayoutGuide.centerYAnchor),
            row.heightAnchor.constraint(equalToConstant: Self.height - inset * 2)
        ])
    }

    /// Hides the keyboard. It's on every keyboard bar, whatever the
    /// toolbar's items say, so the keyboard can always be put away.
    private func hideButton() -> UIButton {
        let button = UIButton(
            configuration: configuration(title: nil, symbol: CommandSymbols.name(for: Self.hideCommand)),
            primaryAction: UIAction { [weak self] _ in self?.run(Self.hideCommand) }
        )
        button.translatesAutoresizingMaskIntoConstraints = false
        return styled(button, title: "Hide the keyboard", labelled: false)
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
        case .widget:
            return gap()
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

    /// Shows which commands are on and which would do nothing where the
    /// cursor is now.
    func show(_ states: CommandStates) {
        let pressed = Set(states.pressed)
        let unavailable = Set(states.unavailable)
        for (command, button) in commandButtons {
            button.isSelected = pressed.contains(command)
            button.isEnabled = !unavailable.contains(command)
        }
    }

    private func styled(_ button: UIButton, title: String, labelled: Bool) -> UIButton {
        button.configurationUpdateHandler = { [tokens] button in
            let fill: UIColor = button.isHighlighted ? tokens.color(\.fillStrong)
                : button.isSelected ? tokens.color(\.fill) : .clear
            let ink: KeyPath<Palette, ThemeColor> = !button.isEnabled ? \.iconDisabled
                : button.isSelected ? \.accent : \.icon
            button.configuration?.background.backgroundColor = fill
            button.configuration?.baseForegroundColor = tokens.color(ink)
        }
        button.accessibilityLabel = title
        // A labelled button is as wide as its label on one line, neither
        // stretched nor squeezed by the scrolling row.
        button.setContentHuggingPriority(.required, for: .horizontal)
        button.setContentCompressionResistancePriority(.required, for: .horizontal)
        button.configuration?.titleLineBreakMode = .byClipping
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
        button.changesSelectionAsPrimaryAction = false
        if command.id == ImageSource.command {
            button.menu = UIMenu(title: command.title, children: imageSourceActions())
            button.showsMenuAsPrimaryAction = true
        }
        commandButtons[command.id] = button
        return styled(button, title: command.title, labelled: title != nil)
    }

    private func menuButton(title: String, commands: [CommandInfo], labels: ToolbarLabels) -> UIButton {
        let (shown, symbol) = face(title: title, symbol: "ellipsis.circle", labels: labels)
        let actions = commands.map { command -> UIMenuElement in
            let image = UIImage(systemName: CommandSymbols.name(for: command.id))
            if command.id == ImageSource.command {
                return UIMenu(title: command.title, image: image, children: imageSourceActions())
            }
            return UIAction(title: command.title, image: image) { [weak self] _ in
                self?.run(command.id)
            }
        }
        let button = UIButton(configuration: configuration(title: shown, symbol: symbol))
        button.menu = UIMenu(title: title, children: actions)
        button.showsMenuAsPrimaryAction = true
        return styled(button, title: title, labelled: shown != nil)
    }

    private func imageSourceActions() -> [UIAction] {
        ImageSource.allCases.map { source in
            UIAction(title: source.title, image: UIImage(systemName: source.symbol)) { [weak self] _ in
                self?.run(source.id)
            }
        }
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
