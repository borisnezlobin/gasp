import UIKit

/// The formats a note exports to.
enum ExportFormat: CaseIterable {
    case pdf
    case webPage

    var title: String {
        switch self {
        case .pdf: "PDF"
        case .webPage: "Web page"
        }
    }
}

/// Exporting and printing: the core lays the note out off the main thread,
/// then the system's share sheet or print panel takes the file.
extension CommandRunner {
    func export(_ path: String, as format: ExportFormat) {
        withSession { session in
            let text = session.document.text()
            let vault = session.vault
            workspace.tell(format == .pdf ? "Laying out the PDF…" : "Making the page…")
            Task.detached(priority: .userInitiated) {
                let file = Result {
                    format == .pdf
                        ? try vault.exportPdf(path: path, text: text)
                        : try vault.exportHtml(path: path, text: text)
                }
                await MainActor.run { self.share(file) }
            }
        }
    }

    func printNote() {
        onNotePath { path in
            guard let session else { return }
            let (text, vault) = (session.document.text(), session.vault)
            workspace.tell("Laying out the page…")
            Task.detached(priority: .userInitiated) {
                let file = Result { try vault.exportPdf(path: path, text: text) }
                await MainActor.run { self.print(file) }
            }
        }
    }

    @MainActor
    private func share(_ file: Result<ExportedFile, Error>) {
        do {
            let exported = try file.get()
            let url = FileManager.default.temporaryDirectory.appending(path: exported.fileName)
            try exported.bytes.write(to: url)
            workspace.sheet = .share(url)
        } catch {
            workspace.tell(error.localizedDescription)
        }
    }

    @MainActor
    private func print(_ file: Result<ExportedFile, Error>) {
        do {
            let exported = try file.get()
            let printer = UIPrintInteractionController.shared
            let info = UIPrintInfo.printInfo()
            info.jobName = exported.fileName
            info.outputType = .general
            printer.printInfo = info
            printer.printingItem = exported.bytes
            printer.present(animated: true)
        } catch {
            workspace.tell(error.localizedDescription)
        }
    }
}
