import Foundation
import Observation
import SwiftUI
import UIKit

/// The vault's `encryption.*` settings the phone acts on.
struct EncryptionPreferences {
    var relockAfter: TimeInterval = 120
    var blurWhenAway = true
    var biometricUnlock = false

    init(vault: VaultFolder?) {
        for item in vault?.settings() ?? [] {
            apply(item)
        }
    }

    private mutating func apply(_ item: SettingItem) {
        switch (item.key, item.value) {
        case ("encryption.relock-after-seconds", .integer(let seconds)): relockAfter = TimeInterval(max(seconds, 0))
        case ("encryption.blur-when-away", .bool(let enabled)): blurWhenAway = enabled
        case ("encryption.biometric-unlock", .bool(let enabled)): biometricUnlock = enabled
        default: break
        }
    }

    /// Whether to keep a key behind Face ID or Touch ID after a password
    /// unlocks it.
    var keepsKeys: Bool {
        biometricUnlock && KeptKeys.biometryAvailable
    }
}

/// Covers unlocked encrypted notes while the app isn't in front, so the
/// app switcher's picture of it doesn't show them, and locks them again
/// once it's been away longer than `encryption.relock-after-seconds`.
@Observable
final class NoteLocker {
    private(set) var coversNotes = false
    @ObservationIgnored private var awaySince: Date?
    @ObservationIgnored private var relockTimer: DispatchWorkItem?
    @ObservationIgnored private var backgroundTask = UIBackgroundTaskIdentifier.invalid
    /// Whether an unlocked encrypted note is open in a tab.
    @ObservationIgnored var encryptedNotesOpen: () -> Bool = { false }
    /// Whether the vault may hold keys in memory, which locking forgets.
    @ObservationIgnored var holdsKeys: () -> Bool = { false }
    @ObservationIgnored var preferences: () -> EncryptionPreferences = { EncryptionPreferences(vault: nil) }
    @ObservationIgnored var lock: () -> Void = {}

    /// The app stopped being in front: covered by Control Center or the
    /// app switcher, or gone to the background.
    func wentAway(toBackground: Bool) {
        guard holdsKeys() else { return }
        let preferences = preferences()
        if preferences.blurWhenAway && encryptedNotesOpen() { coversNotes = true }
        if awaySince == nil { awaySince = .now }
        if toBackground { scheduleRelock(after: preferences.relockAfter) }
    }

    /// The app is in front again: the notes lock first if it was away
    /// too long, then the cover lifts.
    func cameBack() {
        relockTimer?.cancel()
        endBackgroundTask()
        if let awaySince, Date.now.timeIntervalSince(awaySince) > preferences().relockAfter {
            lock()
        }
        awaySince = nil
        coversNotes = false
    }

    /// Locks in the background once the time is up, as long as the phone
    /// lets the app run that long; coming back checks again either way.
    private func scheduleRelock(after delay: TimeInterval) {
        relockTimer?.cancel()
        endBackgroundTask()
        backgroundTask = UIApplication.shared.beginBackgroundTask { [weak self] in self?.endBackgroundTask() }
        let timer = DispatchWorkItem { [weak self] in
            self?.lock()
            self?.endBackgroundTask()
        }
        relockTimer = timer
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: timer)
    }

    private func endBackgroundTask() {
        guard backgroundTask != .invalid else { return }
        UIApplication.shared.endBackgroundTask(backgroundTask)
        backgroundTask = .invalid
    }
}

/// Blurs everything under it while `isOn`, with a lock over the blur.
/// It comes on at once, so the app switcher never catches it halfway.
struct AwayCover: ViewModifier {
    let isOn: Bool
    let tokens: Tokens

    func body(content: Content) -> some View {
        content
            .blur(radius: isOn ? 40 : 0, opaque: true)
            .overlay {
                if isOn {
                    ZStack {
                        tokens.swiftUIColor(\.background).opacity(0.6)
                        Image(systemName: "lock.fill")
                            .font(tokens.symbolFont(2.5))
                            .foregroundStyle(tokens.swiftUIColor(\.textMuted))
                    }
                    .ignoresSafeArea()
                    .accessibilityHidden(true)
                }
            }
    }
}
