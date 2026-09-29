import Darwin
import QuartzCore
import UIKit

/// Timings for headless performance checks, printed to standard output as
/// `probe <measure> <value>` lines, which `xcrun simctl launch
/// --console-pty` shows. Launch arguments choose what runs:
///
/// - `-probe launch`: from the process starting to the first frame whose
///   page (the start page or a note) is laid out.
/// - `-probe open -probeNote <path>`: from asking for a note to its first
///   laid-out frame, and until every equation it asked for has rendered.
/// - `-probe typing -probeNote <path>`: keystrokes and cursor moves in the
///   middle of the note, each from the key to the text laid out again,
///   then whether the text styled as it changed matches it styled afresh.
///   `-probeTyped <keys>` types other keys.
/// - `-probe scroll -probeNote <path>`: the note scrolled top to bottom at
///   a steady speed, with the time between frames and the work in each.
///
/// - `-probe sync`: from the process starting to the end of the sync
///   the app runs as it opens, for the synced notes.
///
/// Every run ends with `probe memory`, the app's footprint in megabytes.
final class PerformanceProbe {
    static let shared = PerformanceProbe()

    private let mode = UserDefaults.standard.string(forKey: "probe")
    private let notePath = UserDefaults.standard.string(forKey: "probeNote")
    /// How long after launch a probe starts, 2 s or `-probeDelay` if longer.
    private let startDelay = max(UserDefaults.standard.double(forKey: "probeDelay"), 2)
    private var ticker: FrameTicker?
    /// Set once the start page has drawn.
    var startPageShown = false

    var isRunning: Bool { mode != nil }

    /// Starts the probe the launch arguments ask for, once the app is up.
    func start(model: AppModel) {
        switch mode {
        case "launch": watchLaunch(model)
        case "sync": watchLaunchSync(model)
        case "open": after(seconds: startDelay) { self.measureOpen(model) }
        case "typing": after(seconds: startDelay) { self.measureTyping(model) }
        case "scroll": after(seconds: startDelay) { self.measureScroll(model) }
        default: break
        }
    }

    // MARK: Launch and opening

    private func watchLaunch(_ model: AppModel) {
        let started = Self.processStart()
        tick(until: { self.pageIsUsable(model) }, then: {
            Self.report("launch-to-usable-ms", (Self.now() - started) * 1000)
            Self.reportMemory()
        })
    }

    private func watchLaunchSync(_ model: AppModel) {
        let started = Self.processStart()
        tick(until: { model.sync.finishedSyncs > 0 }, then: {
            Self.report("launch-to-synced-ms", (Self.now() - started) * 1000)
            Self.reportMemory()
        })
    }

    private func measureOpen(_ model: AppModel) {
        guard let path = notePath else { return Self.report("error", "no -probeNote") }
        let asked = Self.now()
        model.tabs.openInNewTab(path)
        tick(until: { self.pageIsUsable(model) }, then: {
            Self.report("open-to-usable-ms", (Self.now() - asked) * 1000)
            self.tick(until: { MathImages.shared.pendingCount == 0 }, then: {
                Self.report("open-to-math-rendered-ms", (Self.now() - asked) * 1000)
                Self.reportMemory()
            })
        })
    }

    private func pageIsUsable(_ model: AppModel) -> Bool {
        guard let session = model.tabs.activeSession else { return model.tabs.active.path == nil && startPageShown }
        let textView = session.textView
        guard textView.window != nil, textView.bounds.width > 0 else { return false }
        return textView.textLayoutManager?.textViewportLayoutController.viewportRange != nil
    }

    // MARK: Typing

    private func measureTyping(_ model: AppModel) {
        guard let path = notePath else { return Self.report("error", "no -probeNote") }
        model.tabs.openInNewTab(path)
        after(seconds: 2) {
            guard let session = model.tabs.activeSession else { return Self.report("error", "no note") }
            let textView = session.textView
            textView.becomeFirstResponder()
            let text = textView.textStorage.string as NSString
            let middle = text.range(of: "\n", range: NSRange(location: text.length / 2, length: text.length / 2))
            textView.selectedRange = NSRange(location: middle.location == NSNotFound ? 0 : middle.location, length: 0)
            self.after(seconds: 1) { self.typeKeys(into: session) }
        }
    }

