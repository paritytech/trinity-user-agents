import Foundation
import Products
import UIKit

@MainActor
protocol ProductOpening {
    /// False when the product is already on screen and nothing was navigated.
    @discardableResult
    func open(productId: ProductId) -> Bool
}

@MainActor
struct ProductOpener: ProductOpening {
    let navigator: ModuleNavigating

    init(navigator: ModuleNavigating = ModuleNavigator()) {
        self.navigator = navigator
    }

    /// An existing tab keeps its current page; before the tab bar is up the link is deferred.
    @discardableResult
    func open(productId: ProductId) -> Bool {
        let tabBar = UIApplication.shared.mainTabBarController
        guard tabBar?.mountedProductId != productId else {
            return false
        }
        if tabBar?.mountExistingTab(where: { $0.dotDomain == productId }) == true {
            return true
        }

        // Opened directly: the product link is only routed when the products feature is on,
        // and a game product has to open without it.
        if tabBar != nil,
           let host = ProductHostFactory(tldProvider: DotNsTldProviderFacade.shared).host(rawString: productId) {
            navigator.openProduct(page: ProductPage(host: host))
            return true
        }
        guard let url = URL(string: "\(AppConfig.ProductUniversalLink.scheme)://\(productId)") else {
            return false
        }
        DeferredLinkHandler.shared.handle(with: url)
        return true
    }
}
