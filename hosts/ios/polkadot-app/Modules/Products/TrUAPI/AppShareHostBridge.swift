import Foundation
import Products
import TrUAPIHost
import UIKit
import UIKitExt

/// Draws the system share sheet for `share.share`, with the text of the product as its only item. The product learns
/// only whether the user shared the text. The core already checked the tap of a Worker.
final class AppShareHostBridge: ShareHostBridge, @unchecked Sendable {
    private let present: @MainActor (ProductShareSheet) -> Bool

    init(routerFacade: ProductRoutersFacadeProtocol) {
        present = { sheet in routerFacade.productsRouter.present(view: sheet) }
    }

    init(present: @escaping @MainActor (ProductShareSheet) -> Bool) {
        self.present = present
    }

    func shareText(
        productId _: String,
        executionKind _: ProductExecutionKind,
        request: HostShareRequest
    ) async throws -> HostShare {
        let session = await ProductShareSession()
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                Task { @MainActor [present] in
                    guard session.start(continuation) else { return }
                    let sheet = ProductShareSheet(activityItems: [request.text], applicationActivities: nil)
                    sheet.completionWithItemsHandler = { _, completed, _, _ in
                        session.finish(completed ? .shared : .dismissed)
                    }
                    session.sheet = sheet
                    if !present(sheet) {
                        session.fail(ProductShareError.noScreen)
                    }
                }
            }
        } onCancel: {
            Task { @MainActor in session.cancel() }
        }
    }
}

/// The system share sheet that a product opens.
final class ProductShareSheet: UIActivityViewController, ControllerBackedProtocol {}

/// The host has no screen to show the share sheet on.
enum ProductShareError: Error {
    case noScreen
}

/// One open share sheet. It gives its answer one time: the first of the user, a failure or a cancellation wins.
@MainActor
final class ProductShareSession {
    weak var sheet: UIViewController?
    private var continuation: CheckedContinuation<HostShare, Error>?
    private var cancelled = false

    /// Keeps the way back for the answer. Gives `false`, and answers a dismissal, when the core stopped waiting
    /// already: then no sheet opens.
    func start(_ continuation: CheckedContinuation<HostShare, Error>) -> Bool {
        guard !cancelled else {
            continuation.resume(returning: .dismissed)
            return false
        }
        self.continuation = continuation
        return true
    }

    func finish(_ outcome: HostShare) {
        continuation?.resume(returning: outcome)
        continuation = nil
    }

    func fail(_ error: Error) {
        continuation?.resume(throwing: error)
        continuation = nil
    }

    /// The core stopped waiting: the sheet closes, and the answer is a dismissal.
    func cancel() {
        cancelled = true
        sheet?.dismiss(animated: true)
        finish(.dismissed)
    }
}
