import Foundation
import TrUAPIHost
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
            case "release-worker":
                // S6: the harness gives back its hold, so the worker stops when no room screen holds it.
                TrUAPIE2EWorkerHolds.release(productId: productId)
            case "hold-worker":
                TrUAPIE2EWorkerHolds.hold(productId: productId)
            case "wake":
                // B9: a notification wake of the room, as the receiving layer will start it (07-LATER §10.2). The
                // receiving layer is not on this branch yet (O28), so the harness stands in for it.
                TrUAPIE2EWorkerHolds.wake(
                    productId: productId,
                    reason: .notification(roomId: room, watchId: argument ?? "dev")
                )
            default:
                NotificationCenter.default.post(name: Self.notification, object: self)
            }
        }
    }

    /// The worker holds of the harness, by product, so a scenario can let the worker stop and hold it again.
    @MainActor
    enum TrUAPIE2EWorkerHolds {
        struct Hold {
            let release: @Sendable () async -> Void
            let hold: @Sendable () async throws -> Void
            let wake: @Sendable (ChatWakeReason) async -> Void
        }

        private static var holds: [String: Hold] = [:]

        static func register(_ hold: Hold, productId: String) {
            holds[productId] = hold
        }

        static func release(productId: String) {
            guard let hold = holds[productId] else { return }
            Task { await hold.release() }
        }

        static func hold(productId: String) {
            guard let hold = holds[productId] else { return }
            Task { try? await hold.hold() }
        }

        static func wake(productId: String, reason: ChatWakeReason) {
            guard let hold = holds[productId] else { return }
            Task { await hold.wake(reason) }
        }
    }
#endif
