import SwiftUI

/// One soft glyph of the wordmark, as the website moves them: an offset
/// from its place that a spring pulls back, and a squash that rings like
/// jelly after it's been moved. A finger leans it closer from a distance
/// and pushes it aside up close. Every length scales with the glyph's
/// size.
struct SoftGlyph {
    private static let spring: CGFloat = 90
    private static let damping: CGFloat = 6.6
    private static let squashSpring: CGFloat = 480
    private static let squashDamping: CGFloat = 7.5
    private static let push: CGFloat = 0.34
    private static let pull: CGFloat = 0.07
    private static let reach: CGFloat = 0.45
    private static let squashPerSpeed: CGFloat = 0.07
    private static let mostSquash: CGFloat = 0.32

    let phase: CGFloat
    private(set) var offset = CGSize.zero
    private var velocity = CGSize.zero
    private(set) var squash: CGFloat = 0
    private var squashVelocity: CGFloat = 0
    private(set) var angle: CGFloat = 0

    init(phase: CGFloat) {
        self.phase = phase
    }

    static func near(_ size: CGFloat) -> CGFloat { size * 0.55 + 36 }
    static func far(_ size: CGFloat) -> CGFloat { near(size) * 3 }

    /// How unsettled the glyph is, in its own sizes per second.
    func restlessness(_ size: CGFloat) -> CGFloat {
        hypot(velocity.width, velocity.height) / size
    }

    mutating func step(home: CGPoint, finger: CGPoint?, size: CGFloat, elapsed: CGFloat) {
        let want = wanted(home: home, finger: finger, size: size)
        velocity.width += ((want.width - offset.width) * Self.spring - velocity.width * Self.damping) * elapsed
        velocity.height += ((want.height - offset.height) * Self.spring - velocity.height * Self.damping) * elapsed
        offset.width += velocity.width * elapsed
        offset.height += velocity.height * elapsed
        keepWithinReach(size)
        stepSquash(size: size, elapsed: elapsed)
    }

    /// Where the finger wants the glyph, as an offset from home: away
    /// inside `near`, a little towards it out to `far`.
    private func wanted(home: CGPoint, finger: CGPoint?, size: CGFloat) -> CGSize {
        guard let finger else { return .zero }
        let across = finger.x - (home.x + offset.width)
        let down = finger.y - (home.y + offset.height)
        let distance = max(hypot(across, down), 1)
        let near = Self.near(size)
        let far = Self.far(size)
        guard distance <= far else { return .zero }
        let strength = distance < near
            ? -Self.push * pow(1 - distance / near, 2)
            : Self.pull * sin(.pi * (distance - near) / (far - near))
        return CGSize(width: across / distance * strength * size, height: down / distance * strength * size)
    }

    private mutating func keepWithinReach(_ size: CGFloat) {
        let reach = size * Self.reach
        let out = hypot(offset.width, offset.height)
        guard out > reach else { return }
        offset.width *= reach / out
        offset.height *= reach / out
    }

    private mutating func stepSquash(size: CGFloat, elapsed: CGFloat) {
        let speed = hypot(velocity.width, velocity.height)
        if speed > size * 0.2 { angle = atan2(velocity.height, velocity.width) }
        let target = min(Self.mostSquash, speed / size * Self.squashPerSpeed)
        squashVelocity += ((target - squash) * Self.squashSpring - squashVelocity * Self.squashDamping) * elapsed
        squash += squashVelocity * elapsed
    }

    /// How far the glyph stretches along the way it's moving, breathing a
    /// little while it's still.
    func stretch(at time: Double) -> CGFloat {
        1 + squash + 0.022 * sin(CGFloat(time) * 1.7 + phase)
    }

    /// A slow bob up and down while it's still.
    func bob(at time: Double, size: CGFloat) -> CGFloat {
        size * 0.014 * sin(CGFloat(time) * 1.1 + phase * 2)
    }
}

/// A ring on the water where a finger touched it.
struct WaterRing {
    var center: CGPoint
    var born: Double
    var strength: CGFloat

    static let calm = WaterRing(center: .zero, born: 0, strength: 0)
}

/// What a finger is doing over the hero.
enum FingerPhase {
    case down
    case moved
    case lifted
}

/// Everything on the first screen that moves frame to frame: the soft
/// glyphs, the finger, and the rings on the water. Kept in fixed-size
/// storage and stepped from the screen's timeline, so a frame allocates
/// nothing.
final class LiquidMotion {
    private(set) var glyphs: [SoftGlyph]
    private var rings = [WaterRing](repeating: .calm, count: LiquidHeroMetrics.ringSlots)
    private var nextRing = 0
    private var finger: CGPoint?
    private var lastTrail: CGPoint?
    private var lastDate: Date?
    private var greeted = false
    private let start = Date()
    /// How much the glyphs' unrest stirs the water they're seen through.
    private(set) var stirred: CGFloat = 0

    init(glyphCount: Int) {
        glyphs = (0..<glyphCount).map { SoftGlyph(phase: CGFloat($0) * 1.37) }
    }

    var now: Double { Date().timeIntervalSince(start) }

    /// Steps the glyphs on to `date` and returns the water's time.
    func advance(to date: Date, homes: [CGPoint], size: CGFloat) -> Double {
        let elapsed = CGFloat(min(1.0 / 30, max(0, date.timeIntervalSince(lastDate ?? date))))
        lastDate = date
        var restless: CGFloat = 0
        for index in glyphs.indices where index < homes.count {
            glyphs[index].step(home: homes[index], finger: finger, size: size, elapsed: elapsed)
            restless += glyphs[index].restlessness(size)
        }
        let target = restless / CGFloat(max(glyphs.count, 1)) * 0.05
        stirred += (target - stirred) * min(1, elapsed * 6)
        return date.timeIntervalSince(start)
    }

    /// Follows a finger over the hero. `sea` is the water's frame in the
    /// hero: touching down there sets off a ring, and dragging leaves a
    /// trail of fainter ones.
    func touch(_ point: CGPoint, phase: FingerPhase, sea: CGRect) {
        guard phase != .lifted else {
            finger = nil
            lastTrail = nil
            return
        }
        finger = point
        guard sea.contains(point) else { return }
        let onWater = CGPoint(x: point.x - sea.minX, y: point.y - sea.minY)
        if phase == .down {
            lastTrail = point
            drop(at: onWater, strength: 1)
            return
        }
        if let lastTrail, hypot(point.x - lastTrail.x, point.y - lastTrail.y) < LiquidHeroMetrics.trailSpacing {
            return
        }
        lastTrail = point
        drop(at: onWater, strength: 0.35)
    }

    /// Sets off a ring at `point` on the water.
    func drop(at point: CGPoint, strength: CGFloat) {
        rings[nextRing] = WaterRing(center: point, born: now, strength: strength)
        nextRing = (nextRing + 1) % rings.count
    }

    /// Sets off the one ring that greets the reader, the first time only.
    func greet(at point: CGPoint) {
        guard !greeted else { return }
        greeted = true
        drop(at: point, strength: 0.7)
    }

    /// The ring in `slot` for the water shader at `time`.
    func ring(_ slot: Int, at time: Double) -> Shader.Argument {
        let ring = rings[slot]
        return .float4(ring.center.x, ring.center.y, time - ring.born, ring.strength)
    }
}
