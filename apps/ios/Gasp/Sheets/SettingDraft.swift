import Foundation

/// What a text setting's field holds: the saved value until editing starts,
/// then what's typed, which is saved when editing ends.
struct SettingDraft: Equatable {
    private(set) var text = ""
    private(set) var isEditing = false

    func shown(saved: String) -> String {
        isEditing ? text : saved
    }

    mutating func beginEditing(saved: String) {
        guard !isEditing else { return }
        text = saved
        isEditing = true
    }

    mutating func type(_ typed: String) {
        text = typed
        isEditing = true
    }

    /// Stops editing. Returns the trimmed text when it differs from
    /// `saved`, which is the value to write.
    mutating func endEditing(saved: String) -> String? {
        guard isEditing else { return nil }
        isEditing = false
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        text = saved
        return trimmed == saved ? nil : trimmed
    }
}
