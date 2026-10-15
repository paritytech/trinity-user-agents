import Foundation
import Products

/// The foreground worker of the product (VERBOSITY-WASM-PRODUCT I5).
///
/// A product whose Worker executable manifest declares `keepAlive: "foreground"` runs while the app is in the
/// foreground, also with no room screen open, so that it keeps its statement subscriptions. The handler declares it
/// to the worker manager from ``start(messagingSupport:)`` to ``dispose()``. The core holds the worker while the host
/// reports the app in the foreground, so the handler takes no worker request for it.
extension TrUAPIChatHandler {
    func declareForegroundWorker() {
        guard keepAlive == .foreground else { return }

        workers.declareForegroundWorker(for: productId)
    }

    func withdrawForegroundWorker() {
        guard keepAlive == .foreground else { return }

        workers.withdrawForegroundWorker(for: productId)
    }
}
