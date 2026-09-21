import Foundation
import UIKit
import UIKitExt
import UniformTypeIdentifiers
import TrUAPIHost

protocol TrUAPIChatFilePresenting: Sendable {
    @MainActor func selectFiles(request: NativeChatFilePickRequest) async throws -> [URL]
    @MainActor func approveExport(request: NativeChatFileExportRequest) async throws -> Bool
    @MainActor func saveFile(_ url: URL, exportId: String) async throws -> Bool
}

/// Uses the app's top-window presentation convention and Apple's Files UI.
/// Recognized raster/video files retain safe extensions; all other documents
/// use .bin. Nothing is automatically opened, previewed or played.
struct TrUAPIChatFilePresenter: TrUAPIChatFilePresenting {
    @MainActor private static var active: Presentation?

    @MainActor
    func selectFiles(request: NativeChatFilePickRequest) async throws -> [URL] {
        guard request.maxFiles > 0 else { throw ChatFileFailure.invalidRange }
        let approved = try await confirm(
            title: "Send Chat attachments?",
            message: "Product: \(request.productId)\nRecipient: \(request.peerUsername ?? "Chat contact")\nChoose up to \(request.maxFiles) files to send.",
            action: "Choose Files"
        )
        guard approved else { return [] }
        let selected = try await present { operation in
            let picker = UIDocumentPickerViewController(forOpeningContentTypes: [.item], asCopy: true)
            picker.allowsMultipleSelection = request.maxFiles > 1
            picker.delegate = operation
            return picker
        } ?? []
        // Do not silently choose a different subset from what the user selected.
        guard selected.count <= Int(request.maxFiles) else { throw ChatFileFailure.invalidRange }
        return selected
    }

    @MainActor
    func approveExport(request: NativeChatFileExportRequest) async throws -> Bool {
        try await confirm(
            title: "Save Chat attachment?",
            message: "Product: \(request.productId)\nContact: \(request.peerUsername ?? "Chat contact")\nSize: \(request.metadata.sizeBytes) bytes\nThe attachment will be saved as a file, not opened. Only open files you trust.",
            action: "Save to Files"
        )
    }

    @MainActor
    func saveFile(_ url: URL, exportId _: String) async throws -> Bool {
        let result = try await present { operation in
            let picker = UIDocumentPickerViewController(forExporting: [url], asCopy: true)
            picker.delegate = operation
            return picker
        }
        return result != nil
    }

    @MainActor
    private func confirm(title: String, message: String, action: String) async throws -> Bool {
        let result = try await present { operation in
            let alert = UIAlertController(title: title, message: message, preferredStyle: .alert)
            alert.addAction(UIAlertAction(title: "Cancel", style: .cancel) { _ in
                operation.finish(.success(nil))
            })
            alert.addAction(UIAlertAction(title: action, style: .default) { _ in
                operation.finish(.success([]))
            })
            return alert
        }
        return result != nil
    }

    @MainActor
    private func present(
        makeController: (Presentation) -> UIViewController
    ) async throws -> [URL]? {
        try Task.checkCancellation()
        guard Self.active == nil,
              UIApplication.shared.applicationState == .active,
              let parent = UIWindow.topWindow?.topmostViewController,
              parent.viewIfLoaded?.window != nil,
              !parent.isBeingDismissed, !parent.isBeingPresented,
              !(parent is UIAlertController), !(parent is UIDocumentPickerViewController) else {
            throw ChatFileFailure.unavailable
        }
        let operation = Presentation()
        let controller = makeController(operation)
        operation.controller = controller
        Self.active = operation
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                operation.continuation = continuation
                if Task.isCancelled {
                    operation.finish(.failure(CancellationError()))
                } else {
                    parent.present(controller, animated: true)
                    controller.presentationController?.delegate = operation
                }
            }
        } onCancel: {
            Task { @MainActor in operation.finish(.failure(CancellationError())) }
        }
    }

    @MainActor
    private final class Presentation: NSObject, UIDocumentPickerDelegate, UIAdaptivePresentationControllerDelegate {
        var continuation: CheckedContinuation<[URL]?, Error>?
        weak var controller: UIViewController?

        func finish(_ result: Result<[URL]?, Error>) {
            guard let continuation else { return }
            self.continuation = nil
            let complete = {
                if TrUAPIChatFilePresenter.active === self {
                    TrUAPIChatFilePresenter.active = nil
                }
                continuation.resume(with: result)
            }
            if let controller, controller.presentingViewController != nil {
                controller.dismiss(animated: true, completion: complete)
            } else {
                complete()
            }
        }

        func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
            finish(.success(urls))
        }

        func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
            finish(.success(nil))
        }

        func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
            finish(.success(nil))
        }
    }
}
