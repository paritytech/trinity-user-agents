import Foundation
import Products

final class SPAFlowState {
    let dotNsResolver: DotNsResolverProtocol
    let productImages: ProductImageSources
    let hostProvider: ProductHostProviding
    let productResolver: ProductResolving
    let iconViewModelFactory: ProductIconViewModelMaking

    init(
        dotNsResolver: DotNsResolverProtocol,
        productImages: ProductImageSources,
        hostProvider: ProductHostProviding,
        productResolver: ProductResolving,
        iconViewModelFactory: ProductIconViewModelMaking
    ) {
        self.dotNsResolver = dotNsResolver
        self.productImages = productImages
        self.hostProvider = hostProvider
        self.productResolver = productResolver
        self.iconViewModelFactory = iconViewModelFactory
    }
}
