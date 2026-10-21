import Foundation
import Products

final class SPAFlowState {
    let dotNsResolver: DotNsResolverProtocol
    let productImages: ProductImageSources
    let hostProvider: ProductHostProviding
    let productResolver: ProductResolving
    let iconViewModelFactory: ProductIconViewModelMaking
    /// Nil where dotNS releases of the chat product are not taken.
    let chatProductReleases: ChatProductReleases?

    init(
        dotNsResolver: DotNsResolverProtocol,
        productImages: ProductImageSources,
        hostProvider: ProductHostProviding,
        productResolver: ProductResolving,
        iconViewModelFactory: ProductIconViewModelMaking,
        chatProductReleases: ChatProductReleases? = nil
    ) {
        self.dotNsResolver = dotNsResolver
        self.productImages = productImages
        self.hostProvider = hostProvider
        self.productResolver = productResolver
        self.iconViewModelFactory = iconViewModelFactory
        self.chatProductReleases = chatProductReleases
    }
}
