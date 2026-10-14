import Foundation
import PolkadotUI
import Products

protocol ProductIconViewModelMaking {
    /// An icon for `domain` that resolves itself when a view loads it. The same domain always
    /// produces the same cache key, so browser tabs, chat previews and Settings share one download.
    func createViewModel(for domain: String) -> any ImageViewModelProtocol
}

final class ProductIconViewModelFactory {
    private let dotNsResolver: DotNsResolverProtocol
    private let iconLoader: ProductIconLoading
    private let htmlParser: ProductLinkHTMLParsing

    init(
        dotNsResolver: DotNsResolverProtocol,
        iconLoader: ProductIconLoading,
        htmlParser: ProductLinkHTMLParsing = ProductLinkHTMLParser()
    ) {
        self.dotNsResolver = dotNsResolver
        self.iconLoader = iconLoader
        self.htmlParser = htmlParser
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
}
