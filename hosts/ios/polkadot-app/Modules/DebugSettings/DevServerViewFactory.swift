#if DEBUG
    import Foundation
    import Products
    import TrUAPIHost

    /// Debug-only assembly for a product served from a development server on the developer's
    /// machine. Presentation belongs to ``DebugSettingsWireframe``.
    enum DevServerViewFactory {
        /// Always the rust runtime, like the playground: the core runs the product under the
        /// identifier it gave it, whichever runtime the debug toggle selects.
        /// Throws the core's ``DevServerError`` when `address` is not a development server or its
        /// `product_id` is refused, and the TLD lookup's error when the network has no TLD yet.
        @MainActor
        static func createView(
            address: String,
            flowStateProvider: any SPAFlowStateProviding
        ) async throws -> SPAViewProtocol? {
            let product = try parseDevServer(input: address)

            guard let url = URL(string: product.url) else {
                return nil
            }

            let flowState = flowStateProvider.flowState()

            // The page's host only gives the screen its structure; the product runs under
            // `productId`. `resolveHost(label:)` takes a bare label, so separators become hyphens.
            let label = "dev-" + String(product.productId.map { $0.isLetter || $0.isNumber ? $0 : "-" })

            guard let host = try await flowState.hostProvider.resolveHost(label: label) else {
                return nil
            }

            let configuration = SPAConfiguration(
                title: product.productId,
                isRootScreen: false,
                showMoreButton: true,
                page: ProductPage(host: host),
                contentSource: .devServer(url: url, productId: product.productId)
            )

            return SPAViewFactory.createRustView(
                configuration: configuration,
                flowState: flowState
            )
        }

        /// The core's reason, worded for the developer who typed the address.
        static func message(for error: Error) -> String {
            switch error as? DevServerError {
            case let .NotADevServer(message),
                 let .InvalidProductId(message):
                message
            case nil:
                error.localizedDescription
            }
        }
    }
#endif
