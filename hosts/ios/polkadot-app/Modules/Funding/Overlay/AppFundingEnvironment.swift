import Foundation
import Products
import UIKit
import UIKitExt

/// The app around the funding overlay: the payment asset, the balance the
/// CASH card last showed, products as the app opens them, and the shared
/// router the host's own prompts present from.
@MainActor
final class AppFundingEnvironment: FundingOverlayEnvironment {
    private let router: ProductsRouting
    private let flowStateProvider: SPAFlowStateProviding
    let branding: FundingProviderBranding

    init(router: ProductsRouting, flowStateProvider: SPAFlowStateProviding = SPAFlowStateProvider()) {
        self.router = router
        self.flowStateProvider = flowStateProvider
        branding = ManifestFundingProviderBranding(flowStateProvider: flowStateProvider)
    }

    var cash: FundingCash {
        FundingCash(
            symbol: PaymentAssetBranding.shared.current.symbol,
            precision: PaymentRequestViewFactory.mainChainAsset()?.asset.decimalPrecision ?? 6
        )
    }

    var spendable: Decimal? {
        FundingActivityCenter.shared.spendable
    }

    func present(_ controller: UIViewController) -> Bool {
        router.present(view: FundingPresentedController(controller: controller))
    }

    func providerPage(providerId: String, route: String, title: String) -> UIViewController? {
        let destination = route.hasPrefix("/") || route.hasPrefix("#") ? providerId + route : providerId + "/" + route
        let flowState = flowStateProvider.flowState()
        guard let page = flowState.hostProvider.page(navigationDestination: destination) else { return nil }

        let configuration = SPAConfiguration(title: title, isRootScreen: false, showMoreButton: false, page: page)
        return SPAViewFactory.createView(configuration: configuration, flowState: flowState)?.controller
    }

    func fundingSessionChanged(intent _: String) {
        FundingActivityCenter.shared.refresh()
    }
}

/// Hands a plain controller to the router, which presents views.
private final class FundingPresentedController: ControllerBackedProtocol {
    let controller: UIViewController

    init(controller: UIViewController) {
        self.controller = controller
    }

    var isSetup: Bool { controller.isViewLoaded }
}

/// A provider's name and icon from its product's root manifest, resolved
/// once per provider.
actor ManifestFundingProviderBranding: FundingProviderBranding {
    private let flowStateProvider: SPAFlowStateProviding
    private var resolved: [String: FundingProviderBrand] = [:]

    init(flowStateProvider: SPAFlowStateProviding) {
        self.flowStateProvider = flowStateProvider
    }

    func brand(for providerId: String) async -> FundingProviderBrand {
        if let brand = resolved[providerId] { return brand }

        let resolver = flowStateProvider.flowState().productResolver
        let name = try? await resolver.resolve(providerId).displayName
        let iconData = await ProductIconLoader(productResolver: resolver).loadIcon(for: providerId)

        let brand = FundingProviderBrand(
            name: name ?? FundingProviderBrand.placeholder(for: providerId).name,
            icon: iconData.flatMap(UIImage.init(data:))
        )
        resolved[providerId] = brand
        return brand
    }
}
