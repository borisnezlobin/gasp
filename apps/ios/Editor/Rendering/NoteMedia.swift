import UIKit

/// Math, vault images (with the pixels to decode them at) and link card
/// images that styling found missing.
struct WantedMedia {
    let math: Set<MathKey>
    let images: [URL: Int]
    let cards: Set<URL>
}

/// What drawing one note needs besides its plan: its rendered math and
/// the vault's images. Styling asks for what it shows; what isn't ready
/// is noted, and the editing session fetches it and restyles those lines.
final class NoteMedia {
    let vault: VaultFolder
    var notePath: String
    /// The text column's width, which images and wide tables fit into.
    var columnWidth: CGFloat = 320
    private(set) var wantedMath: Set<MathKey> = []
    private(set) var wantedImages: [URL: Int] = [:]
    private(set) var wantedCardImages: Set<URL> = []
    private var files: [String: URL] = [:]
    private var missingFiles: Set<String> = []

    init(vault: VaultFolder, notePath: String) {
        self.vault = vault
        self.notePath = notePath
    }

    /// The equation if it's rendered; otherwise it's noted as wanted.
    func math(_ key: MathKey) -> RenderedMath? {
        if let rendered = MathImages.shared.image(key) { return rendered }
        if MathImages.shared.failure(key) == nil { wantedMath.insert(key) }
        return nil
    }

    /// The file an image widget's target names, beside the note or
    /// anywhere in the vault.
    func imageFile(_ target: String) -> URL? {
        if let known = files[target] { return known }
        guard !missingFiles.contains(target) else { return nil }
        guard let path = vault.imageFile(notePath: notePath, target: target) else {
            missingFiles.insert(target)
            return nil
        }
        let url = URL(fileURLWithPath: path)
        files[target] = url
        return url
    }

    /// Notes that the image at `file` should be decoded `pixels` across.
    func wantImage(_ file: URL, pixels: Int) {
        guard VaultImages.shared.image(file, pixels: pixels) == nil else { return }
        wantedImages[file] = pixels
    }

    /// Notes that a link card's preview image should be downloaded.
    func wantCardImage(_ url: URL) {
        wantedCardImages.insert(url)
    }

    /// What styling found missing since the last call.
    func takeWanted() -> WantedMedia {
        defer {
            wantedMath = []
            wantedImages = [:]
            wantedCardImages = []
        }
        return WantedMedia(math: wantedMath, images: wantedImages, cards: wantedCardImages)
    }

    /// Looks for files again, after the vault changed.
    func forgetFiles() {
        files = [:]
        missingFiles = []
    }
}
