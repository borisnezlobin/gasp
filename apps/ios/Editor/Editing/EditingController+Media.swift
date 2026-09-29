import UIKit

/// Math and images arrive after the text: styling notes what it's missing,
/// this asks for it, and the lines that show it are restyled as it comes,
/// a batch at a time.
extension EditingController: MathImagesObserver {
    /// How long arrivals gather before the lines showing them restyle.
    private static let redrawDelay: TimeInterval = 0.08

    /// Asks for every equation in the note as it opens, nearest the cursor
    /// first, so most are ready before they scroll into view.
    func prefetchMath() {
        MathImages.shared.observe(self)
        let farAway = TextRange(start: UInt32.max, end: UInt32.max)
        let cursor = Int(pendingTop ?? textView.selectedRange.location)
        let widgets = document.plan(selection: farAway).lines.flatMap { line in
            line.widgets.compactMap { widget in mathKey(of: widget, in: line).map { (widget, $0) } }
        }
        let nearestFirst = widgets.sorted { abs(Int($0.0.range.start) - cursor) < abs(Int($1.0.range.start) - cursor) }
        MathImages.shared.request(nearestFirst.map(\.1))
    }

    func fetchMissingMedia() {
        let wanted = media.takeWanted()
        MathImages.shared.request(wanted.math)
        for (file, pixels) in wanted.images {
            VaultImages.shared.load(file, pixels: pixels) { [weak self] in
                self?.imagesArrived = true
                self?.scheduleMediaRedraw()
            }
        }
    }

    func mathArrived(_ key: MathKey) {
        arrivedMath.insert(key.tex)
        scheduleMediaRedraw()
    }

    /// Restyles the lines showing what arrived, once for however many
    /// renders finish close together.
    private func scheduleMediaRedraw() {
        guard !redrawQueued else { return }
        redrawQueued = true
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.redrawDelay) { [weak self] in
            guard let self else { return }
            let (math, images) = (arrivedMath, imagesArrived)
            arrivedMath = []
            imagesArrived = false
            redrawQueued = false
            styler.forgetLines { line in
                line.widgets.contains { images && $0.isImage || $0.mathSource.map(math.contains) == true }
            }
            restyle(edited: nil)
        }
    }

    /// The key a math widget renders with, at the size its line's text is.
    private func mathKey(of widget: Widget, in line: LinePlan) -> MathKey? {
        let lineSize = LineStyler.baseLook(of: line, tokens: tokens).size(tokens)
        switch widget.kind {
        case .inlineMath(let tex, let display): return MathKey(tex: tex, display: display, fontSize: lineSize)
        case .mathBlock(let tex): return MathKey(tex: tex, display: true, fontSize: tokens.bodySize)
        default: return nil
        }
    }
}

extension Widget {
    var isImage: Bool {
        if case .image = kind { return true }
        return false
    }

    /// The TeX of the equation the widget draws, if it draws one.
    var mathSource: String? {
        switch kind {
        case .inlineMath(let source, _), .mathBlock(let source), .mathPreview(let source, _): source
        default: nil
        }
    }
}
