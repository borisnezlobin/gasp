import Foundation
import LocalAuthentication
import Security

/// Keys of encrypted notes kept in this iPhone's Keychain behind Face ID
/// or Touch ID, named by the core's key ids. They never leave the device
/// and stop working when the enrolled faces or fingers change.
enum KeptKeys {
    private static let service = "Gasp encryption"
    /// Keychain writes, in order and off the main thread.
    private static let writes = DispatchQueue(label: "com.borisnezlobin.gasp.kept-keys", qos: .userInitiated)

    /// Whether this iPhone can unlock with Face ID or Touch ID now.
    static var biometryAvailable: Bool {
        LAContext().canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: nil)
    }

    /// "Face ID" or "Touch ID", for buttons.
    static var biometryName: String {
        let context = LAContext()
        _ = context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: nil)
        return context.biometryType == .touchID ? "Touch ID" : "Face ID"
    }

    static var biometrySymbol: String {
        biometryName == "Touch ID" ? "touchid" : "faceid"
    }

    /// Keeps `key`, replacing one kept under the same id, off the main thread.
    static func keep(_ key: KeptKey) {
        writes.async {
            guard let access = SecAccessControlCreateWithFlags(
                nil, kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly, .biometryCurrentSet, nil
            ) else { return }
            SecItemDelete(query(id: key.id) as CFDictionary)
            var item = query(id: key.id)
            item[kSecValueData as String] = Data(key.bytes)
            item[kSecAttrAccessControl as String] = access
            SecItemAdd(item as CFDictionary, nil)
        }
    }

    /// Whether a key is kept under `id`, asked without showing Face ID,
    /// so it answers at once and never waits on the person.
    static func has(id: String) -> Bool {
        let context = LAContext()
        context.interactionNotAllowed = true
        var lookup = query(id: id)
        lookup[kSecUseAuthenticationContext as String] = context
        let status = SecItemCopyMatching(lookup as CFDictionary, nil)
        return status == errSecSuccess || status == errSecInteractionNotAllowed
    }

    /// The key kept under `id`, once Face ID or Touch ID agrees. Nil when
    /// the person cancels or there's none. Asking blocks its thread while
    /// Face ID shows, so it runs after any waiting write, off the main thread.
    static func read(id: String, reason: String) async -> Data? {
        await withCheckedContinuation { continuation in
            writes.async {
                let context = LAContext()
                context.localizedReason = reason
                var lookup = query(id: id)
                lookup[kSecUseAuthenticationContext as String] = context
                lookup[kSecReturnData as String] = true
                var found: CFTypeRef?
                let status = SecItemCopyMatching(lookup as CFDictionary, &found)
                continuation.resume(returning: status == errSecSuccess ? found as? Data : nil)
            }
        }
    }

    static func forget(id: String) {
        writes.async { SecItemDelete(query(id: id) as CFDictionary) }
    }

    private static func query(id: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: id
        ]
    }
}
