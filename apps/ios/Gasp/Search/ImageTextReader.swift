import CryptoKit
import Foundation
import ImageIO
import PDFKit
import Vision

/// Reads the text in the vault's images and PDFs with Apple's Vision
/// framework, in the background and at low priority, so vault search finds
/// notes by what their pictures say. The core keeps what was read in a
/// cache on this device, so each file is read once and read again only
/// when it changes.
final class ImageTextReader {
    let texts: ImageTexts
    private let queue = DispatchQueue(label: "com.borisnezlobin.gasp.ocr", qos: .utility)
    /// Files read between saves of the cache.
    private static let saveEvery = 10
    /// PDF pages without a text layer that are read as pictures, at most.
    private static let scannedPages = 12

    init(vault: VaultFolder) {
        texts = ImageTexts(vault: vault, cacheFile: Self.cacheFile(for: vault).path)
    }

    /// Reads every file that's new or changed since the last time.
    func start() {
        queue.async { [texts] in
            let files = texts.filesToRead()
            for (index, path) in files.enumerated() {
                let url = URL(fileURLWithPath: texts.fullPath(path: path))
                // Each file's decoded pixels go as soon as it's read.
                autoreleasepool { texts.record(path: path, text: Self.text(in: url)) }
                if index % Self.saveEvery == Self.saveEvery - 1 { try? texts.save() }
            }
            if !files.isEmpty { try? texts.save() }
        }
    }

    private static func cacheFile(for vault: VaultFolder) -> URL {
        let digest = SHA256.hash(data: Data(vault.location().utf8))
        let name = digest.prefix(8).map { String(format: "%02x", $0) }.joined()
        return URL.cachesDirectory.appending(path: "ocr/\(name).txt")
    }

    private static func text(in file: URL) -> String {
        guard file.pathExtension.lowercased() == "pdf" else {
            return recognised(in: CGImageSourceCreateWithURL(file as CFURL, nil)
                .flatMap { CGImageSourceCreateImageAtIndex($0, 0, nil) })
        }
        return pdfText(file)
    }

    /// A PDF's own text layer, or its first pages read as pictures when it
    /// has none, as a scan doesn't.
    private static func pdfText(_ file: URL) -> String {
        guard let document = PDFDocument(url: file) else { return "" }
        if let layer = document.string, !layer.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return layer
        }
        let pages = (0..<min(document.pageCount, scannedPages)).compactMap(document.page(at:))
        return pages.map { recognised(in: picture(of: $0)) }.joined(separator: "\n")
    }

    private static func picture(of page: PDFPage) -> CGImage? {
        let bounds = page.bounds(for: .mediaBox)
        let scale: CGFloat = 2
        let size = CGSize(width: bounds.width * scale, height: bounds.height * scale)
        let image = page.thumbnail(of: size, for: .mediaBox)
        return image.cgImage
    }

    private static func recognised(in image: CGImage?) -> String {
        guard let image else { return "" }
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = true
        try? VNImageRequestHandler(cgImage: image).perform([request])
        let lines = request.results?.compactMap { $0.topCandidates(1).first?.string } ?? []
        return lines.joined(separator: "\n")
    }
}
