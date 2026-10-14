import Foundation
import PolkadotUI
import Products

protocol ProductIconViewModelMaking {
    /// An icon for `domain` that resolves itself when a view loads it. The same domain always
    /// produces the same cache key, so browser tabs, chat previews and Settings share one download.
    func createViewModel(for domain: String) -> any ImageViewModelProtocol

    /// An icon that a product gives as a chat resource source, or nil when the source is not a resource.
    func createViewModel(forResource source: String) -> (any ImageViewModelProtocol)?

    /// An image that a product gives as a chat resource, for example the avatar of a room.
    func createViewModel(forChatResource resource: ChatResource) -> any ImageViewModelProtocol
}

final class ProductIconViewModelFactory {
    private let dotNsResolver: DotNsResolverProtocol
    private let iconLoader: ProductIconLoading
    private let htmlParser: ProductLinkHTMLParsing
    private let resources: ChatResourceResolving

    init(
        dotNsResolver: DotNsResolverProtocol,
        iconLoader: ProductIconLoading,
        htmlParser: ProductLinkHTMLParsing = ProductLinkHTMLParser(),
        resources: ChatResourceResolving = ChatResourceResolver.shared
    ) {
        self.dotNsResolver = dotNsResolver
        self.iconLoader = iconLoader
        self.htmlParser = htmlParser
        self.resources = resources
    }
}

extension ProductIconViewModelFactory: ProductIconViewModelMaking {
    func createViewModel(for domain: String) -> any ImageViewModelProtocol {
        ProductIconImageViewModel(
            provider: ProductIconImageDataProvider(
                domain: domain,
                dotNsResolver: dotNsResolver,
                iconLoader: iconLoader,
                htmlParser: htmlParser
            )
        )
    }

    func createViewModel(forResource source: String) -> (any ImageViewModelProtocol)? {
        ChatResource(source: source).map(createViewModel(forChatResource:))
    }

    func createViewModel(forChatResource resource: ChatResource) -> any ImageViewModelProtocol {
        ChatResourceImageViewModel(resource: resource, resolver: resources)
    }
}
