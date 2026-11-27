import Foundation
import PolkadotUI
import Products
import enum TrUAPIHost.ExpandedCardCoverOutcome
import UIKitExt

@MainActor
protocol ExpandedPocketCardViewProtocol: ControllerBackedProtocol {
    var isOnDisplay: Bool { get }
    var origin: PocketOrigin? { get set }

    func navigate(to url: URL)
    func navigate(to page: ProductPage)
    func reload()
    func showLoading()
    func hideLoading()
    func showLoadFailure(_ content: ErrorContent)
    func updateLoadPhase(_ phase: ProductLoadPhase)
    func setCardCovered(_ covered: Bool) -> ExpandedCardCoverOutcome
    func applyOpeningCover(covered: Bool)
}

@MainActor
protocol ExpandedPocketCardPresenterProtocol: AnyObject {
    func setup(engine: JSEngineProtocol)
    func didBecomeVisible()
    func didInterceptNavigation(to url: URL)
    func didRequestDeeplink(_ url: URL)
}

@MainActor
protocol ExpandedPocketCardInteractorInputProtocol: AnyObject {
    func setup(engine: JSEngineProtocol)
    func checkForNewBuild()
}

@MainActor
protocol ExpandedPocketCardInteractorOutputProtocol: AnyObject {
    func didRequestNavigation(to url: URL)
    func didRequestReload()
    func didUpdateLoadPhase(_ phase: ProductLoadPhase)
    func didFail(error: Error)
    func didRequestCardCovered(_ covered: Bool) -> ExpandedCardCoverOutcome
}

@MainActor
protocol ExpandedPocketCardWireframeProtocol: AnyObject, AlertPresentable, ErrorPresentable {
    func showProductSPA(productHost: ProductHost)
    func openDeeplink(_ url: URL)
}

@MainActor
protocol ExpandedPocketCardRuntimeMaking {
    func makeRuntime(card: any ExpandedCardCovering) throws -> SPARuntimeProtocol
}
