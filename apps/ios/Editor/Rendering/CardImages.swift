import CryptoKit
import ImageIO
import UIKit

/// Link cards' preview images from the web: downloaded once, kept in the
/// app's caches folder, and drawn as a square cropped from the middle with
/// rounded corners. A card keeps its size while its image downloads.
final class CardImages {
    static let shared = CardImages()

    private let thumbnails = NSCache<NSString, UIImage>()
    private var loading: [URL: [() -> Void]] = [:]
    private var failed: Set<URL> = []
    private let folder = URL.cachesDirectory.appending(path: "cards", directoryHint: .isDirectory)

    private init() {
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    }

    /// The preview at `side` points square with corners of `radius`, once
    /// it's downloaded.
    func image(_ url: URL, side: CGFloat, radius: CGFloat) -> UIImage? {
        let key = "\(Int(side)) \(url.absoluteString)" as NSString
        if let thumbnail = thumbnails.object(forKey: key) { return thumbnail }
        guard let data = try? Data(contentsOf: file(for: url)),
              let thumbnail = Self.square(data, side: side, radius: radius) else { return nil }
        thumbnails.setObject(thumbnail, forKey: key)
        return thumbnail
    }

    /// Downloads the preview; `ready` runs on the main thread once it's
    /// kept. Called on the main thread.
    func load(_ url: URL, ready: @escaping () -> Void) {
        guard !failed.contains(url), !FileManager.default.fileExists(atPath: file(for: url).path) else { return }
        let first = loading[url] == nil
        loading[url, default: []].append(ready)
        guard first else { return }
        let destination = file(for: url)
        URLSession.shared.dataTask(with: url) { [weak self] data, response, _ in
            let succeeded = (response as? HTTPURLResponse).map { (200..<300).contains($0.statusCode) } ?? false
            if succeeded, let data { try? data.write(to: destination) }
            DispatchQueue.main.async {
                if !succeeded { self?.failed.insert(url) }
                self?.loading.removeValue(forKey: url)?.forEach { $0() }
            }
        }.resume()
    }

    private func file(for url: URL) -> URL {
        let digest = SHA256.hash(data: Data(url.absoluteString.utf8))
        return folder.appending(path: digest.prefix(12).map { String(format: "%02x", $0) }.joined())
    }

    /// The middle of the picture, `side` points square with rounded corners.
    private static func square(_ data: Data, side: CGFloat, radius: CGFloat) -> UIImage? {
        let scale = UIScreen.main.scale
        let options = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: Int(side * scale * 3)
        ] as CFDictionary
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let decoded = CGImageSourceCreateThumbnailAtIndex(source, 0, options) else { return nil }
        let picture = UIImage(cgImage: decoded)
        let format = UIGraphicsImageRendererFormat()
        format.scale = scale
        let size = CGSize(width: side, height: side)
        return UIGraphicsImageRenderer(size: size, format: format).image { _ in
            UIBezierPath(roundedRect: CGRect(origin: .zero, size: size), cornerRadius: radius).addClip()
            let fill = max(side / picture.size.width, side / picture.size.height)
            let drawn = CGSize(width: picture.size.width * fill, height: picture.size.height * fill)
            picture.draw(in: CGRect(x: (side - drawn.width) / 2, y: (side - drawn.height) / 2,
                                    width: drawn.width, height: drawn.height))
        }
    }
}
