import UIKit
import Products
import UIKitExt

@MainActor
final class SPAWireframe: SPAWireframeProtocol, ChatNavigating {
    func openChat(
        from view: ControllerBackedProtocol?,
        chatId: Chat.Id
    ) {
        if let presentingController = view?.controller.navigationController?.presentingViewController {
            presentingController.dismiss(animated: true)
        } else {
            view?.controller.navigationController?.popToRootViewController(animated: true)
        }

        navigateToChat(with: chatId, force: false)
    }

    func showProductSPA(from _: ControllerBackedProtocol?, productHost: ProductHost) {
        UIApplication.shared.mainTabBarController?.openProduct(page: ProductPage(host: productHost))
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
