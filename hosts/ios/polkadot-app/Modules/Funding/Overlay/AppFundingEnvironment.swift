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
    private let bundledProducts: BundledProducts
    let branding: FundingProviderBranding

    init(
        router: ProductsRouting,
        flowStateProvider: SPAFlowStateProviding = SPAFlowStateProvider(),
        bundledProducts: BundledProducts = .app
    ) {
        self.router = router
        self.flowStateProvider = flowStateProvider
        self.bundledProducts = bundledProducts
        branding = ManifestFundingProviderBranding(
            flowStateProvider: flowStateProvider,
            bundledProducts: bundledProducts
        )
    }

    var cash: FundingCash {
        .current
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

        // A provider the app ships opens from its own files, under its own id.
        let bundled = bundledProducts.product(providerId).flatMap { $0.hasApp() ? $0 : nil }
        let configuration = SPAConfiguration(
            title: title,
            isRootScreen: false,
            showMoreButton: false,
            page: page,
            contentSource: bundled.map { .bundled($0.appDirectory) } ?? .dotNs
        )
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
/// once per provider. A provider the app ships has no manifest read for it.
actor ManifestFundingProviderBranding: FundingProviderBranding {
    private let flowStateProvider: SPAFlowStateProviding
    private let bundledProducts: BundledProducts
    private var resolved: [String: FundingProviderBrand] = [:]

    init(flowStateProvider: SPAFlowStateProviding, bundledProducts: BundledProducts = .none) {
        self.flowStateProvider = flowStateProvider
        self.bundledProducts = bundledProducts
    }

    func brand(for providerId: String) async -> FundingProviderBrand {
        if let brand = resolved[providerId] { return brand }

        guard bundledProducts.product(providerId) == nil else {
            return FundingProviderBrand.placeholder(for: providerId)
        }

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

extension FundingCash {
    /// The app's payment asset.
    @MainActor static var current: FundingCash {
        FundingCash(
            symbol: PaymentAssetBranding.shared.current.symbol,
            precision: PaymentRequestViewFactory.mainChainAsset()?.asset.decimalPrecision ?? 6
        )
    }
}
