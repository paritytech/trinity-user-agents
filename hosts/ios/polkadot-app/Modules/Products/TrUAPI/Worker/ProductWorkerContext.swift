import Foundation
import Products

/// What a product's worker is served through, made on first ask and kept for
/// the session.
///
/// The worker itself comes and goes with demand; these do not. A chat session
/// binds the surface whether or not a worker is up, and every execution the
/// product opens carries this same pair, so the product keeps one chat context
/// and one set of routers across restarts.
final class ProductWorkerContext: @unchecked Sendable {
    let chat = ProductChatSurface()
    let routers: ProductRoutersFacadeProtocol = ProductRoutersFacade.worker()
}
