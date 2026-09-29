import UIKit

/// One equation at one size: what a rendered image is cached by.
struct MathKey: Hashable {
    let tex: String
    let display: Bool
    /// In hundredths of a point, so sizes that print the same share a key.
    let size: Int

    init(tex: String, display: Bool, fontSize: CGFloat) {
        self.tex = tex
        self.display = display
        size = Int((fontSize * 100).rounded())
    }

    var fontSize: CGFloat {
        CGFloat(size) / 100
    }
}

/// An equation drawn by the core as coverage (a grey image whose white is
/// ink), with its size and baseline in points. It takes the colour it's
/// drawn in, so one render serves light and dark mode.
final class RenderedMath {
    let mask: CGImage
    let size: CGSize
    /// From the top edge down to the baseline.
    let baseline: CGFloat
    let scale: CGFloat
    /// The equation and size it was rendered for.
    let key: MathKey

    init(key: MathKey, mask: CGImage, size: CGSize, baseline: CGFloat, scale: CGFloat) {
        self.key = key
        self.mask = mask
        self.size = size
        self.baseline = baseline
        self.scale = scale
    }

    /// The bytes the image holds, for the cache's budget.
    var cost: Int { mask.bytesPerRow * mask.height }

    func draw(in rect: CGRect, color: UIColor, context: CGContext) {
        context.saveGState()
        context.translateBy(x: rect.minX, y: rect.maxY)
        context.scaleBy(x: 1, y: -1)
        let flipped = CGRect(origin: .zero, size: rect.size)
        context.clip(to: flipped, mask: mask)
        context.setFillColor(color.cgColor)
        context.fill(flipped)
        context.restoreGState()
    }

    /// The equation filled with `color` as resolved for `traits`.
    func image(color: UIColor, traits: UITraitCollection) -> UIImage {
        let format = UIGraphicsImageRendererFormat()
        format.scale = scale
        let resolved = color.resolvedColor(with: traits)
        return UIGraphicsImageRenderer(size: size, format: format).image { renderer in
            draw(in: CGRect(origin: .zero, size: size), color: resolved, context: renderer.cgContext)
        }
    }
}

/// Hears about each equation as its render finishes.
protocol MathImagesObserver: AnyObject {
    func mathArrived(_ key: MathKey)
}

/// Rendered equations, shared by every open note. Renders run on a few
/// background threads; a note asks for the ones it shows and hears back
/// on the main thread once a batch is ready. A note's equations are all
/// asked for when it opens, so most are ready before they scroll in.
final class MathImages {
    static let shared = MathImages()

    private let cache = NSCache<MathCacheKey, RenderedMath>()
    private var failed: [MathKey: String] = [:]
    private var pending: Set<MathKey> = []
    private let observers = NSHashTable<AnyObject>.weakObjects()
    private let queue = OperationQueue()
    private let scale = UIScreen.main.scale

    private init() {
        cache.totalCostLimit = 48 * 1024 * 1024
        queue.maxConcurrentOperationCount = min(max(ProcessInfo.processInfo.activeProcessorCount - 2, 2), 4)
        queue.qualityOfService = .userInitiated
        queue.addOperation { warmUpMath() }
    }

    /// How many renders are asked for and not finished yet.
    var pendingCount: Int { pending.count }

    func image(_ key: MathKey) -> RenderedMath? {
        cache.object(forKey: MathCacheKey(key))
    }

    /// Why the equation couldn't be drawn, once its render failed.
    func failure(_ key: MathKey) -> String? {
        failed[key]
    }

    /// Tells `observer`, on the main thread, about every render that
    /// finishes while it's around.
    func observe(_ observer: MathImagesObserver) {
        observers.add(observer)
    }

    /// Renders every key not ready or on its way yet, in order. Called on
    /// the main thread.
    func request(_ keys: some Sequence<MathKey>) {
        for key in keys where image(key) == nil && failed[key] == nil {
            guard pending.insert(key).inserted else { continue }
            let scale = scale
            queue.addOperation { [weak self] in
                let result = renderMath(tex: key.tex, display: key.display,
                                        fontSize: Double(key.fontSize), pixelsPerPoint: Double(scale))
                DispatchQueue.main.async { self?.finish(key, result) }
            }
        }
    }

    private func finish(_ key: MathKey, _ result: MathRender) {
        pending.remove(key)
        switch result {
        case .drawn(let image):
            if let rendered = Self.rendered(image, key: key, scale: scale) {
                cache.setObject(rendered, forKey: MathCacheKey(key), cost: rendered.cost)
            }
        case .failed(let message):
            failed[key] = message
        }
        for case let observer as MathImagesObserver in observers.allObjects {
            observer.mathArrived(key)
        }
    }

    private static func rendered(_ image: MathImage, key: MathKey, scale: CGFloat) -> RenderedMath? {
        let width = Int(image.pixelWidth)
        let height = Int(image.pixelHeight)
        guard let provider = CGDataProvider(data: image.coverage as CFData),
              let mask = CGImage(
                  width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 8, bytesPerRow: width,
                  space: CGColorSpaceCreateDeviceGray(),
                  bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.none.rawValue),
                  provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent
              )
        else { return nil }
        let size = CGSize(width: image.width, height: image.height)
        return RenderedMath(key: key, mask: mask, size: size, baseline: CGFloat(image.baseline), scale: scale)
    }
}

/// `NSCache` wants an object for a key.
private final class MathCacheKey: NSObject {
    let key: MathKey

    init(_ key: MathKey) {
        self.key = key
    }

    override var hash: Int { key.hashValue }

    override func isEqual(_ object: Any?) -> Bool {
        (object as? MathCacheKey)?.key == key
    }
}
