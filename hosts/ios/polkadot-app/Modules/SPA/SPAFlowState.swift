import Foundation
import IssueMonitoring
import Products

final class SPAFlowState {
    let dotNsResolver: DotNsResolverProtocol
    let productImages: ProductImageSources
    let hostProvider: ProductHostProviding
    let productResolver: ProductResolving
    let contentLoader: ProductContentLoading
    let iconViewModelFactory: ProductIconViewModelMaking
    /// Host load telemetry for everything this flow opens; disabled in builds without reporting.
    let telemetry: HostLoadTelemetry

    init(
        dotNsResolver: DotNsResolverProtocol,
        productImages: ProductImageSources,
        hostProvider: ProductHostProviding,
        productResolver: ProductResolving,
        contentLoader: ProductContentLoading,
        iconViewModelFactory: ProductIconViewModelMaking,
        telemetry: HostLoadTelemetry
    ) {
        self.dotNsResolver = dotNsResolver
        self.productImages = productImages
        self.hostProvider = hostProvider
        self.productResolver = productResolver
        self.contentLoader = contentLoader
        self.iconViewModelFactory = iconViewModelFactory
        self.telemetry = telemetry
    }
}
