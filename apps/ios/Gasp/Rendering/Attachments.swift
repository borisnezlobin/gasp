import UIKit

/// Rendered math drawn in a line, sitting on the text's baseline, in the
/// text colour of the mode it's drawn in.
final class MathAttachment: NSTextAttachment {
    private let math: RenderedMath
    private let color: UIColor

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
        let key = DrawnPictures.key(math, color.resolvedColor(with: traits))
        return DrawnPictures.shared.picture(key) { math.image(color: color, traits: traits) }
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
        let decoded = VaultImages.shared.image(file, pixels: pixels)
        let fill = placeholderColor.resolvedColor(with: .current)
        let key = decoded == nil
            ? "placeholder \(size) \(cornerRadius) \(fill)" : "\(file.path) \(size) \(cornerRadius)"
        return DrawnPictures.shared.picture(key as NSString) {
            ImageDrawing.rounded(decoded, size: size, fill: fill, cornerRadius: cornerRadius)
        }
    }

    override func image(forBounds imageBounds: CGRect, textContainer: NSTextContainer?, characterIndex charIndex: Int)
        -> UIImage? {
        image(for: imageBounds, location: PlainLocation(), textContainer: textContainer)
    }
}

/// Pictures as they're drawn in the text, tinted or with rounded corners,
/// kept for every attachment that draws the same one and dropped first
/// when memory runs short, so restyling a line or scrolling back to it
/// doesn't draw its pictures again.
final class DrawnPictures {
    static let shared = DrawnPictures()

    private let pictures = NSCache<NSString, UIImage>()

    private init() {
        pictures.totalCostLimit = 32 * 1024 * 1024
    }

    static func key(_ math: RenderedMath, _ color: UIColor) -> NSString {
        "math \(math.key.display) \(math.key.size) \(color) \(math.key.tex)" as NSString
    }

    func picture(_ key: NSString, draw: () -> UIImage) -> UIImage {
        if let picture = pictures.object(forKey: key) { return picture }
        let picture = draw()
        let cost = picture.cgImage.map { $0.bytesPerRow * $0.height } ?? 1
        pictures.setObject(picture, forKey: key, cost: cost)
        return picture
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
