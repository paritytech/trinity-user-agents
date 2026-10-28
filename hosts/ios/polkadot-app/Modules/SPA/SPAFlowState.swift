import Foundation
import Products

final class SPAFlowState {
    let dotNsResolver: DotNsResolverProtocol
    let productImages: ProductImageSources
    let hostProvider: ProductHostProviding
    let productResolver: ProductResolving
    let contentLoader: ProductContentLoading
    let iconViewModelFactory: ProductIconViewModelMaking

    init(
        dotNsResolver: DotNsResolverProtocol,
        productImages: ProductImageSources,
        hostProvider: ProductHostProviding,
        productResolver: ProductResolving,
        contentLoader: ProductContentLoading,
        iconViewModelFactory: ProductIconViewModelMaking
    ) {
        self.dotNsResolver = dotNsResolver
        self.productImages = productImages
        self.hostProvider = hostProvider
        self.productResolver = productResolver
        self.contentLoader = contentLoader
        self.iconViewModelFactory = iconViewModelFactory
    }
}
