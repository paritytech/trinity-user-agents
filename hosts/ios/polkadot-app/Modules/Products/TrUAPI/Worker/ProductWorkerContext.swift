import Foundation
import Products
import UIKitExt

/// What a product's worker is served through, made on first ask and kept for
/// the session.
///
/// The worker itself comes and goes with demand; these do not. A chat session
/// binds the surface whether or not a worker is up, and every execution the
/// product opens carries this same pair, so the product keeps one chat context
/// and one set of routers across restarts.
final class ProductWorkerContext: @unchecked Sendable {
    let chat = ProductChatSurface()
    let routers: ProductRoutersFacadeProtocol

    init(presentationFallback: (@MainActor @Sendable () -> ControllerBackedProtocol?)? = nil) {
        routers = ProductRoutersFacade.worker(fallback: presentationFallback)
    }
}
