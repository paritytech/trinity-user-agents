import Foundation
import TrUAPIHost
import UIKit

#if targetEnvironment(simulator)
    /// An action of the user on the screen of a product room, which the truapi E2E harness asks for through the
    /// command feed (P6.10). Each action runs the code path of the gesture that it stands for, from the point after
    /// the gesture: the harness cannot tap the simulator.
    ///
    /// `open`, `list` and `permission` act on the app. Every other action goes to the room screen that shows the
    /// room.
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
            case "release-worker",
                 "hold-worker",
                 "wake":
                runWorkerAction(productId: productId)
            case "permission":
                // The answer of the user to the permission prompt on the screen: `always`, `once` or `deny`.
                TrUAPIE2EPendingPermission.answer(argument)
            case "permission-reset":
                // The core keeps a refusal of `PreimageSubmit` for good. The harness clears it after S58, so that
                // the next run asks again.
                resetPreimageSubmit(productId: productId)
            case "share-open",
                 "share-send",
                 "dismiss":
                TrUAPIE2EShareActions.run(action: action, argument: argument, room: room, productId: productId)
            case "staging-fault":
                // S59: the next stage fails as a failed submit.
                TrUAPIE2EStagingFault.set(.submitFailed(reason: "E2E fault"))
            default:
                NotificationCenter.default.post(name: Self.notification, object: self)
            }
        }
    }

    extension TrUAPIE2EHostAction {
        @MainActor
        private func runWorkerAction(productId: String) {
            switch action {
            case "release-worker":
                // S6: the harness gives back its hold, so the worker stops when no room screen holds it.
                TrUAPIE2EWorkerHolds.release(productId: productId)
            case "hold-worker":
                TrUAPIE2EWorkerHolds.hold(productId: productId)
            default:
                // B9: a notification wake of the room, as the receiving layer will start it (07-LATER §10.2). The
                // receiving layer is not on this branch yet (O28), so the harness stands in for it.
                TrUAPIE2EWorkerHolds.wake(
                    productId: productId,
                    reason: .notification(roomId: room, watchId: argument ?? "dev")
                )
            }
        }

        private func resetPreimageSubmit(productId: String) {
            let execution = ProductPocketService.current?.workers?.currentExecution(of: productId)
            do {
                try execution?.setPermissionAuthorizationStatus(
                    request: .remote(RemotePermissionRequest(permission: .preimageSubmit)),
                    status: .notDetermined
                )
                Logger.shared.debug("[e2e:ui] permission-reset \(productId) \(execution == nil ? "no-worker" : "done")")
            } catch {
                Logger.shared.warning("[e2e:ui] permission-reset \(productId) failed: \(error)")
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