    /// What's typed, or `-probeTyped` to try other keys.
    private static let typed = Array(
        UserDefaults.standard.string(forKey: "probeTyped") ?? "the quick brown fox and a lazy dog "
    )
    private static let keystrokes = 200

    private func typeKeys(into session: EditingController) {
        let textView = session.textView
        var samples: [Double] = []
        var index = 0
        tick(until: {
            let key = String(Self.typed[index % Self.typed.count])
            let started = Self.now()
            textView.insertText(key)
            textView.layoutIfNeeded()
            samples.append((Self.now() - started) * 1000)
            index += 1
            return index >= Self.keystrokes
        }, then: {
            Self.reportSpread("keystroke-ms", samples)
            self.moveCursor(in: session)
        })
    }

    /// Moves the cursor down a line at a time, then back up.
    private func moveCursor(in session: EditingController) {
        let textView = session.textView
        var samples: [Double] = []
        var moves = 0
        tick(until: {
            let text = textView.textStorage.string as NSString
            let here = textView.selectedRange.location
            let target = moves < 50
                ? NSMaxRange(text.lineRange(for: NSRange(location: here, length: 0)))
                : text.lineRange(for: NSRange(location: max(here - 1, 0), length: 0)).location
            let started = Self.now()
            textView.selectedRange = NSRange(location: min(target, text.length), length: 0)
            textView.layoutIfNeeded()
            samples.append((Self.now() - started) * 1000)
            moves += 1
            return moves >= 100
        }, then: {
            Self.reportSpread("cursor-move-ms", samples)
            Self.checkStyling(session)
            Self.reportMemory()
        })
    }

    /// Whether the text styled a line at a time as it changed looks the
    /// same as the text styled afresh from a whole plan.
    private static func checkStyling(_ session: EditingController) {
        let kept = fingerprint(session.textView.textStorage)
        session.use(session.tokens)
        let fresh = fingerprint(session.textView.textStorage)
        let differing = zip(kept, fresh).filter { $0 != $1 }.count + abs(kept.count - fresh.count)
        report("styling-matches-fresh", differing == 0 ? "yes" : "no, \(differing) runs differ")
        guard let first = zip(kept, fresh).first(where: { $0 != $1 }) else { return }
        report("first-difference", "\(first.0) | \(first.1)")
        let text = session.textView.textStorage.string as NSString
        let location = Int(first.0.prefix { $0 != "+" }) ?? 0
        let line = text.lineRange(for: NSRange(location: min(location, text.length), length: 0))
        report("first-difference-line", text.substring(with: line).debugDescription)
    }

    private static func rgba(_ color: UIColor) -> String {
        var (red, green, blue, alpha) = (CGFloat(0), CGFloat(0), CGFloat(0), CGFloat(0))
        color.resolvedColor(with: .current).getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        return String(format: "%.3f %.3f %.3f %.3f", red, green, blue, alpha)
    }

    /// Each run of attributes, as text: where it is and what it holds.
    private static func fingerprint(_ storage: NSTextStorage) -> [String] {
        var runs: [String] = []
        storage.enumerateAttributes(in: NSRange(location: 0, length: storage.length)) { attributes, range, _ in
            let described = attributes.keys.map(\.rawValue).sorted().map { key -> String in
                let value = attributes[NSAttributedString.Key(key)]
                switch value {
                case let font as UIFont: return "\(key)=\(font.fontName) \(font.pointSize)"
                case let color as UIColor: return "\(key)=\(rgba(color))"
                case let style as NSParagraphStyle:
                    return "\(key)=\(style.description.split(separator: "\n").joined(separator: " "))"
                default: return "\(key)=\(type(of: value))"
                }
            }
            runs.append("\(range.location)+\(range.length) \(described.joined(separator: ","))")
        }
        return runs
    }

    // MARK: Scrolling

