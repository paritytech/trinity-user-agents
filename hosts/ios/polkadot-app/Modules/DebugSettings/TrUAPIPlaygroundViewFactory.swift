#if DEBUG
    import Foundation
    import Products
    import TrUAPIHost
    import UIKit
    import Keystore_iOS
    import KeyDerivation

    /// Debug-only assembly for the truapi playground (interactive API
    /// explorer). Presentation belongs to ``DebugSettingsWireframe``.
    enum TrUAPIPlaygroundViewFactory {
        static let playgroundName: String = "truapi-playground"
        static let playgroundURL = URL(string: "http://localhost:3000")

        /// The playground as a direct-URL SPA; the shared runtime and its production session are
        /// sourced from the process-wide provider inside `SPAViewFactory.createView`.
        @MainActor
        static func createView(
            flowStateProvider: any SPAFlowStateProviding
        ) -> SPAViewProtocol? {
            guard let url = playgroundURL else {
                return nil
            }

            let flowState = flowStateProvider.flowState()

            guard let host = flowState.hostProvider.host(label: playgroundName) else {
                return nil
            }

            let configuration = SPAConfiguration(
                title: "TrUAPI Playground",
                isRootScreen: false,
                showMoreButton: false,
                page: ProductPage(host: host),
                contentSource: .directURL(url)
            )

            return SPAViewFactory.createView(
                configuration: configuration,
                flowState: flowState
            )
        }
    }
#endif
