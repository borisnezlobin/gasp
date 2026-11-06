import SwiftUI
import UIKit

/// This iPhone's own SSH key, for a repository it reaches over SSH: a
/// button that copies it, GitHub's page for adding it, and the fingerprint
/// GitHub shows once it's added. The setup screen and the sync settings
/// both show these rows.
struct DeviceKeyRows: View {
    let tokens: Tokens
    @State private var key: DeviceSshKey?
    @State private var problem: String?
    @State private var copied = false

    static let newKeyLink = URL(string: "https://github.com/settings/ssh/new")!

    /// How GitHub's list of keys names this one.
    static var label: String { "Gasp on \(UIDevice.current.model)" }

    var body: some View {
        Button(action: copy) {
            Label(
                copied ? "Copied this iPhone's key" : "Copy this iPhone's key",
                systemImage: copied ? "checkmark" : "doc.on.doc"
            )
            .contentTransition(.symbolEffect(.replace))
        }
        .task { load() }
        Link(destination: Self.newKeyLink) {
            Label("Add a key on GitHub", systemImage: "arrow.up.right.square")
        }
        if let key {
            LabeledContent("GitHub lists it as") {
                Text(key.fingerprint)
                    .font(Font(tokens.uiFont(size: tokens.smallSize)).monospaced())
                    .textSelection(.enabled)
            }
        }
        if let problem {
            ProblemText(message: problem, tokens: tokens)
        }
    }

    private func load() {
        guard key == nil else { return }
        do {
            key = try thisDeviceSshKey(label: Self.label)
            problem = nil
        } catch {
            problem = error.shownMessage
        }
    }

    private func copy() {
        load()
        guard let key else { return }
        UIPasteboard.general.string = key.public
        copied = true
    }
}
