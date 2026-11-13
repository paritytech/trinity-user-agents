import Foundation
import Products
import enum TrUAPIHost.ExpandedCardFaceOutcome
import UIKit
import UIKitExt
@testable import polkadot_app

@MainActor
final class StubExpandedPocketCardView: ExpandedPocketCardViewProtocol {
    enum Event: Equatable {
        case navigate(URL)
        case reload
        case showLoading
        case hideLoading
        case showLoadFailure(title: String)
        case updateLoadPhase(ProductLoadPhase)
    }

    private(set) var events: [Event] = []
    private(set) var navigatedPages: [ProductPage] = []
    private(set) var faceRequests: [Bool] = []
    var faceOutcome: ExpandedCardFaceOutcome = .applied
    var isOnDisplay = true
    weak var origin: PocketOrigin?

    let controller = UIViewController()
    var isSetup: Bool { true }

    func navigate(to url: URL) { events.append(.navigate(url)) }
    func navigate(to page: ProductPage) { navigatedPages.append(page) }
    func reload() { events.append(.reload) }
    func showLoading() { events.append(.showLoading) }
    func hideLoading() { events.append(.hideLoading) }
    func showLoadFailure(_ content: ErrorContent) { events.append(.showLoadFailure(title: content.title)) }
    func updateLoadPhase(_ phase: ProductLoadPhase) { events.append(.updateLoadPhase(phase)) }

    func setFaceShown(_ shown: Bool) -> ExpandedCardFaceOutcome {
        faceRequests.append(shown)
        return faceOutcome
    }

    func applyOpeningFace(shown _: Bool) {}
}
