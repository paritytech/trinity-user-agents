import PolkadotUI
import UIKitExt

protocol PolkadotSigningDetailsViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: PolkadotSigningDetailsViewLayout.ViewModel)
}

@MainActor
protocol PolkadotSigningDetailsPresenterProtocol: AnyObject {
    func setup()
}

@MainActor
protocol PolkadotSigningDetailsWireframeProtocol: AnyObject {
    func hide(view: PolkadotSigningDetailsViewProtocol?)
}
