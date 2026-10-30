import Foundation
import FoundationExt
import UIKit
import Keystore_iOS
import PolkadotUI
import Products

/// Creates ``ProductBot`` instances for a given product.
///
/// Which runtime drives the bot's chat is decided by the injected
/// ``ChatRuntimeMaking``; this wraps it so a worker release can reload it.
final class ProductBotFactory: @unchecked Sendable {
    private let contentLoader: ProductContentLoading
    private let runtimeFactory: ChatRuntimeMaking
    private let productImages: ProductImageSources
    private let logger: LoggerProtocol

    init(
        contentLoader: ProductContentLoading,
        runtimeFactory: ChatRuntimeMaking,
        productImages: ProductImageSources,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.contentLoader = contentLoader
        self.runtimeFactory = runtimeFactory
        self.productImages = productImages
        self.logger = logger
    }

    func create(resolved: ResolvedProduct) -> ProductBot? {
        guard let runtime = try? ReloadableChatRuntime(
            resolved: resolved,
            runtimeFactory: runtimeFactory,
            contentLoader: contentLoader
        ) else {
            return nil
        }

        let images = productImages.resolver(contentId: { resolved.contentId(for: .worker) })

        return ProductBot(
            product: resolved.product,
            description: resolved.description?.nilIfEmpty,
            runtime: runtime,
            resolveImage: WidgetImageResolver.cached { await images.resolve($0) },
            logger: logger
        )
    }
}
