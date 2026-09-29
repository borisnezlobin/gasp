import SwiftUI
import UIKit

@main
struct GaspApp: App {
    @State private var model = AppModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            BrowserView()
                .environment(model)
                .tint(model.library.tokens.swiftUIColor(\.accent))
                .onAppear {
                    Self.styleNavigationBars(model.library.tokens)
                    model.library.appearance.apply()
                }
                // A change in settings, or one a sync brings in, restyles
                // the app at once.
                .onChange(of: model.library.appearance) { _, appearance in appearance.apply() }
        }
        .onChange(of: scenePhase) { _, phase in
            model.sceneChanged(to: phase)
            // A window made after launch takes the setting too.
            if phase == .active { model.library.appearance.apply() }
        }
        .backgroundTask(.appRefresh(SyncCenter.refreshTaskIdentifier)) {
            await model.sync.syncInBackgroundRefresh()
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
