import Foundation
import Products
import UIKitExt

/// One access point for the routers a product context needs. A single
/// `setPresentationView` fans the anchor out to both routers — no router
/// presents on the topmost view controller on its own.
///
/// `productsRouter` is exposed concretely because one `ProductsRouter` instance
/// serves three protocol lenses: app flows call its `ProductsRouting`
/// extensions, while the package-side requesters consume it as
/// `ProductPermissionRouting` / `AllowancePromptRouting`.
protocol ProductRoutersFacadeProtocol: AnyObject {
    var productsRouter: ProductsRouter { get }
    var navigationRouter: ProductsNavigationRouting { get }

    @MainActor func setPresentationView(_ view: ControllerBackedProtocol)
}

final class ProductRoutersFacade: ProductRoutersFacadeProtocol {
    let productsRouter: ProductsRouter
    let navigationRouter: ProductsNavigationRouting

    init(
        productsRouter: ProductsRouter,
        navigationRouter: ProductsNavigationRouting
    ) {
        self.productsRouter = productsRouter
        self.navigationRouter = navigationRouter
    }

    @MainActor
    func setPresentationView(_ view: ControllerBackedProtocol) {
        productsRouter.setPresentationView(view)
        navigationRouter.setPresentationView(view)
    }
}

// MARK: - Context compositions

extension ProductRoutersFacade {
    /// Worker context: navigation opens the target product page in the main tab
    /// bar (or an external URL), mirroring the SPA context.
    static func worker() -> ProductRoutersFacade {
        make(navigationRouter: ProductsNavigationRouter())
    }

    /// SSO context: prompts anchor to the view the sign-in host receives via
    /// the service coordinator (the main tab bar); until it is attached,
    /// prompts deliver denial.
    static func sso() -> ProductRoutersFacade {
        make(navigationRouter: ForbiddenNavigationRouter())
    }

    /// SPA context: navigation opens further product pages within the flow.
    @MainActor
    static func spa() -> ProductRoutersFacade {
        make(navigationRouter: ProductsNavigationRouter())
    }
}

private extension ProductRoutersFacade {
    static func make(navigationRouter: ProductsNavigationRouting) -> ProductRoutersFacade {
        ProductRoutersFacade(
            productsRouter: ProductsRouter(),
            navigationRouter: navigationRouter
        )
    }
}
