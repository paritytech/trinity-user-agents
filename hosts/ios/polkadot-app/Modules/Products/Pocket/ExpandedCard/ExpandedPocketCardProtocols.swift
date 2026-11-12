import PolkadotUI
import UIKitExt

protocol ExpandedPocketCardViewProtocol: ControllerBackedProtocol {}

@MainActor
protocol ExpandedPocketCardPresenterProtocol: AnyObject {
    func setup()
}

@MainActor
protocol ExpandedPocketCardInteractorInputProtocol: AnyObject {}

@MainActor
protocol ExpandedPocketCardInteractorOutputProtocol: AnyObject {}

protocol ExpandedPocketCardWireframeProtocol: AnyObject, AlertPresentable, ErrorPresentable {}

@MainActor
protocol ExpandedPocketCardRuntimeMaking {
    func makeRuntime(face: any ExpandedCardFaceShowing) throws -> SPARuntimeProtocol
}
