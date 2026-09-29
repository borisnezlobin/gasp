import SwiftUI

/// The templates folder's notes. Picking one types its text at the cursor
/// with the title, date and time filled in.
struct TemplatePicker: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""

    private var tokens: Tokens { model.library.tokens }

    private var templates: [String] {
        let all = model.library.vault?.templates() ?? []
        guard !query.isEmpty else { return all }
        return all.filter { $0.localizedCaseInsensitiveContains(query) }
    }

    var body: some View {
        NavigationStack {
            List(templates, id: \.self) { name in
                Button(name) {
                    dismiss()
                    model.runner.insertTemplate(name)
                }
                .font(Font(tokens.textFont(size: tokens.bodySize)))
                .foregroundStyle(tokens.swiftUIColor(\.textStrong))
            }
            .overlay {
                if templates.isEmpty {
                    ContentUnavailableView(
                        "No templates",
                        systemImage: "doc.badge.plus",
                        description: Text("Notes in the vault's templates folder show up here.")
                    )
                }
            }
            .searchable(text: $query, prompt: "Find a template")
            .navigationTitle("Insert template")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
        }
        .presentationDetents([.medium, .large])
    }
}
