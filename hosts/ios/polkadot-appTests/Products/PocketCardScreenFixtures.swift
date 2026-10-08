import Products
import UIKit
@testable import polkadot_app

/// The card and the screen size the tests of an opened card share, so a face
/// sized against one is never checked against another.

let loyaltyCard = PocketCardViewModel(
    key: PocketCardKey(productId: "game.paseo", cardId: PocketCardId(value: "loyalty")),
    title: "Loyalty",
    privileged: false,
    face: nil
)

let screenSize = CGSize(width: 393, height: 800)

/// Puts `screen` in a window, which a card's screen needs before its face may move.
@MainActor
func showing(_ screen: UIViewController) -> UIWindow {
    let window = UIWindow(frame: CGRect(origin: .zero, size: screenSize))
    window.rootViewController = screen
    window.isHidden = false
    window.layoutIfNeeded()
    return window
}
