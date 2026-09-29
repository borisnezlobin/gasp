import ImageIO
import UIKit

/// Images from the vault, decoded off the main thread at the size they're
/// drawn rather than their own, so a folder of photos doesn't fill memory.
/// A file's pixel size is read from its header alone, so the text lays out
/// at the right height before the pixels arrive.
final class VaultImages {
    static let shared = VaultImages()

    private let decoded = NSCache<NSString, UIImage>()
    private var pixelSizes: [URL: CGSize] = [:]
    private var loading: [NSString: [() -> Void]] = [:]
    private let queue = OperationQueue()

    private init() {
        decoded.totalCostLimit = 64 * 1024 * 1024
        queue.maxConcurrentOperationCount = 2
        queue.qualityOfService = .userInitiated
    }

    /// The file's size in pixels, or `nil` when it isn't an image.
    func pixelSize(of file: URL) -> CGSize? {
        if let known = pixelSizes[file] { return known }
        guard let source = CGImageSourceCreateWithURL(file as CFURL, nil),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? CGFloat,
              let height = properties[kCGImagePropertyPixelHeight] as? CGFloat
        else { return nil }
        let turned = (properties[kCGImagePropertyOrientation] as? UInt32).map { $0 >= 5 } ?? false
        let size = turned ? CGSize(width: height, height: width) : CGSize(width: width, height: height)
        pixelSizes[file] = size
        return size
    }

    /// The image decoded to fit `pixels` across, if it's ready.
    func image(_ file: URL, pixels: Int) -> UIImage? {
        decoded.object(forKey: Self.key(file, pixels))
    }

    /// Decodes the image to fit `pixels` across; `ready` runs on the main
    /// thread once it's there. Called on the main thread.
    func load(_ file: URL, pixels: Int, ready: @escaping () -> Void) {
        let key = Self.key(file, pixels)
        guard decoded.object(forKey: key) == nil else { return ready() }
        let first = loading[key] == nil
        loading[key, default: []].append(ready)
        guard first else { return }
        queue.addOperation { [weak self] in
            let image = Self.downsampled(file, pixels: pixels)
            DispatchQueue.main.async {
                if let image {
                    self?.decoded.setObject(image, forKey: key, cost: Self.cost(of: image))
                }
                self?.loading.removeValue(forKey: key)?.forEach { $0() }
            }
        }
    }

    private static func key(_ file: URL, _ pixels: Int) -> NSString {
        "\(pixels) \(file.path)" as NSString
    }

    private static func cost(of image: UIImage) -> Int {
        guard let cgImage = image.cgImage else { return 1 }
        return cgImage.bytesPerRow * cgImage.height
    }

    private static func downsampled(_ file: URL, pixels: Int) -> UIImage? {
        let sourceOptions = [kCGImageSourceShouldCache: false] as CFDictionary
        guard let source = CGImageSourceCreateWithURL(file as CFURL, sourceOptions) else { return nil }
        let options = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: max(pixels, 1)
        ] as CFDictionary
        guard let thumbnail = CGImageSourceCreateThumbnailAtIndex(source, 0, options) else { return nil }
        return UIImage(cgImage: thumbnail)
    }
}
