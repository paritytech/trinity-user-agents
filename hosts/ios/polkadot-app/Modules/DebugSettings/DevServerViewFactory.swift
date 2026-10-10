#if DEBUG
    import Foundation
    import Products
    import TrUAPIHost

    /// Debug-only assembly for a product served from a development server on the developer's
    /// machine. Presentation belongs to ``DebugSettingsWireframe``.
    enum DevServerViewFactory {
        /// Always the rust runtime, like the playground: the core runs the product under its
        /// `localhost[:port]` identifier whichever runtime the debug toggle selects.
        @MainActor
        static func createView(
            product: DevServerProduct,
            flowStateProvider: any SPAFlowStateProviding
        ) async -> SPAViewProtocol? {
            guard let origin = URL(string: product.origin) else {
                return nil
            }

            let flowState = flowStateProvider.flowState()

            // The page's host only gives the screen its structure; the product runs under
            // `productId`. ProductHost rejects the port separator, so it becomes a hyphen.
            let label = "dev-" + product.productId.replacingOccurrences(of: ":", with: "-")

            guard let host = try? await flowState.hostProvider.resolveHost(label: label) else {
                return nil
            }

            let configuration = SPAConfiguration(
                title: product.origin,
                isRootScreen: false,
                showMoreButton: true,
                page: ProductPage(host: host),
                contentSource: .devServer(origin: origin, productId: product.productId)
            )

            return SPAViewFactory.createRustView(
                configuration: configuration,
                flowState: flowState
            )
        }
    }
#endif
