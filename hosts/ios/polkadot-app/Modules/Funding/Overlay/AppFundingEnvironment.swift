import Coinage
import Foundation
import Products
import SubstrateSdk
import TrUAPIHost
import UIKit
import UIKitExt

/// The app around the funding overlay: the payment asset, the balance a
/// payment can spend, products as the app opens them, and the shared
/// router the host's own prompts present from.
@MainActor
final class AppFundingEnvironment: FundingOverlayEnvironment {
    private let router: ProductsRouting
    private let flowStateProvider: SPAFlowStateProviding
    let branding: FundingProviderBranding

    init(
        router: ProductsRouting,
        flowStateProvider: SPAFlowStateProviding = SPAFlowStateProvider()
    ) {
        self.router = router
        self.flowStateProvider = flowStateProvider
        branding = ManifestFundingProviderBranding(flowStateProvider: flowStateProvider)
    }

    var cashInfo: AssetBalanceDisplayInfo {
        .digitalDollarFigure(assetPrecision: PaymentRequestViewFactory.mainChainAsset()?.asset.decimalPrecision ?? 6)
    }

    var cashSymbol: String {
        PaymentAssetBranding.shared.current.symbol
    }

    private(set) lazy var iconLoader: FundingIconLoader? = {
        let flowState = flowStateProvider.flowState()
        return FundingIconLoader.make(dotNsResolver: flowState.dotNsResolver, products: flowState.productResolver)
    }()

    /// What a payment can spend, as the payment path checks it, on every
    /// balance change. Returns at once when payments are not assembled yet
    /// or the balance service cannot be built; the balance stream follows
    /// the wallet for as long as it runs, so the loop otherwise ends only
    /// with the task's cancellation or a stream failure, which is logged.
    func followSpendable(_ onChange: @escaping @MainActor (Balance) -> Void) async {
        guard let dependencies: TruApiDependenciesLocator = RootDependencyLocator.getDependency(),
              let support: PaymentsSupport = dependencies.getDependency(),
              let balanceService = try? await support.coinageService.coinageBalanceService()
        else { return }

        do {
            for try await balance in balanceService.balanceStream {
                onChange(balance.spendableByPayment)
            }
        } catch {
            if !Task.isCancelled {
                Logger.shared.error("[funding] following the spendable balance failed: \(error)")
            }
        }
    }

    func present(_ controller: UIViewController) -> Bool {
        router.present(view: FundingPresentedController(controller: controller))
    }

    func providerPage(providerId: ProductId, route: String, title: String) -> UIViewController? {
        let path = route.hasPrefix("/") || route.hasPrefix("#") ? route : "/" + route
        let destination = providerId.rawValue + path
        let flowState = flowStateProvider.flowState()

        guard let page = flowState.hostProvider.page(navigationDestination: destination) else { return nil }

        let configuration = SPAConfiguration(
            title: title,
            isRootScreen: false,
            showMoreButton: false,
            page: page
        )
        return SPAViewFactory.createView(configuration: configuration, flowState: flowState)?.controller
    }

    func showNotice(_ message: String) {
        UIWindow.topWindow?.rootViewController?.topModalViewController.showToast(
            message: message,
            type: .error,
            duration: 4
        )
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

/// A provider's name and icon from its product's published root manifest,
/// resolved once per provider.
actor ManifestFundingProviderBranding: FundingProviderBranding {
    private let flowStateProvider: SPAFlowStateProviding
    private var resolved: [ProductId: FundingProviderBrand] = [:]

    init(flowStateProvider: SPAFlowStateProviding) {
        self.flowStateProvider = flowStateProvider
    }

    func brand(for providerId: ProductId) async -> FundingProviderBrand {
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
