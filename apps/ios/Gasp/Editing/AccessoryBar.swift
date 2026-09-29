import UIKit

/// The bar above the software keyboard: the `mobile.toolbar` commands as
/// icon buttons in a row that scrolls sideways when they don't fit.
final class AccessoryBar: UIInputView {
    private let scroller = UIScrollView()
    private let row = UIStackView()
    private let run: (String) -> Void
    private let tokens: Tokens
    private static let height: CGFloat = 46

    init(commands: [CommandInfo], tokens: Tokens, run: @escaping (String) -> Void) {
        self.run = run
        self.tokens = tokens
        super.init(frame: CGRect(x: 0, y: 0, width: 0, height: Self.height), inputViewStyle: .keyboard)
        allowsSelfSizing = true
        layOut()
        show(commands)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("The bar is made in code")
    }

    override var intrinsicContentSize: CGSize {
        CGSize(width: UIView.noIntrinsicMetric, height: Self.height)
    }

    /// Replaces the buttons, after the toolbar setting changed.
    func show(_ commands: [CommandInfo]) {
        row.arrangedSubviews.forEach { $0.removeFromSuperview() }
        commands.forEach { row.addArrangedSubview(button(for: $0)) }
    }

    private func layOut() {
        scroller.showsHorizontalScrollIndicator = false
        scroller.alwaysBounceHorizontal = true
        scroller.translatesAutoresizingMaskIntoConstraints = false
        row.axis = .horizontal
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

    private func button(for command: CommandInfo) -> UIButton {
        var configuration = UIButton.Configuration.plain()
        configuration.image = UIImage(systemName: CommandSymbols.name(for: command.id))
        configuration.preferredSymbolConfigurationForImage = UIImage.SymbolConfiguration(textStyle: .body)
        configuration.baseForegroundColor = tokens.color(\.icon)
        configuration.cornerStyle = .medium
        let button = UIButton(configuration: configuration, primaryAction: UIAction { [weak self] _ in
            self?.run(command.id)
        })
        button.configurationUpdateHandler = { [tokens] button in
            let fill = button.isHighlighted ? tokens.color(\.fillStrong) : .clear
            button.configuration?.background.backgroundColor = fill
        }
        button.accessibilityLabel = command.title
        button.widthAnchor.constraint(equalToConstant: 44).isActive = true
        return button
    }
}
