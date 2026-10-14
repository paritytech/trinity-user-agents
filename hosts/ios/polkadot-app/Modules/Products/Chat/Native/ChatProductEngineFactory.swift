import Foundation
import UIKit
import Products

enum ChatProductEngineFactoryError: Error {
    case invalidProductUrl(productId: ProductId)
}

/// Scheme-handler + WKWebView engine assembly for chat product runtimes,
/// shared by the native (`ProductBotFactory`) and rust chat paths.
enum ChatProductEngineFactory {
    struct EngineContext {
        let productUrl: URL
        let engineFactory: @Sendable () -> JSEngineProtocol
    }

    static func makeContext(
        source: ProductWorkerSource,
        productFileProvider: ChatProductFileProviding,
        logger: LoggerProtocol
    ) throws -> EngineContext {
        let schemeHandler = ProductScriptSchemeHandler(
            productId: source.contentId,
            entryRelativePath: source.entryRelativePath,
            productFileProvider: productFileProvider
        )

        guard let baseURL = schemeHandler.getBaseUrl(),
              let productUrl = schemeHandler.getProductUrl() else {
            throw ChatProductEngineFactoryError.invalidProductUrl(productId: source.contentId)
        }

        let engineFactory: @Sendable () -> JSEngineProtocol = { [logger] in
            WKWebViewJSEngine(
                engineBaseUrl: baseURL,
                scriptHandlers: [
                    JSEngineLogger(logger: logger)
                ],
                hostWindowProvider: { UIWindow.keyWindow },
                urlSchemeHandlers: [
                    ProductScriptSchemeHandler.scheme: schemeHandler
                ],
                logger: logger
            )
        }

        return EngineContext(productUrl: productUrl, engineFactory: engineFactory)
    }
}
