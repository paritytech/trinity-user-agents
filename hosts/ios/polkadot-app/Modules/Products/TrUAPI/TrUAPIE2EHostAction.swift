import Foundation
import UIKit

#if targetEnvironment(simulator)
    /// An action of the user on the screen of a product room, which the truapi E2E harness asks for through the
    /// command feed (P6.10). Each action runs the code path of the gesture that it stands for, from the point after
    /// the gesture: the harness cannot tap the simulator.
    ///
    /// `open` and `list` act on the navigation. Every other action goes to the room screen that shows the room.
    struct TrUAPIE2EHostAction {
        static let notification = Notification.Name("TrUAPIE2EHostAction")
        static let inputTextNotification = Notification.Name("TrUAPIE2ESetInputText")

        let room: String
        let action: String
        let argument: String?
        let target: String?

        @MainActor
        func run(productId: String) {
            switch action {
            case "open":
                // Through the link pipeline of the app, as a tap on a room link does (P6.9).
                let link = argument ?? "polkadot://\(productId)/-/chat/\(room)"
                if let url = URL(string: link) {
                    DeferredLinkHandler.shared.handle(with: url)
                }
            case "list":
                let tabBar = UIApplication.shared.mainTabBarController
                tabBar?.select(tab: .chat)
                (tabBar?.view(for: .chat) as? UINavigationController)?.popToRootViewController(animated: false)
            default:
                NotificationCenter.default.post(name: Self.notification, object: self)
            }
        }
    }
#endif
