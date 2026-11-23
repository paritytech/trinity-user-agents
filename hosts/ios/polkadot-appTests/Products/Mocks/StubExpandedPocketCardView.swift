import Foundation
import Products
import enum TrUAPIHost.ExpandedCardFaceOutcome
import enum TrUAPIHost.ExpandedCardTurnOutcome
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
    func setFaceShown(_: Bool) -> ExpandedCardFaceOutcome { .applied }
    func setBackShown(_: Bool) -> ExpandedCardTurnOutcome { .applied }
    func applyOpeningFace(shown _: Bool) {}
}
