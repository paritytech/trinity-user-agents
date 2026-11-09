import Testing
import UIKit
@testable import polkadot_app

/// An open Pocket card is pushed over the Wallet, yet Android shows its tab bar
/// as it does on the Pocket: shown, unfoldable, with the page ending above it,
/// and a tap on the selected tab pops the card off the stack.
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
    func popsAStandInWhenItsTabIsTappedAgain() {
        let standingIn = UINavigationController(rootViewController: UIViewController())
        standingIn.setViewControllers([UIViewController(), StandInScreen()], animated: false)
        let pushed = UINavigationController(rootViewController: UIViewController())
        pushed.setViewControllers([UIViewController(), UIViewController()], animated: false)

        let actions = [standingIn, pushed].map(TabBarReselectionPolicy.action(for:))

        #expect(actions == [.popToRoot(standingIn), .popToRoot(pushed)])
    }

    @Test
    func collectiblesIsTheTabRootAndPopsWhenItsTabIsTappedAgain() {
        let root = UIViewController()
        let collectibles = CollectiblesViewController(product: StubSPAView())
        let navigation = UINavigationController(rootViewController: root)
        navigation.setViewControllers([root, collectibles], animated: false)

        #expect(TabBarHiddenPolicy.isTabRoot(in: [root, collectibles], showing: collectibles))
        #expect(TabBarReselectionPolicy.action(for: navigation) == .popToRoot(navigation))
    }

    @Test
    func balanceIsTheTabRootAndPopsWhenItsTabIsTappedAgain() {
        let root = UIViewController()
        let balance = AssetDetailsScreenViewController(viewModel: AssetDetailsViewModel(), origin: nil)
        let navigation = UINavigationController(rootViewController: root)
        navigation.setViewControllers([root, balance], animated: false)

        #expect(TabBarHiddenPolicy.isTabRoot(in: [root, balance], showing: balance))
        #expect(TabBarReselectionPolicy.action(for: navigation) == .popToRoot(navigation))
    }
}

private final class StandInScreen: UIViewController, TabBarRootStandIn {}
