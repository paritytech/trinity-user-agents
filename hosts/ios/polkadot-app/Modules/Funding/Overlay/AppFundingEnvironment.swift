import Coinage
import Foundation
import Products
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

    /// What a payment can spend, as the payment path checks it, on every
    /// balance change. Returns at once when payments are not assembled yet
    /// or the balance service cannot be built; the balance stream follows
    /// the wallet for as long as it runs, so the loop otherwise ends only
    /// with the task's cancellation or a stream failure, which is logged.
    func followSpendable(_ onChange: @escaping @MainActor (Decimal) -> Void) async {
        guard let dependencies: TruApiDependenciesLocator = RootDependencyLocator.getDependency(),
              let support: PaymentsSupport = dependencies.getDependency(),
              let balanceService = try? await support.coinageService.coinageBalanceService()
        else { return }

        do {
            for try await balance in balanceService.balanceStream {
                let spendable = balanceService.denominationContext.decimal(fromPlanks: balance.spendableByPayment)
                onChange(spendable)
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

    func providerPage(providerId: ProductId, path: String, title: String) -> UIViewController? {
        let destination = providerId.rawValue + path
        let flowState = flowStateProvider.flowState()

        // A provider the app ships opens from its own files, under its own id.
        let bundled = bundledProducts.product(providerId)
            .flatMap { $0.hasApp() ? $0 : nil }
        let page = bundled.flatMap { $0.page(navigationDestination: destination) }
            ?? flowState.hostProvider.page(navigationDestination: destination)
        guard let page else { return nil }

        let configuration = SPAConfiguration(
            title: title,
            isRootScreen: false,
            showMoreButton: false,
            page: page,
            contentSource: bundled.map { .bundled($0.appDirectory) } ?? .dotNs
        )
        return SPAViewFactory.createView(configuration: configuration, flowState: flowState)?.controller
    }

    /// Whether a published provider's App is on dotNS. The core answers for
    /// a bundled provider from the bundle.
    func providerHasApp(providerId: ProductId) async -> Bool {
        guard let product = try? await flowStateProvider.flowState().productResolver.resolve(providerId) else {
            return true
        }
        return product.executables.app != nil
    }

    func showNotice(_ message: String) {
        UIWindow.topWindow?.rootViewController?.topModalViewController.showToast(
            message: message,
            type: .error,
            duration: 4
        )
    }

    func fundingActivityChanged() {
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
    private var resolved: [ProductId: FundingProviderBrand] = [:]

    init(flowStateProvider: SPAFlowStateProviding, bundledProducts: BundledProducts = .none) {
        self.flowStateProvider = flowStateProvider
        self.bundledProducts = bundledProducts
    }

    func brand(for providerId: ProductId) async -> FundingProviderBrand {
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
