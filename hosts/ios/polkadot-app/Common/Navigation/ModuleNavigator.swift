import UIKit
import Products

@MainActor
protocol ModuleNavigating: AnyObject {
    func openChat(_ model: ChatOpenModel)
    func presentModally(_ viewController: UIViewController)
    /// Clears what covers the tabs, selects the Wallet tab and hands over its
    /// stack, saying whether the Wallet was already the tab on screen.
    func openWallet(then show: @escaping @MainActor (UINavigationController, _ wasOnScreen: Bool) -> Void)
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

    func openWallet(then show: @escaping @MainActor (UINavigationController, _ wasOnScreen: Bool) -> Void) {
        guard
            let view = UIApplication.shared.mainTabBarController,
            let wallet = view.view(for: .wallet) as? UINavigationController
        else { return }

        let wasOnScreen = view.container.selectedController === wallet
        PresentedPageDismissal.dismissingPresented(on: view, unless: Self.isPinnedPresentation) { [weak view] in
            view?.select(tab: .wallet)
            show(wallet, wasOnScreen)
        }
    }

    func openChat(_ model: ChatOpenModel) {
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

    /// An open card stays open, as the Pocket shows it.
    func openPocket() {
        openWallet { wallet, _ in
            guard !(wallet.topViewController is PocketCardScreenViewController) else { return }

            wallet.popToRootViewController(animated: true)
        }
    }

    func openScanPanel() {
        UIApplication.shared.mainTabBarController?.openScanPanel()
    }
}

@MainActor
private extension ModuleNavigator {
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
