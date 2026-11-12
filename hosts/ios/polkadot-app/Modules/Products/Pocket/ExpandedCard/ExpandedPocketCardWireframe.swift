import Products
import UIKit

@MainActor
final class ExpandedPocketCardWireframe: ExpandedPocketCardWireframeProtocol {
    private let deepLinkHandler: DeferredLinkHandling

    init(deepLinkHandler: DeferredLinkHandling = DeferredLinkHandler.shared) {
        self.deepLinkHandler = deepLinkHandler
    }

    func showProductSPA(productHost: ProductHost) {
        UIApplication.shared.mainTabBarController?.openProduct(page: ProductPage(host: productHost))
    }

    func openDeeplink(_ url: URL) {
        deepLinkHandler.handle(with: url)
    }
}
