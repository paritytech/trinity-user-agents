import Foundation
import Products
import UIKit

@MainActor
protocol ProductOpening {
    func open(productId: ProductId)
}

@MainActor
struct ProductOpener: ProductOpening {
    let navigator: ModuleNavigating

    init(navigator: ModuleNavigating = ModuleNavigator()) {
        self.navigator = navigator
    }

    /// An existing tab keeps its current page; before the tab bar is up the link is deferred.
    func open(productId: ProductId) {
        let tabBar = UIApplication.shared.mainTabBarController
        guard tabBar?.mountedProductId != productId else {
            return
        }
        if tabBar?.mountExistingTab(where: { $0.dotDomain == productId }) == true {
            return
        }

        // Opened directly: the product link is only routed when the products feature is on,
        // and a game product has to open without it.
        if tabBar != nil,
           let host = ProductHostFactory(tldProvider: DotNsTldProviderFacade.shared).host(rawString: productId) {
            navigator.openProduct(page: ProductPage(host: host))
        } else if let url = URL(string: "\(AppConfig.ProductUniversalLink.scheme)://\(productId)") {
            DeferredLinkHandler.shared.handle(with: url)
        }
    }
}
