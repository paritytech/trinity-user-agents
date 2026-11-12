import Foundation
import PolkadotUI
import Products
import enum TrUAPIHost.ExpandedCardFaceOutcome
import UIKitExt

@MainActor
protocol ExpandedPocketCardViewProtocol: ControllerBackedProtocol {
    var isOnDisplay: Bool { get }

    func navigate(to url: URL)
    func navigate(to page: ProductPage)
    func reload()
    func showLoading()
    func hideLoading()
    func showLoadFailure(_ content: ErrorContent)
    func updateLoadPhase(_ phase: ProductLoadPhase)
    func setFaceShown(_ shown: Bool) -> ExpandedCardFaceOutcome
    func applyOpeningFace(shown: Bool)
}

@MainActor
protocol ExpandedPocketCardPresenterProtocol: AnyObject {
    func setup(engine: JSEngineProtocol)
    func didBecomeVisible()
    func didTapRetry()
    func didInterceptNavigation(to url: URL)
    func didRequestDeeplink(_ url: URL)
}

@MainActor
protocol ExpandedPocketCardInteractorInputProtocol: AnyObject {
    func setup(engine: JSEngineProtocol)
    func retry()
    func checkForNewBuild()
}

@MainActor
protocol ExpandedPocketCardInteractorOutputProtocol: AnyObject {
    func didRequestNavigation(to url: URL)
    func didRequestReload()
    func didUpdateLoadPhase(_ phase: ProductLoadPhase)
    func didFail(error: Error)
    func didRequestFaceShown(_ shown: Bool) -> ExpandedCardFaceOutcome
}

@MainActor
protocol ExpandedPocketCardWireframeProtocol: AnyObject, AlertPresentable, ErrorPresentable {
    func showProductSPA(productHost: ProductHost)
    func openDeeplink(_ url: URL)
}

@MainActor
protocol ExpandedPocketCardRuntimeMaking {
    func makeRuntime(face: any ExpandedCardFaceShowing) throws -> SPARuntimeProtocol
}
