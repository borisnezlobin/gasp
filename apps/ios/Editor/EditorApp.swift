import SwiftUI
import UIKit

@main
struct EditorApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            BrowserView()
                .environment(model)
                .tint(model.library.tokens.swiftUIColor(\.accent))
                .onAppear { Self.styleNavigationBars(model.library.tokens) }
        }
    }

    /// Navigation titles in the theme's interface font, for the sheets.
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
