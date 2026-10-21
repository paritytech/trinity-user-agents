import PhotosUI
import UIKit
import UniformTypeIdentifiers

/// Presents the photo picker or the document picker for one attachment of a product room, and reads the bytes.
@MainActor
final class ProductAttachmentPicker: NSObject {
    private var completion: ((PickedAttachment) -> Void)?
    private var kind: ChatAttachmentKind = .photo
    private let logger: LoggerProtocol

    init(logger: LoggerProtocol = Logger.shared) {
        self.logger = logger
    }

    func present(
        _ kind: ChatAttachmentKind,
        from controller: UIViewController,
        completion: @escaping (PickedAttachment) -> Void
    ) {
        self.kind = kind
        self.completion = completion

        switch kind {
        case .photo,
             .video:
            var configuration = PHPickerConfiguration()
            configuration.selectionLimit = 1
            configuration.preferredAssetRepresentationMode = .current
            configuration.filter = kind == .photo ? .images : .videos
            let picker = PHPickerViewController(configuration: configuration)
            picker.delegate = self
            controller.present(picker, animated: true)
        case .file:
            let picker = UIDocumentPickerViewController(forOpeningContentTypes: [.item], asCopy: true)
            picker.delegate = self
            picker.allowsMultipleSelection = false
            controller.present(picker, animated: true)
        }
    }

    private func finish(_ picked: PickedAttachment?) {
        let completion = completion
        self.completion = nil
        if let picked {
            completion?(picked)
        }
    }
}

extension ProductAttachmentPicker: PHPickerViewControllerDelegate {
    nonisolated func picker(_ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]) {
        MainActor.assumeIsolated {
            picker.dismiss(animated: true)
            guard let provider = results.first?.itemProvider else {
                finish(nil)
                return
            }

            let type: UTType = kind == .photo ? .image : .movie
            provider.loadFileRepresentation(forTypeIdentifier: type.identifier) { [weak self] url, error in
                let data = url.flatMap { try? Data(contentsOf: $0) }
                let mimeType = url.flatMap { UTType(filenameExtension: $0.pathExtension)?.preferredMIMEType }
                Task { @MainActor in
                    guard let self else { return }
                    guard let data else {
                        self.logger.warning("Attachment did not load: \(String(describing: error))")
                        self.finish(nil)
                        return
                    }
                    self.finish(self.kind == .photo ? .image(data) : .video(data, mimeType: mimeType ?? "video/mp4"))
                }
            }
        }
    }
}

extension ProductAttachmentPicker: UIDocumentPickerDelegate {
    nonisolated func documentPicker(_: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        let picked = urls.first.flatMap { url in
            (try? Data(contentsOf: url)).map { PickedAttachment.file($0, name: url.lastPathComponent) }
        }
        MainActor.assumeIsolated { finish(picked) }
    }

    nonisolated func documentPickerWasCancelled(_: UIDocumentPickerViewController) {
        MainActor.assumeIsolated { finish(nil) }
    }
}
