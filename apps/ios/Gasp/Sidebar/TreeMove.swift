import Foundation

/// A note or a folder picked up in the file tree, to move into a folder.
/// Paths are relative to the vault; `""` is its top.
enum TreeEntry: Equatable {
    case note(String)
    case folder(String)

    var path: String {
        switch self {
        case .note(let path), .folder(let path): path
        }
    }

    /// The folder it's in now.
    var parent: String {
        (path as NSString).deletingLastPathComponent
    }

    /// Whether dropping it on `folder` would move it: not where it already
    /// is, and a folder never into itself or a folder inside it.
    func canMove(into folder: String) -> Bool {
        guard folder != parent else { return false }
        guard case .folder(let moving) = self else { return true }
        return folder != moving && !folder.hasPrefix(moving + "/")
    }
}

enum VaultPaths {
    /// Where `path` ends up once the folder `old` has moved to `new`: the
    /// same place inside it, or `path` itself when it isn't inside it.
    static func path(_ path: String, afterMovingFolder old: String, to new: String) -> String {
        if path == old { return new }
        guard path.hasPrefix(old + "/") else { return path }
        return new + path.dropFirst(old.count)
    }
}

/// One row of a folder outline: a folder and how deep it sits.
struct FolderOutlineRow: Equatable, Identifiable {
    let folder: String
    let depth: Int

    var id: String { folder }
    var name: String { (folder as NSString).lastPathComponent }
}

enum FolderOutline {
    /// Every folder under its parent, siblings in the order Files sorts
    /// them, each as deep as its path is long.
    static func rows(_ folders: [String]) -> [FolderOutlineRow] {
        folders
            .sorted(by: comesBefore)
            .map { FolderOutlineRow(folder: $0, depth: $0.split(separator: "/").count - 1) }
    }

    /// Compares folder by folder, so `A/B` sorts right after `A` and
    /// before `A B`.
    private static func comesBefore(_ first: String, _ second: String) -> Bool {
        let firstParts = first.split(separator: "/").map(String.init)
        let secondParts = second.split(separator: "/").map(String.init)
        for (one, other) in zip(firstParts, secondParts) where one != other {
            return one.localizedStandardCompare(other) == .orderedAscending
        }
        return firstParts.count < secondParts.count
    }
}
