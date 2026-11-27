import Foundation
import Products
import enum TrUAPIHost.ExpandedCardCoverOutcome
import UIKit
import UIKitExt
@testable import polkadot_app

@MainActor
final class StubExpandedPocketCardView: ExpandedPocketCardViewProtocol {
    var isOnDisplay = true
    weak var origin: PocketOrigin?

    let controller = UIViewController()
    var isSetup: Bool { true }

    func navigate(to _: URL) {}
    func navigate(to _: ProductPage) {}
    func reload() {}
    func showLoading() {}
    func hideLoading() {}
    func showLoadFailure(_: ErrorContent) {}
    func updateLoadPhase(_: ProductLoadPhase) {}
    func setCardCovered(_: Bool) -> ExpandedCardCoverOutcome { .applied }
    func applyOpeningCover(covered _: Bool) {}
}
