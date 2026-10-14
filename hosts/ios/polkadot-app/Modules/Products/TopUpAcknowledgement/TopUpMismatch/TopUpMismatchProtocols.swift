import PolkadotUI
import UIKitExt

protocol TopUpMismatchViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: any TopUpMismatchViewModelProtocol)
}

@MainActor
protocol TopUpMismatchPresenterProtocol: AnyObject {
    func setup()
    func didTapClose()
}

@MainActor
protocol TopUpMismatchWireframeProtocol: AnyObject {
    func dismiss(view: TopUpMismatchViewProtocol?)
}
