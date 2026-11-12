import Foundation
import PolkadotUI
import Products
import enum TrUAPIHost.ExpandedCardFaceOutcome
import UIKitExt

protocol ExpandedPocketCardViewProtocol: ControllerBackedProtocol {}

@MainActor
protocol ExpandedPocketCardPresenterProtocol: AnyObject {
    func setup()
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

protocol ExpandedPocketCardWireframeProtocol: AnyObject, AlertPresentable, ErrorPresentable {}

@MainActor
protocol ExpandedPocketCardRuntimeMaking {
    func makeRuntime(face: any ExpandedCardFaceShowing) throws -> SPARuntimeProtocol
}