    /// Points scrolled a second, about a firm flick.
    private static let scrollSpeed: CGFloat = 2000

    private func measureScroll(_ model: AppModel) {
        guard let path = notePath else { return Self.report("error", "no -probeNote") }
        model.tabs.openInNewTab(path)
        after(seconds: 1) {
            guard let textView = model.tabs.activeSession?.textView else { return Self.report("error", "no note") }
            var gaps: [Double] = []
            var work: [Double] = []
            var last: CFTimeInterval?
            self.tick(until: { link in
                if let last { gaps.append((link.timestamp - last) * 1000) }
                last = link.timestamp
                let started = Self.now()
                let bottom = textView.contentSize.height - textView.bounds.height
                let next = min(textView.contentOffset.y + Self.scrollSpeed * CGFloat(link.duration), bottom)
                textView.contentOffset.y = next
                textView.layoutIfNeeded()
                work.append((Self.now() - started) * 1000)
                return next >= bottom
            }, then: {
                Self.reportSpread("scroll-frame-gap-ms", gaps)
                Self.reportSpread("scroll-frame-work-ms", work)
                Self.report("scroll-frames-over-25ms", gaps.filter { $0 > 25 }.count)
                Self.reportMemory()
            })
        }
    }

    // MARK: Frames and time

    /// Runs `step` on every frame until it answers true, then `done`.
    private func tick(until step: @escaping (CADisplayLink) -> Bool, then done: @escaping () -> Void) {
        ticker = FrameTicker { [weak self] link in
            guard step(link) else { return }
            self?.ticker?.stop()
            self?.ticker = nil
            done()
        }
    }

    private func tick(until step: @escaping () -> Bool, then done: @escaping () -> Void) {
        tick(until: { (_: CADisplayLink) in step() }, then: done)
    }

    private func after(seconds: Double, _ work: @escaping () -> Void) {
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds, execute: work)
    }

    private static func now() -> Double {
        var time = timeval()
        gettimeofday(&time, nil)
        return Double(time.tv_sec) + Double(time.tv_usec) / 1_000_000
    }

    /// When this process started, from the kernel.
    private static func processStart() -> Double {
        var info = kinfo_proc()
        var size = MemoryLayout<kinfo_proc>.stride
        var name: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()]
        guard sysctl(&name, 4, &info, &size, nil, 0) == 0 else { return now() }
        let start = info.kp_proc.p_un.__p_starttime
        return Double(start.tv_sec) + Double(start.tv_usec) / 1_000_000
    }

    // MARK: Reporting

    private static func report(_ measure: String, _ value: Any) {
        let line = "probe \(measure) \(value)\n"
        FileHandle.standardOutput.write(Data(line.utf8))
    }

    private static func reportSpread(_ measure: String, _ samples: [Double]) {
        let sorted = samples.sorted()
        guard !sorted.isEmpty else { return report(measure, "none") }
        let median = sorted[sorted.count / 2]
        let p95 = sorted[min(sorted.count * 95 / 100, sorted.count - 1)]
        let summary = String(format: "median %.2f p95 %.2f max %.2f n %d", median, p95, sorted.last ?? 0, sorted.count)
        report(measure, summary)
    }

    private static func reportMemory() {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
        let result = withUnsafeMutablePointer(to: &info) { pointer in
            pointer.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
            }
        }
        guard result == KERN_SUCCESS else { return }
        report("memory-mb", String(format: "%.1f", Double(info.phys_footprint) / 1_048_576))
    }
}

/// Calls back on every frame the screen draws.
private final class FrameTicker: NSObject {
    private var link: CADisplayLink?
    private let onFrame: (CADisplayLink) -> Void

    init(onFrame: @escaping (CADisplayLink) -> Void) {
        self.onFrame = onFrame
        super.init()
        let link = CADisplayLink(target: self, selector: #selector(frame(_:)))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    @objc private func frame(_ link: CADisplayLink) {
        onFrame(link)
    }

    /// The link holds on to its target, so it's let go of here.
    func stop() {
        link?.invalidate()
        link = nil
    }
}
