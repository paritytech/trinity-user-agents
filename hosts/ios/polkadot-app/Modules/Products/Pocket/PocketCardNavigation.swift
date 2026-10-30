import UIKit

/// Puts a card's screen on the Wallet tab's stack, directly above the Pocket,
/// moving there as Android does: a flight from a Pocket on screen, a crossfade
/// from another open card, and no animation otherwise.
@MainActor
enum PocketCardNavigation {
    static func show(
        _ screen: PocketCardScreenViewController,
        on wallet: UINavigationController,
        animated: Bool,
        then shown: @escaping @MainActor () -> Void
    ) {
        if let transition = wallet.transitionCoordinator {
            transition.animate(alongsideTransition: nil) { _ in
                show(screen, on: wallet, animated: animated, then: shown)
            }
            return
        }

        guard let pocket = wallet.viewControllers.first else { return }

        if animated, wallet.topViewController === pocket {
            wallet.pushViewController(screen, animated: true)
        } else if animated, wallet.topViewController is PocketCardScreenViewController {
            UIView.transition(
                with: wallet.view,
                duration: PocketCardFlightTiming.duration,
                options: .transitionCrossDissolve
            ) {
                wallet.setViewControllers([pocket, screen], animated: false)
            }
        } else {
            wallet.setViewControllers([pocket, screen], animated: false)
        }

        shown()
    }
}
