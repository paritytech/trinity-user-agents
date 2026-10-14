import PolkadotUI
import UIKitExt

protocol TopUpErrorViewProtocol: ControllerBackedProtocol {
    func didReceive(title: String, message: String, closeButtonTitle: String)
}

@MainActor
protocol TopUpErrorPresenterProtocol: AnyObject {
    func setup()
    func didTapClose()
}

@MainActor
protocol TopUpErrorWireframeProtocol: AnyObject {
    func dismiss(view: TopUpErrorViewProtocol?)
}
