import UIKit
import Products

@MainActor
protocol ModuleNavigating: AnyObject {
    func openChat(_ model: ChatOpenModel)
    func presentModally(_ viewController: UIViewController)
    func presentFullScreen(_ viewController: UIViewController)
    func openProduct(page: ProductPage)
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

    func openChat(_ model: ChatOpenModel) {
        guard let view = UIApplication.shared.mainTabBarController else {
            return
        }

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

    func openProduct(page: ProductPage) {
        guard let tabBar = UIWindow.keyWindow?.rootViewController as? MainTabBarViewController else {
            return
        }

        tabBar.openProduct(page: page)
    }

    func openScanPanel() {
        UIApplication.shared.mainTabBarController?.openScanPanel()
    }
}
