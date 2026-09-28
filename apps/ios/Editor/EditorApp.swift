import SwiftUI
import UIKit

@main
struct EditorApp: App {
    @State private var library: VaultLibrary

    init() {
        let library = VaultLibrary()
        _library = State(initialValue: library)
        Self.styleNavigationBars(library.tokens)
    }

    var body: some Scene {
        WindowGroup {
            NoteListView()
                .environment(library)
                .tint(library.tokens.swiftUIColor(\.accent))
        }
    }

    /// Navigation titles in the theme's interface font.
    private static func styleNavigationBars(_ tokens: Tokens) {
        let appearance = UINavigationBar.appearance()
        appearance.titleTextAttributes = [
            .font: tokens.uiFont(size: tokens.bodySize, bold: true),
            .foregroundColor: tokens.color(\.textStrong)
        ]
        appearance.largeTitleTextAttributes = [
            .font: tokens.uiFont(size: tokens.bodySize * CGFloat(tokens.typography.titleScale), bold: true),
            .foregroundColor: tokens.color(\.textStrong)
        ]
    }
}
