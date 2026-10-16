import Foundation
import UIKit
import UIKitExt

#if targetEnvironment(simulator)
    /// The share actions of the truapi E2E harness. The simulator has no tap tool, so the harness opens the share
    /// sheet of a link, selects a room in it and presses Share through the presenter, as the screen does.
    @MainActor
    enum TrUAPIE2EShareActions {
        /// Runs the share action `action` of the harness: `share-open`, `share-send` or `dismiss`.
        static func run(action: String, argument: String?, room: String, productId: String) {
            switch action {
            case "share-open": open(link: argument)
            case "share-send": send(productId: productId, room: argument ?? room)
            default: dismissTop()
            }
        }

        /// Opens the share sheet of the app for `link`, as the Share entry of a product page does.
        static func open(link: String?) {
            guard
                let link,
                let url = URL(string: link),
                let tabBar = UIApplication.shared.mainTabBarController
            else {
                Logger.shared.warning("[e2e:ui] share-open: no link or no tab bar")
                return
            }

            let sheet = ShareViewFactory.createView(items: [.url(url)], host: tabBar)
            tabBar.controller.topmostPresented.present(sheet.controller, animated: true)
            Logger.shared.debug("[e2e:ui] share-open \(link)")
        }

        /// Selects the room `room` of `productId` in the open share sheet, and presses Share.
        static func send(productId: String, room: String) {
            guard let sheet = UIApplication.shared.mainTabBarController?.controller.topmostPresented
                as? ShareViewController
            else {
                Logger.shared.warning("[e2e:ui] share-send: no share sheet")
                return
            }

            sheet.presenter.didToggleSelection(chatId: .chatExtension(productId, roomId: room), isSelected: true)
            sheet.presenter.didTapShare()
            Logger.shared.debug("[e2e:ui] share-send \(productId) \(room)")
        }

        /// Closes the screen on top, for example the system share sheet that a product opened.
        static func dismissTop() {
            let top = UIApplication.shared.mainTabBarController?.controller.topmostPresented
            top?.dismiss(animated: true)
            Logger.shared.debug("[e2e:ui] dismiss \(top.map { String(describing: type(of: $0)) } ?? "nothing")")
        }
    }
#endif
