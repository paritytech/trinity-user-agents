import KeyDerivation
import Products
import SubstrateSdk
import UIKit

@MainActor
protocol ModuleNavigating: AnyObject {
    func openChat(_ model: ChatOpenModel)
    func presentModally(_ viewController: UIViewController)
    func presentFullScreen(_ viewController: UIViewController)
    /// Leaves the screen it opens from in the window, for a presentation the
    /// delegate animates over it.
    func presentOverFullScreen(
        _ viewController: UIViewController,
        animatedBy transitioningDelegate: UIViewControllerTransitioningDelegate
    )
    func openProduct(page: ProductPage)
    func presentProduct(page: ProductPage)
    func openPocket()
    func openScanPanel()
}

extension ModuleNavigating {
    func openChat(_ chat: Chat.Id) {
        openChat(.existingChat(chat))
    }
}

final class ModuleNavigator {}

extension ModuleNavigator: ModuleNavigating {
    func presentModally(_ viewController: UIViewController) {
        let navigationController = AppNavigationController(rootViewController: viewController)
        navigationController.modalPresentationStyle = .pageSheet
        UIWindow.topWindow?.topmostViewController?.present(navigationController, animated: true)
    }

    func presentFullScreen(_ viewController: UIViewController) {
        let navigationController = AppNavigationController(rootViewController: viewController)
        navigationController.modalPresentationStyle = .fullScreen
        UIWindow.topWindow?.topmostViewController?.present(navigationController, animated: true)
    }

    func presentOverFullScreen(
        _ viewController: UIViewController,
        animatedBy transitioningDelegate: UIViewControllerTransitioningDelegate
    ) {
        let navigationController = AppNavigationController(rootViewController: viewController)
        navigationController.modalPresentationStyle = .overFullScreen
        navigationController.modalPresentationCapturesStatusBarAppearance = true
        navigationController.view.accessibilityViewIsModal = true
        navigationController.transitioningDelegate = transitioningDelegate

        UIWindow.topWindow?.topmostViewController?.present(navigationController, animated: true)
    }

    func openChat(_ model: ChatOpenModel) {
        // The chat product runs the person chats on this install (HX2): its room link opens the chat with the
        // account, and the product sends a chat request first when it is no contact yet.
        if PersonChatMode.isProductChat, let link = Self.productChatLink(for: model) {
            UIApplication.shared.open(link)
            return
        }

        guard let view = UIApplication.shared.mainTabBarController else {
            return
        }

        PresentedPageDismissal.dismissingPresented(on: view, unless: Self.isPinnedPresentation) { [weak view] in
            guard let view else {
                return
            }
            Self.showChat(model, in: view)
        }
    }

    func openProduct(page: ProductPage) {
        UIApplication.shared.mainTabBarController?.openProduct(page: page)
    }

    func presentProduct(page: ProductPage) {
        UIApplication.shared.mainTabBarController?.presentProduct(page: page)
    }

    func openPocket() {
        guard let view = UIApplication.shared.mainTabBarController else {
            return
        }

        // A card is a modal over the tabs, so the Pocket would be selected unseen.
        view.presentedViewController?.dismiss(animated: true)
        view.select(tab: .wallet)
    }

    func openScanPanel() {
        UIApplication.shared.mainTabBarController?.openScanPanel()
    }
}

@MainActor
private extension ModuleNavigator {
    static func productChatLink(for model: ChatOpenModel) -> URL? {
        let account: AccountId
        switch model {
        case let .existingChat(.person(accountId)): account = accountId
        case let .newRequest(request): account = request.remoteContact.accountId
        case .existingChat: return nil
        }
        guard let tld = DotNsTldProviderFacade.shared.currentTld() else { return nil }
        return ProductChatLink.url(account: account, tld: tld)
    }

    static func isPinnedPresentation(_ controller: UIViewController) -> Bool {
        controller is any LocalAuthViewProtocol
    }

    static func showChat(_ model: ChatOpenModel, in view: MainTabBarViewController) {
        view.select(tab: .chat)
        let tabNavigation = view.view(for: .chat) as? UINavigationController

        if case let .existingChat(chat) = model {
            let existing = tabNavigation?.viewControllers
                .compactMap { $0 as? ChatViewController }
                .first(where: { $0.presenter.chatId == chat })
            if let existing {
                tabNavigation?.popToViewController(existing, animated: true)
                return
            }
        }

        guard
            let contactList = tabNavigation?.viewControllers.first as? ContactsListViewController
        else {
            return
        }
        let contactListPresenter = contactList.presenter as? ContactsListPresenter
        contactListPresenter?.wireframe.showChat(from: contactList, for: model)

        // removing intermediate chats
        guard
            let tabNavigation,
            tabNavigation.viewControllers.count > 2,
            let rootViewController = tabNavigation.viewControllers.first,
            let topViewController = tabNavigation.viewControllers.last
        else {
            return
        }
        tabNavigation.viewControllers = [rootViewController, topViewController]
    }
}

/// The room link that opens the chat product's chat with an account: its home room, with the payload the product
/// answers with that contact's room (`contact:<account hex>`).
enum ProductChatLink {
    static func url(account: AccountId, tld: String) -> URL? {
        URL(string: "polkadot://\(BuiltInProduct.chat(for: tld))/-/chat/chats#contact:\(account.toHex())")
    }
}
