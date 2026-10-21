import UIKit
import Products
import UIKitExt

@MainActor
final class SPAWireframe: SPAWireframeProtocol, ChatNavigating {
    private let deepLinkHandler: DeferredLinkHandling

    init(deepLinkHandler: DeferredLinkHandling = DeferredLinkHandler.shared) {
        self.deepLinkHandler = deepLinkHandler
    }

    func openChat(
        from view: ControllerBackedProtocol?,
        chatId: Chat.Id
    ) {
        let controller = view?.controller
        let navigation = controller?.navigationController
        if let presentingController = navigation?.presentingViewController {
            presentingController.dismiss(animated: true)
        } else if let controller, navigation?.viewControllers.contains(controller) == true {
            navigation?.popToRootViewController(animated: true)
        }

        navigateToChat(with: chatId, force: false)
    }

    func showProductSPA(from _: ControllerBackedProtocol?, productHost: ProductHost) {
        UIApplication.shared.mainTabBarController?.openProduct(page: ProductPage(host: productHost))
    }

    func openDeeplink(_ url: URL) {
        deepLinkHandler.handle(with: url)
    }

    func showMoreActions(
        from view: ControllerBackedProtocol?,
        actions: [SPAMoreAction],
        closeTitle: String
    ) {
        guard let view else { return }

        let sheet = SPAMoreActionsViewFactory.createView(
            actions: actions,
            closeTitle: closeTitle
        )
        view.controller.present(sheet.controller, animated: true)
    }

    func shareURL(_ url: URL, from view: ControllerBackedProtocol?) {
        guard let view else { return }

        let sheet = ShareViewFactory.createView(items: [.url(url)], host: view)
        view.controller.present(sheet.controller, animated: true)
    }

    func minimize() {
        UIApplication.shared.mainTabBarController?.minimizeSPA()
    }

    func close(tabId: UUID) {
        UIApplication.shared.mainTabBarController?.closeSPA(tabId: tabId)
    }
}
