import UIKit
import UIKitExt

enum PreviewDocumentResult {
    case success
    case failure
}

/// Protocol that provides ability to preview a document using system component
@MainActor
protocol DocumentPreviewPresenting: AnyObject {
    /// Method dependency injection for presenter that will present document preview
    /// - Parameter presenter: View Controller to present document preview
    func use(presenter: ControllerBackedProtocol)
    /// Preview document at given URL using system component
    /// - Parameters:
    ///   - documentURL: URL of the document to preview
    ///   - completion: completion closure that indicates whether preview was successful or failed
    func previewDocument(at documentURL: URL, _ completion: @escaping (PreviewDocumentResult) -> Void)
}

@MainActor
final class DocumentPreviewAdapter: NSObject, DocumentPreviewPresenting {
    private weak var presenter: ControllerBackedProtocol!

    private var existingTintColor: UIColor?
    private let documentInteractionControllerClass: UIDocumentInteractionController.Type

    init(
        documentInteractionControllerClass: UIDocumentInteractionController.Type = UIDocumentInteractionController.self
    ) {
        self.documentInteractionControllerClass = documentInteractionControllerClass
    }

    func use(presenter: ControllerBackedProtocol) {
        self.presenter = presenter
    }

    func previewDocument(at documentURL: URL, _ completion: @escaping (PreviewDocumentResult) -> Void) {
        let documentInteractionController = documentInteractionControllerClass.init(url: documentURL)
        documentInteractionController.delegate = self
        let isPresented = documentInteractionController.presentPreview(animated: true)
        completion(isPresented ? .success : .failure)
    }
}

extension DocumentPreviewAdapter: @MainActor UIDocumentInteractionControllerDelegate {
    func documentInteractionControllerViewControllerForPreview(_: UIDocumentInteractionController)
        -> UIViewController {
        existingTintColor = presenter.controller.navigationController?.navigationBar.tintColor
        presenter.controller.navigationController?.toolbar.tintColor = .systemBlue
        presenter.controller.navigationController?.navigationBar.tintColor = .systemBlue
        return presenter.controller.navigationController ?? presenter.controller
    }

    func documentInteractionControllerViewForPreview(_: UIDocumentInteractionController) -> UIView? {
        presenter.controller.view
    }

    func documentInteractionControllerRectForPreview(_: UIDocumentInteractionController) -> CGRect {
        presenter.controller.view.bounds
    }

    func documentInteractionControllerDidEndPreview(_: UIDocumentInteractionController) {
        presenter.controller.navigationController?.toolbar.tintColor = existingTintColor
        presenter.controller.navigationController?.navigationBar.tintColor = existingTintColor
    }
}
