import UIKit
@testable import polkadot_app

/// A Wallet whose Collectibles peek sits on screen at a fixed frame, for the Collectibles screen to fly from.
@MainActor
final class FakeCollectiblesOrigin: CollectiblesOrigin {
    let collectiblesBookFrame: CGRect? = CGRect(x: 16, y: 420, width: 361, height: 536)

    func setCollectiblesBookLifted() {}
}
