import Foundation
import Products

final class SPAFlowState {
    let dotNsResolver: DotNsResolverProtocol
    let ipfsGatewayBaseUrl: URL
    let hostProvider: ProductHostProviding
    let productResolver: ProductResolving
    let iconViewModelFactory: ProductIconViewModelMaking

    init(
        dotNsResolver: DotNsResolverProtocol,
        ipfsGatewayBaseUrl: URL,
        hostProvider: ProductHostProviding,
        productResolver: ProductResolving,
        iconViewModelFactory: ProductIconViewModelMaking
    ) {
        self.dotNsResolver = dotNsResolver
        self.ipfsGatewayBaseUrl = ipfsGatewayBaseUrl
        self.hostProvider = hostProvider
        self.productResolver = productResolver
        self.iconViewModelFactory = iconViewModelFactory
    }
}
