import UIKit

/// Rendered math drawn in a line, sitting on the text's baseline, in the
/// text colour of the mode it's drawn in.
final class MathAttachment: NSTextAttachment {
    private let math: RenderedMath
    private let color: UIColor
    private var drawn: [UIUserInterfaceStyle: UIImage] = [:]

    init(_ math: RenderedMath, color: UIColor) {
        self.math = math
        self.color = color
        super.init(data: nil, ofType: nil)
        let descent = math.size.height - math.baseline
        bounds = CGRect(x: 0, y: -descent, width: math.size.width, height: math.size.height)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("Attachments aren't decoded")
    }

    override func image(
        for bounds: CGRect, attributes: [NSAttributedString.Key: Any] = [:],
        location: NSTextLocation, textContainer: NSTextContainer?
    ) -> UIImage? {
        let traits = UITraitCollection.current
        if let image = drawn[traits.userInterfaceStyle] { return image }
        let image = math.image(color: color, traits: traits)
        drawn[traits.userInterfaceStyle] = image
        return image
    }

    /// The same picture where TextKit 1 draws, as in a table's grid.
    override func image(forBounds imageBounds: CGRect, textContainer: NSTextContainer?, characterIndex charIndex: Int)
        -> UIImage? {
        image(for: imageBounds, location: PlainLocation(), textContainer: textContainer)
    }
}

/// `NSTextAttachment.image(for:…)` wants a location it doesn't read.
final class PlainLocation: NSObject, NSTextLocation {
    func compare(_ location: NSTextLocation) -> ComparisonResult {
        .orderedSame
    }
}

/// An image from the vault drawn in a line at the width the note asks
/// for, or its own, never wider than the column. Until its pixels are
/// decoded it's a quiet placeholder of the same size.
final class ImageAttachment: NSTextAttachment {
    let file: URL
    private let size: CGSize
    private let placeholderColor: UIColor
    private let cornerRadius: CGFloat
    private var finished: UIImage?

    init(file: URL, size: CGSize, placeholderColor: UIColor, cornerRadius: CGFloat) {
        self.file = file
        self.size = size
        self.placeholderColor = placeholderColor
        self.cornerRadius = cornerRadius
        super.init(data: nil, ofType: nil)
        bounds = CGRect(origin: .zero, size: size)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("Attachments aren't decoded")
    }

    /// The longest side in pixels the image is decoded at.
    var pixels: Int {
        Int((max(size.width, size.height) * UIScreen.main.scale).rounded(.up))
    }

    override func image(
        for bounds: CGRect, attributes: [NSAttributedString.Key: Any] = [:],
        location: NSTextLocation, textContainer: NSTextContainer?
    ) -> UIImage? {
        if let finished { return finished }
        let decoded = VaultImages.shared.image(file, pixels: pixels)
        let image = ImageDrawing.rounded(decoded, size: size, fill: placeholderColor, cornerRadius: cornerRadius)
        if decoded != nil { finished = image }
        return image
    }

    override func image(forBounds imageBounds: CGRect, textContainer: NSTextContainer?, characterIndex charIndex: Int)
        -> UIImage? {
        image(for: imageBounds, location: PlainLocation(), textContainer: textContainer)
    }
}

enum ImageDrawing {
    /// `image` clipped to rounded corners, or the fill alone while there's
    /// no image yet.
    static func rounded(_ image: UIImage?, size: CGSize, fill: UIColor, cornerRadius: CGFloat) -> UIImage {
        let format = UIGraphicsImageRendererFormat()
        format.scale = UIScreen.main.scale
        return UIGraphicsImageRenderer(size: size, format: format).image { _ in
            let rect = CGRect(origin: .zero, size: size)
            UIBezierPath(roundedRect: rect, cornerRadius: cornerRadius).addClip()
            guard let image else {
                fill.setFill()
                UIRectFill(rect)
                return
            }
            image.draw(in: rect)
        }
    }

    /// The size an image with `pixels` is drawn at: the width the note asks
    /// for, or its own, scaled down to fit `columnWidth`, keeping its
    /// shape unless the note gives a height too.
    static func displaySize(pixels: CGSize, requested: (UInt32?, UInt32?), columnWidth: CGFloat) -> CGSize {
        let natural = CGSize(width: max(pixels.width, 1), height: max(pixels.height, 1))
        let aspect = min(max(natural.width / natural.height, 0.25), 4)
        let width = requested.0.map { CGFloat($0) } ?? natural.width
        let height: CGFloat = if requested.0 != nil, let wantedHeight = requested.1 {
            CGFloat(wantedHeight)
        } else {
            width / aspect
        }
        let shrink = min(max(columnWidth, 1) / width, 1)
        return CGSize(width: (width * shrink).rounded(), height: (height * shrink).rounded())
    }
}
