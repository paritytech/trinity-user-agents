import Testing
import UIKit
@testable import polkadot_app

/// An open Pocket card is pushed over the Wallet, yet Android shows its tab bar
/// as it does on the Pocket: shown, unfoldable, with the page ending above it,
/// and a tap on the selected tab leaves the card alone.
@MainActor
struct TabBarRootStandInTests {
    @Test
    func showsTheBarOverAStandInAsOverTheTabRoot() {
        let root = UIViewController()
        let standIn = StandInScreen()
        let pushed = UIViewController()

        let treatedAsRoot = [standIn, pushed].map { screen in
            TabBarHiddenPolicy.isTabRoot(in: [root, screen], showing: screen)
        }

        #expect(treatedAsRoot == [true, false])
    }

    @Test
    func leavesAStandInAloneWhenItsTabIsTappedAgain() {
        let standingIn = UINavigationController(rootViewController: UIViewController())
        standingIn.setViewControllers([UIViewController(), StandInScreen()], animated: false)
        let pushed = UINavigationController(rootViewController: UIViewController())
        pushed.setViewControllers([UIViewController(), UIViewController()], animated: false)

        let actions = [standingIn, pushed].map(TabBarReselectionPolicy.action(for:))

        #expect(actions == [.ignore, .popToRoot(pushed)])
    }
}

private final class StandInScreen: UIViewController, TabBarRootStandIn {}
